// Job-free tests against the real Windows SDK, D3D WARP and Direct2D.
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <initguid.h>
#include <xpsobjectmodel_1.h>
#include "../print_windows.cpp"
#include <cassert>
#include <limits>

namespace {
// This package target writes only to an in-memory COM stream. It has no printer,
// spooler, output path or document-target factory and cannot submit a print job.
struct MemoryDocument : implements<MemoryDocument, IPrintDocumentPackageTarget, IXpsDocumentPackageTarget> {
    com_ptr<IXpsOMObjectFactory> factory;
    com_ptr<IStream> stream;
    MemoryDocument() {
        check_hresult(CoCreateInstance(__uuidof(XpsOMObjectFactory), nullptr, CLSCTX_INPROC_SERVER,
            __uuidof(IXpsOMObjectFactory), factory.put_void()));
        check_hresult(CreateStreamOnHGlobal(nullptr, TRUE, stream.put()));
    }
    HRESULT STDMETHODCALLTYPE GetPackageTargetTypes(UINT32 *count, GUID **types) noexcept override {
        *types = static_cast<GUID *>(CoTaskMemAlloc(sizeof(GUID)));
        if (!*types) return E_OUTOFMEMORY;
        **types = ID_DOCUMENTPACKAGETARGET_MSXPS;
        *count = 1;
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE GetPackageTarget(REFGUID type, REFIID iid, void **out) noexcept override {
        if (type != ID_DOCUMENTPACKAGETARGET_MSXPS) return E_NOINTERFACE;
        return QueryInterface(iid, out);
    }
    HRESULT STDMETHODCALLTYPE Cancel() noexcept override { return S_OK; }
    HRESULT STDMETHODCALLTYPE GetXpsOMFactory(IXpsOMObjectFactory **out) noexcept override {
        return factory->QueryInterface(__uuidof(IXpsOMObjectFactory), reinterpret_cast<void **>(out));
    }
    HRESULT STDMETHODCALLTYPE GetXpsType(XPS_DOCUMENT_TYPE *type) noexcept override {
        *type = XPS_DOCUMENT_TYPE_XPS;
        return S_OK;
    }
    HRESULT STDMETHODCALLTYPE GetXpsOMPackageWriter(IOpcPartUri *sequence, IOpcPartUri *discard,
        IXpsOMPackageWriter **writer) noexcept override {
        return factory->CreatePackageWriterOnStream(stream.get(), TRUE,
            XPS_INTERLEAVING_OFF, sequence, nullptr, nullptr, nullptr, discard, writer);
    }
    void verify(Size expected) {
        LARGE_INTEGER zero{};
        check_hresult(stream->Seek(zero, STREAM_SEEK_SET, nullptr));
        com_ptr<IXpsOMPackage> package;
        check_hresult(factory->CreatePackageFromStream(stream.get(), FALSE, package.put()));
        com_ptr<IXpsOMDocumentSequence> sequence;
        check_hresult(package->GetDocumentSequence(sequence.put()));
        com_ptr<IXpsOMDocumentCollection> documents;
        check_hresult(sequence->GetDocuments(documents.put()));
        UINT32 count = 0;
        check_hresult(documents->GetCount(&count));
        assert(count == 1);
        com_ptr<IXpsOMDocument> document;
        check_hresult(documents->GetAt(0, document.put()));
        com_ptr<IXpsOMPageReferenceCollection> pages;
        check_hresult(document->GetPageReferences(pages.put()));
        check_hresult(pages->GetCount(&count));
        assert(count == 1);
        com_ptr<IXpsOMPageReference> reference;
        check_hresult(pages->GetAt(0, reference.put()));
        com_ptr<IXpsOMPage> page;
        check_hresult(reference->GetPage(page.put()));
        XPS_SIZE size;
        check_hresult(page->GetPageDimensions(&size));
        assert(size.width == expected.Width && size.height == expected.Height);
        com_ptr<IXpsOMVisualCollection> visuals;
        check_hresult(page->GetVisuals(visuals.put()));
        check_hresult(visuals->GetCount(&count));
        assert(count > 0);
    }
};

struct PreviewTarget : implements<PreviewTarget, IPrintPreviewDxgiPackageTarget> {
    com_ptr<ID3D11Texture2D> image;
    float dpi_x = 0, dpi_y = 0;
    HRESULT STDMETHODCALLTYPE SetJobPageCount(PageCountType type, UINT32 count) noexcept override {
        return type == PageCountType::FinalPageCount && count == 1 ? S_OK : E_INVALIDARG;
    }
    HRESULT STDMETHODCALLTYPE InvalidatePreview() noexcept override { return S_OK; }
    HRESULT STDMETHODCALLTYPE DrawPage(UINT32 page, IDXGISurface *surface, FLOAT x, FLOAT y) noexcept override {
        if (page != 1 || !surface) return E_INVALIDARG;
        dpi_x = x;
        dpi_y = y;
        return surface->QueryInterface(__uuidof(ID3D11Texture2D), image.put_void());
    }
};

struct PreviewPackage : implements<PreviewPackage, IPrintDocumentPackageTarget> {
    com_ptr<PreviewTarget> target = make_self<PreviewTarget>();
    HRESULT STDMETHODCALLTYPE GetPackageTargetTypes(UINT32 *, GUID **) noexcept override {
        return E_NOTIMPL;
    }
    HRESULT STDMETHODCALLTYPE GetPackageTarget(REFGUID type, REFIID iid, void **out) noexcept override {
        if (type != ID_PREVIEWPACKAGETARGET_DXGI) return E_NOINTERFACE;
        return target->QueryInterface(iid, out);
    }
    HRESULT STDMETHODCALLTYPE Cancel() noexcept override {
        assert(false && "Preview tests must never start/cancel a spool job");
        return E_UNEXPECTED;
    }
};

void rendering_and_lifecycle() {
    auto state = std::make_shared<Session>();
    // Unequal channels on opposite halves expose color order and mirroring.
    std::vector<unsigned char> pixels(120 * 80 * 4, 255);
    for (size_t index = 0; index < pixels.size(); index += 4) {
        pixels[index] = pixels[index + 1] = 0;
        if ((index / 4) % 120 >= 60) {
            pixels[index] = 255;
            pixels[index + 2] = 0;
        }
    }
    auto source = make_self<PageSource>(state, pixels.data(), 120, 80, 297, 210, 150);
    auto package = make_self<PreviewPackage>();
    auto document_source = source.as<IPrintDocumentSource>();
    auto native_source = document_source.as<IPrintDocumentPageSource>();
    com_ptr<IPrintPreviewPageCollection> collection;
    check_hresult(native_source->GetPreviewPageCollection(package.get(), collection.put()));
    assert(collection);
    for (auto size : {Size{1123, 794}, Size{794, 1123}, Size{1587, 1123}, Size{816, 1344}}) {
        source->description.PageSize = size;
        source->description.ImageableRect = {20, 30, size.Width - 60, size.Height - 70};
        PageSource::validate_description(source->description);
        check_hresult(collection->MakePage(1, size.Width, size.Height));
        D3D11_TEXTURE2D_DESC desc;
        package->target->image->GetDesc(&desc);
        assert(desc.Width == UINT(size.Width) && desc.Height == UINT(size.Height));
        assert(package->target->dpi_x == 96 && package->target->dpi_y == 96);
        desc.BindFlags = 0;
        desc.Usage = D3D11_USAGE_STAGING;
        desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
        com_ptr<ID3D11Texture2D> readback;
        check_hresult(source->d3d->CreateTexture2D(&desc, nullptr, readback.put()));
        com_ptr<ID3D11DeviceContext> context;
        source->d3d->GetImmediateContext(context.put());
        context->CopyResource(readback.get(), package->target->image.get());
        D3D11_MAPPED_SUBRESOURCE mapped;
        check_hresult(context->Map(readback.get(), 0, D3D11_MAP_READ, 0, &mapped));
        auto data = static_cast<unsigned char *>(mapped.pData);
        assert(data[0] == 255 && data[1] == 255 && data[2] == 255);
        double fit[4];
        auto area = source->description.ImageableRect;
        assert(thinoda_print_placement(297, 210, area.Width, area.Height, fit));
        double transition = std::max(2.0, fit[2] / 120 * 2);
        UINT red_pixels = 0;
        for (UINT y = 0; y < desc.Height; ++y) {
            for (UINT x = 0; x < desc.Width; ++x) {
                auto pixel = data + y * mapped.RowPitch + x * 4;
                bool interior = x > area.X + fit[0] + 2 && x < area.X + fit[0] + fit[2] - 2 &&
                    y > area.Y + fit[1] + 2 && y < area.Y + fit[1] + fit[3] - 2;
                if (interior) {
                    if (x < area.X + fit[0] + fit[2] / 2 - transition) {
                        assert(pixel[0] == 0 && pixel[1] == 0 && pixel[2] == 255);
                        ++red_pixels;
                    }
                    if (x > area.X + fit[0] + fit[2] / 2 + transition)
                        assert(pixel[0] == 255 && pixel[1] == 0 && pixel[2] == 0);
                }
                bool exterior = x < area.X + fit[0] - 2 || x > area.X + fit[0] + fit[2] + 2 ||
                    y < area.Y + fit[1] - 2 || y > area.Y + fit[1] + fit[3] + 2;
                if (exterior) assert(pixel[0] == 255 && pixel[1] == 255 && pixel[2] == 255);
            }
        }
        assert(red_pixels > 10000);
        context->Unmap(readback.get(), 0);
        auto document = make_self<MemoryDocument>();
        source->write_document(source->description, document.get());
        document->verify(size);
    }
    check_hresult(collection->MakePage(JOB_PAGE_APPLICATION_DEFINED, 10000, 10000));
    D3D11_TEXTURE2D_DESC large;
    package->target->image->GetDesc(&large);
    assert(large.Width <= 4096 && large.Height <= 4096);
    assert(collection->MakePage(2, 800, 600) == E_INVALIDARG);
    assert(collection->MakePage(1, 0, 600) == E_INVALIDARG);
    assert(collection->MakePage(1, std::numeric_limits<float>::infinity(), 600) == E_INVALIDARG);
    assert(source->MakeDocument(nullptr, nullptr) == E_POINTER);
    source->description.ImageableRect.Width = -1;
    bool rejected = false;
    try { PageSource::validate_description(source->description); }
    catch (const hresult_invalid_argument &) { rejected = true; }
    assert(rejected);
    // Retain OS-facing interfaces beyond disconnect; late callbacks are safe.
    source->disconnect();
    assert(source->pixels.empty() && !source->preview);
    assert(collection->MakePage(1, 800, 600) == RO_E_CLOSED);
    assert(collection->Paginate(1, nullptr) == RO_E_CLOSED);
    assert(native_source->MakeDocument(nullptr, nullptr) == RO_E_CLOSED);
    collection = nullptr;
    assert(native_source->GetPreviewPageCollection(package.get(), collection.put()) == RO_E_CLOSED);
}

void outcomes() {
    for (auto value : {PrintTaskCompletion::Abandoned, PrintTaskCompletion::Canceled,
                      PrintTaskCompletion::Submitted, PrintTaskCompletion::Failed}) {
        auto state = std::make_shared<Session>();
        state->complete(value);
        assert(WaitForSingleObject(state->finished.get(), 0) == WAIT_OBJECT_0);
        assert(state->result == (value == PrintTaskCompletion::Submitted ? 1 :
            value == PrintTaskCompletion::Failed ? -1 : 0));
        assert(FAILED(state->failure) == (value == PrintTaskCompletion::Failed));
        if (value != PrintTaskCompletion::Failed) assert(state->outcome() == state->result);
        int first = state->result;
        state->finish(1);
        assert(state->result == first);
    }
    auto state = std::make_shared<Session>();
    state->fail(E_INVALIDARG);
    state->fail(E_FAIL);
    state->complete(PrintTaskCompletion::Submitted);
    assert(state->failure == E_INVALIDARG);
    for (HRESULT code : {E_ABORT, HRESULT_FROM_WIN32(ERROR_CANCELLED),
                         HRESULT_FROM_WIN32(ERROR_PRINT_CANCELLED), E_INVALIDARG}) {
        auto cancelled = std::make_shared<Session>();
        cancelled->fail(code);
        cancelled->complete(PrintTaskCompletion::Canceled);
        bool rejected = false;
        try { assert(cancelled->outcome() == 0); }
        catch (const hresult_error &) { rejected = true; }
        assert(rejected == (code == E_INVALIDARG));
    }
}

int preview_ui() {
    // Explicit manual mode only. Default/CI execution never opens a print UI.
    HWND owner = CreateWindowExW(0, L"STATIC", L"Thinoda native preview probe - Cancel only",
        WS_OVERLAPPEDWINDOW | WS_VISIBLE, CW_USEDEFAULT, CW_USEDEFAULT, 900, 700,
        nullptr, nullptr, GetModuleHandleW(nullptr), nullptr);
    assert(owner);
    std::vector<unsigned char> pixels(1240 * 1754 * 4, 255);
    for (int y = 400; y < 1300; ++y) {
        for (int x = 200; x < 1000; ++x) {
            size_t index = (size_t(y) * 1240 + x) * 4;
            pixels[index] = 100;
            pixels[index + 1] = 180;
            pixels[index + 2] = 40;
        }
    }
    char error[1024]{};
    int result = thinoda_print(owner, 1, pixels.data(), 1240, 1754, 210, 297, 150,
        "Thinoda native preview probe - Cancel only", error, sizeof(error));
    printf("Native UI result: %d %s\n", result, error);
    assert(IsWindowEnabled(owner));
    WCHAR executable[MAX_PATH];
    assert(GetModuleFileNameW(nullptr, executable, MAX_PATH));
    std::wstring log_path = std::wstring(executable) + L".result.txt";
    FILE *log = nullptr;
    if (_wfopen_s(&log, log_path.c_str(), L"w") == 0 && log) {
        fprintf(log, "Native UI result: %d; owner re-enabled: %d; %s\n",
            result, int(IsWindowEnabled(owner)), error);
        fclose(log);
    } else return 1;
    DestroyWindow(owner);
    return result == 0 ? 0 : 1;
}
}

int main(int argc, char **argv) {
    try {
#ifdef THINODA_PREVIEW_UI_PROBE
        if (argc == 1) return preview_ui();
#endif
        if (argc == 2 && std::string(argv[1]) == "--preview-ui") return preview_ui();
        Apartment apartment;
        rendering_and_lifecycle();
        outcomes();
        puts("Job-free Windows preview rendering, dimensions, lifecycle and outcomes passed");
        return 0;
    } catch (const hresult_error &error) {
        fprintf(stderr, "Native test HRESULT 0x%08lx: %s\n",
            static_cast<unsigned long>(error.code().value), to_string(error.message()).c_str());
        return 1;
    }
}
