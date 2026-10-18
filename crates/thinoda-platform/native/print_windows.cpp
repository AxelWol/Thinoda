#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <roapi.h>
#include <printmanagerinterop.h>
#include <DocumentSource.h>
#include <PrintPreview.h>
#include <d3d11.h>
#include <d2d1_1.h>
#include <wincodec.h>
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Graphics.Printing.h>
#include <algorithm>
#include <atomic>
#include <cmath>
#include <memory>
#include <mutex>
#include <vector>
#include "print.h"

using namespace winrt;
using namespace winrt::Windows::Foundation;
using namespace winrt::Windows::Graphics::Printing;
using AsyncState = winrt::Windows::Foundation::AsyncStatus;

namespace {
struct Session {
    handle finished{CreateEventW(nullptr, TRUE, FALSE, nullptr)};
    std::atomic<int> result{-2};
    std::atomic<HRESULT> failure{S_OK};
    PrintTask task{nullptr};
    event_token completed{};

    void fail(HRESULT code) {
        HRESULT expected = S_OK;
        failure.compare_exchange_strong(expected, code);
    }
    void finish(int code) {
        int expected = -2;
        if (result.compare_exchange_strong(expected, code)) SetEvent(finished.get());
    }
    void complete(PrintTaskCompletion completion) {
        switch (completion) {
        case PrintTaskCompletion::Submitted: finish(1); break;
        case PrintTaskCompletion::Canceled:
        case PrintTaskCompletion::Abandoned: finish(0); break;
        default: fail(E_FAIL); finish(-1); break;
        }
    }
    int outcome() const {
        int code = result.load();
        HRESULT error = failure.load();
        // Cancellation can interrupt MakeDocument. Do not turn an OS-confirmed
        // user abort into an error, or hide a genuine drawing/driver failure.
        if (code == 0 && (error == E_ABORT || error == HRESULT_FROM_WIN32(ERROR_CANCELLED) ||
                          error == HRESULT_FROM_WIN32(ERROR_PRINT_CANCELLED)))
            return 0;
        check_hresult(error);
        return code;
    }
};

// The OS may retain/call the source after its UI closes. Own the pixels, serialize
// graphics callbacks, and explicitly disconnect instead of retaining Rust memory.
struct PageSource : implements<PageSource, IPrintDocumentSource,
                               IPrintDocumentPageSource, IPrintPreviewPageCollection> {
    std::mutex mutex;
    std::shared_ptr<Session> session;
    std::vector<unsigned char> pixels;
    int width, height, dpi;
    double width_mm, height_mm;
    bool closed = false;
    PrintPageDescription description{};
    com_ptr<IPrintPreviewDxgiPackageTarget> preview;
    com_ptr<ID3D11Device> d3d;
    com_ptr<ID2D1Device> device;
    com_ptr<IWICImagingFactory> wic;

    PageSource(std::shared_ptr<Session> state, const unsigned char *data,
               int w, int h, double mm_w, double mm_h, int source_dpi)
        : session(std::move(state)), pixels(data, data + size_t(w) * size_t(h) * 4),
          width(w), height(h), dpi(source_dpi), width_mm(mm_w), height_mm(mm_h) {
        // WARP works on remote desktops and both CPU architectures without
        // depending on a particular GPU or the application's rendering device.
        check_hresult(D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_WARP, nullptr,
            D3D11_CREATE_DEVICE_BGRA_SUPPORT, nullptr, 0, D3D11_SDK_VERSION,
            d3d.put(), nullptr, nullptr));
        auto dxgi = d3d.as<IDXGIDevice>();
        com_ptr<ID2D1Factory1> factory;
        check_hresult(D2D1CreateFactory(D2D1_FACTORY_TYPE_MULTI_THREADED,
            __uuidof(ID2D1Factory1), nullptr, factory.put_void()));
        check_hresult(factory->CreateDevice(dxgi.get(), device.put()));
        check_hresult(CoCreateInstance(CLSID_WICImagingFactory2, nullptr,
            CLSCTX_INPROC_SERVER, __uuidof(IWICImagingFactory), wic.put_void()));
    }

    void disconnect() {
        std::lock_guard<std::mutex> lock(mutex);
        closed = true;
        preview = nullptr;
        pixels.clear();
    }

    static void validate_description(const PrintPageDescription &page) {
        const auto &size = page.PageSize;
        const auto &area = page.ImageableRect;
        if (!std::isfinite(size.Width) || !std::isfinite(size.Height) ||
            !std::isfinite(area.X) || !std::isfinite(area.Y) ||
            !std::isfinite(area.Width) || !std::isfinite(area.Height) ||
            size.Width <= 0 || size.Height <= 0 || area.X < 0 || area.Y < 0 ||
            area.Width <= 0 || area.Height <= 0 ||
            area.X + area.Width > size.Width + 0.1f ||
            area.Y + area.Height > size.Height + 0.1f)
            throw hresult_invalid_argument(L"Printer returned invalid printable dimensions");
    }

    static PrintPageDescription page_description(::IInspectable *options) {
        PrintTaskOptions settings{nullptr};
        copy_from_abi(settings, options);
        auto page = settings.GetPageDescription(1);
        validate_description(page);
        return page;
    }

    void draw(ID2D1DeviceContext *context, const PrintPageDescription &page) {
        const auto &area = page.ImageableRect;
        double fit[4];
        if (!thinoda_print_placement(width_mm, height_mm, area.Width, area.Height, fit))
            throw hresult_invalid_argument(L"Cannot fit page into printer dimensions");
        com_ptr<ID2D1Bitmap1> bitmap;
        auto properties = D2D1::BitmapProperties1(D2D1_BITMAP_OPTIONS_NONE,
            D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_IGNORE));
        check_hresult(context->CreateBitmap(D2D1::SizeU(width, height), pixels.data(),
            UINT32(width) * 4, properties, bitmap.put()));
        context->BeginDraw();
        context->Clear(D2D1::ColorF(D2D1::ColorF::White));
        auto target = D2D1::RectF(
            float(area.X + fit[0]), float(area.Y + fit[1]),
            float(area.X + fit[0] + fit[2]), float(area.Y + fit[1] + fit[3]));
        context->DrawBitmap(bitmap.get(), target, 1.0f, D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC);
        check_hresult(context->EndDraw());
    }

    HRESULT STDMETHODCALLTYPE GetPreviewPageCollection(
        IPrintDocumentPackageTarget *target, IPrintPreviewPageCollection **collection) noexcept override {
        if (!collection) return E_POINTER;
        *collection = nullptr;
        try {
            std::lock_guard<std::mutex> lock(mutex);
            if (closed) return RO_E_CLOSED;
            if (!target) return E_POINTER;
            check_hresult(target->GetPackageTarget(ID_PREVIEWPACKAGETARGET_DXGI,
                __uuidof(IPrintPreviewDxgiPackageTarget), preview.put_void()));
            return QueryInterface(__uuidof(IPrintPreviewPageCollection),
                reinterpret_cast<void **>(collection));
        } catch (...) {
            auto code = to_hresult();
            session->fail(code);
            return code;
        }
    }

    HRESULT STDMETHODCALLTYPE Paginate(UINT32, ::IInspectable *options) noexcept override {
        try {
            std::lock_guard<std::mutex> lock(mutex);
            if (closed) return RO_E_CLOSED;
            if (!options || !preview) return E_POINTER;
            description = page_description(options);
            check_hresult(preview->SetJobPageCount(PageCountType::FinalPageCount, 1));
            check_hresult(preview->InvalidatePreview());
            return S_OK;
        } catch (...) {
            auto code = to_hresult();
            session->fail(code);
            return code;
        }
    }

    HRESULT STDMETHODCALLTYPE MakePage(UINT32 page, FLOAT requested_w, FLOAT requested_h) noexcept override {
        try {
            std::lock_guard<std::mutex> lock(mutex);
            if (closed) return RO_E_CLOSED;
            if (!preview || (page != 1 && page != JOB_PAGE_APPLICATION_DEFINED) ||
                !std::isfinite(requested_w) || !std::isfinite(requested_h) ||
                requested_w <= 0 || requested_h <= 0 ||
                description.PageSize.Width <= 0 || description.PageSize.Height <= 0)
                return E_INVALIDARG;
            double scale = std::min({double(requested_w) / description.PageSize.Width,
                double(requested_h) / description.PageSize.Height,
                4096.0 / description.PageSize.Width, 4096.0 / description.PageSize.Height});
            D3D11_TEXTURE2D_DESC texture_desc{};
            texture_desc.Width = UINT(std::ceil(description.PageSize.Width * scale));
            texture_desc.Height = UINT(std::ceil(description.PageSize.Height * scale));
            texture_desc.MipLevels = texture_desc.ArraySize = 1;
            texture_desc.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
            texture_desc.SampleDesc.Count = 1;
            texture_desc.BindFlags = D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE;
            com_ptr<ID3D11Texture2D> texture;
            check_hresult(d3d->CreateTexture2D(&texture_desc, nullptr, texture.put()));
            auto surface = texture.as<IDXGISurface>();
            com_ptr<ID2D1DeviceContext> context;
            check_hresult(device->CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE, context.put()));
            float output_dpi = float(96.0 * scale);
            auto properties = D2D1::BitmapProperties1(
                D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                D2D1::PixelFormat(texture_desc.Format, D2D1_ALPHA_MODE_IGNORE),
                output_dpi, output_dpi);
            com_ptr<ID2D1Bitmap1> target;
            check_hresult(context->CreateBitmapFromDxgiSurface(surface.get(), &properties, target.put()));
            context->SetTarget(target.get());
            context->SetDpi(output_dpi, output_dpi);
            draw(context.get(), description);
            context->SetTarget(nullptr);
            check_hresult(preview->DrawPage(1, surface.get(), output_dpi, output_dpi));
            return S_OK;
        } catch (...) {
            auto code = to_hresult();
            session->fail(code);
            return code;
        }
    }

    void write_document(const PrintPageDescription &page, IPrintDocumentPackageTarget *target) {
        com_ptr<ID2D1DeviceContext> context;
        check_hresult(device->CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE, context.put()));
        com_ptr<ID2D1CommandList> commands;
        check_hresult(context->CreateCommandList(commands.put()));
        context->SetTarget(commands.get());
        draw(context.get(), page);
        context->SetTarget(nullptr);
        check_hresult(commands->Close());
        D2D1_PRINT_CONTROL_PROPERTIES properties{
            D2D1_PRINT_FONT_SUBSET_MODE_DEFAULT, float(dpi), D2D1_COLOR_SPACE_SRGB};
        com_ptr<ID2D1PrintControl> control;
        check_hresult(device->CreatePrintControl(wic.get(), target, &properties, control.put()));
        check_hresult(control->AddPage(commands.get(),
            D2D1::SizeF(page.PageSize.Width, page.PageSize.Height), nullptr));
        check_hresult(control->Close());
    }

    HRESULT STDMETHODCALLTYPE MakeDocument(::IInspectable *options,
        IPrintDocumentPackageTarget *target) noexcept override {
        try {
            std::lock_guard<std::mutex> lock(mutex);
            if (closed) return RO_E_CLOSED;
            if (!options || !target) return E_POINTER;
            write_document(page_description(options), target);
            return S_OK;
        } catch (...) {
            auto code = to_hresult();
            session->fail(code);
            if (target) target->Cancel();
            return code;
        }
    }
};

struct Apartment {
    HRESULT status = RoInitialize(RO_INIT_SINGLETHREADED);
    Apartment() { check_hresult(status); }
    ~Apartment() { RoUninitialize(); }
};

struct ModalOwner {
    HWND window;
    BOOL enabled;
    explicit ModalOwner(HWND owner) : window(owner), enabled(IsWindowEnabled(owner)) {
        if (enabled) EnableWindow(window, FALSE);
    }
    ~ModalOwner() {
        if (enabled && IsWindow(window)) EnableWindow(window, TRUE);
    }
};

void wait_for_completion(const std::shared_ptr<Session> &state) {
    HANDLE event = state->finished.get();
    for (;;) {
        DWORD result = MsgWaitForMultipleObjects(1, &event, FALSE, INFINITE, QS_ALLINPUT);
        if (result == WAIT_OBJECT_0) return;
        if (result != WAIT_OBJECT_0 + 1) throw_last_error();
        MSG message;
        while (PeekMessageW(&message, nullptr, 0, 0, PM_REMOVE)) {
            if (message.message == WM_QUIT) {
                PostQuitMessage(int(message.wParam));
                throw hresult_error(E_ABORT, L"Application closed during printing");
            }
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}
} // namespace

extern "C" int thinoda_print(void *owner, int owner_kind, const unsigned char *pixels,
    int width, int height, double width_mm, double height_mm, int dpi,
    const char *title, char *error, size_t error_len) {
    HWND window = static_cast<HWND>(owner);
    if (owner_kind != 1 || !IsWindow(window) ||
        GetWindowThreadProcessId(window, nullptr) != GetCurrentThreadId())
        return print_error(error, error_len, "Printing must run on the application window thread");
    try {
        Apartment apartment;
        if (!PrintManager::IsSupported())
            return print_error(error, error_len, "Windows native printing is not supported on this device");
        auto state = std::make_shared<Session>();
        if (!state->finished) throw_last_error();
        auto source = make_self<PageSource>(state, pixels, width, height, width_mm, height_mm, dpi);
        auto interop = get_activation_factory<PrintManager, IPrintManagerInterop>();
        PrintManager manager{nullptr};
        check_hresult(interop->GetForWindow(window, guid_of<PrintManager>(), put_abi(manager)));
        auto requested = manager.PrintTaskRequested(auto_revoke,
            [state, source, job_title = to_hstring(title), width_mm, height_mm]
            (const PrintManager &, const PrintTaskRequestedEventArgs &args) {
                try {
                    state->task = args.Request().CreatePrintTask(job_title,
                        [source, state](const PrintTaskSourceRequestedArgs &request) {
                            try { request.SetSource(source.as<IPrintDocumentSource>()); }
                            catch (...) { state->fail(to_hresult()); }
                        });
                    state->completed = state->task.Completed(
                        [state](const PrintTask &, const PrintTaskCompletedEventArgs &done) {
                            try { state->complete(done.Completion()); }
                            catch (...) { state->fail(to_hresult()); state->finish(-1); }
                        });
                    auto options = state->task.Options();
                    options.Orientation(width_mm > height_mm
                        ? PrintOrientation::Landscape : PrintOrientation::Portrait);
                    auto short_mm = std::min(width_mm, height_mm);
                    auto long_mm = std::max(width_mm, height_mm);
                    options.MediaSize(std::abs(short_mm - 210.0) < 0.1 ? PrintMediaSize::IsoA4 :
                        std::abs(short_mm - 297.0) < 0.1 ? PrintMediaSize::IsoA3 :
                        std::abs(long_mm - 355.6) < 0.1 ? PrintMediaSize::NorthAmericaLegal :
                        PrintMediaSize::NorthAmericaLetter);
                } catch (...) { state->fail(to_hresult()); state->finish(-1); }
            });
        IAsyncOperation<bool> show{nullptr};
        try {
            check_hresult(interop->ShowPrintUIForWindowAsync(window,
                guid_of<IAsyncOperation<bool>>(), put_abi(show)));
            show.Completed([state](const IAsyncOperation<bool> &operation, AsyncState status) {
                try {
                    if (status != AsyncState::Completed || !operation.GetResults()) {
                        state->fail(status == AsyncState::Error ? HRESULT(operation.ErrorCode()) : E_FAIL);
                        state->finish(-1);
                    }
                } catch (...) { state->fail(to_hresult()); state->finish(-1); }
            });
            ModalOwner modal(window);
            wait_for_completion(state);
        } catch (...) {
            requested.revoke();
            if (state->task) {
                state->task.Completed(state->completed);
                state->task = nullptr;
            }
            source->disconnect();
            throw;
        }
        requested.revoke();
        if (state->task) {
            state->task.Completed(state->completed);
            state->task = nullptr;
        }
        source->disconnect();
        return state->outcome();
    } catch (const hresult_error &failure) {
        snprintf(error, error_len, "Windows native print preview failed (HRESULT 0x%08lx): %s",
            static_cast<unsigned long>(failure.code().value), to_string(failure.message()).c_str());
        return -1;
    } catch (const std::bad_alloc &) {
        return print_error(error, error_len, "Cannot allocate native print preview resources");
    } catch (const std::exception &failure) {
        snprintf(error, error_len, "Windows native print preview failed: %s", failure.what());
        return -1;
    }
}
