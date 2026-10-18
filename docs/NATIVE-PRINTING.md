_Last updated: 2026-10-18 10:53:14 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

# Native system printing

## Windows system preview

The Windows backend supplies the page to **Windows' own print UI**, not a second
Thinoda preview, an external viewer or a PDF handoff. The previous `PrintDlgW`
path only returned a GDI printer DC after confirmation: it supplied no document
source to the modern Windows preview host.

The replacement uses the Windows SDK's C++/WinRT projection, without adding
WinUI, XAML, a runtime installer, registry changes or a default-printer fallback:

1. `IPrintManagerInterop.GetForWindow` registers `PrintTaskRequested` for the
   live application HWND. `ShowPrintUIForWindowAsync` opens the system UI.
2. The print task receives a custom `IPrintDocumentSource` implementing the
   SDK `DocumentSource.h` contracts `IPrintDocumentPageSource` and
   `IPrintPreviewPageCollection`.
3. `Paginate` reads page 1's selected paper and imageable rectangle from
   `PrintTaskOptions.GetPageDescription`. The document always has one page.
   `MakePage` renders an opaque BGRA DXGI surface and calls
   `IPrintPreviewDxgiPackageTarget.DrawPage`. Preview surfaces are bounded to
   4096 pixels on either axis, independently of source/print resolution.
4. On print confirmation, Windows calls `MakeDocument` with its package target.
   Direct2D records the same raster drawing into a command list and sends one
   physical page through `ID2D1PrintControl.AddPage`/`Close`.

The whole app-rendered source page (including its margins) is centered and fitted
uniformly into the OS imageable rectangle. Changing paper/orientation changes
that rectangle, not topic layout or the source page's aspect ratio. Preview and
submission use the same drawing routine. Copies, duplex, collation, color,
quality, tray selection and further supported driver settings remain OS-owned;
Thinoda seeds only paper and orientation. Driver processing can still affect
physical output, which no application preview can guarantee.

The entry point remains synchronous on the HWND's STA thread. It pumps native
messages while keeping the owner disabled, owns a copy of the source pixels,
serializes rendering callbacks, unregisters events and disconnects the source
on return. Retained interfaces reject late callbacks instead of borrowing Rust
memory. Cancelled/abandoned tasks return cancellation; only the task's
`Submitted` completion returns submission. UI display is not submission, and
native failures include their HRESULT. Unsupported native printing is an
explicit error, never a classic-dialog/browser fallback.

The native UI follows Windows' language/theme. No app translation keys changed;
the existing EN/DE and expanded-locale print controls remain unchanged.

### References

- [Printing from a Windows desktop app](https://learn.microsoft.com/windows/apps/develop/devices-sensors/print-from-your-app)
- [IPrintManagerInterop](https://learn.microsoft.com/windows/win32/api/printmanagerinterop/nn-printmanagerinterop-iprintmanagerinterop)
- [ShowPrintUIForWindowAsync](https://learn.microsoft.com/windows/win32/api/printmanagerinterop/nf-printmanagerinterop-iprintmanagerinterop-showprintuiforwindowasync)
- [PrintTaskOptions.GetPageDescription](https://learn.microsoft.com/uwp/api/windows.graphics.printing.printtaskoptions.getpagedescription)
- [ID2D1PrintControl](https://learn.microsoft.com/windows/win32/api/d2d1_1/nn-d2d1_1-id2d1printcontrol)
- Windows SDK 10.0.26100.0: `winrt/DocumentSource.h`, `um/PrintPreview.h`,
  `um/PrintManagerInterop.h` and `cppwinrt/winrt/Windows.Graphics.Printing.h`.
  The real SDK declarations, not hand-declared vtables, are compiled in CI.

## Other native platforms: audit, not new preview delivery

macOS retains its standalone application-modal `NSPrintPanel` followed by
`NSPrintOperation`. The existing panel does **not** connect the operation's
document preview, so this change does not claim to fix macOS inline preview.
That requires a separate AppKit panel/operation lifecycle change with reliable
cancellation/error differentiation and is retained in the backlog.

Linux retains GTK 3 `GtkPrintOperation`, `begin-print` and `draw-page`, using
Cairo and the selected page setup/hardware margins. Native preview availability
depends on GTK and the desktop's printing backend; no new inline-preview
guarantee or external-viewer handoff is added here. Existing GTK runtime loading,
X11/Wayland behavior, main-thread checks and cancellation remain unchanged.

## Job-free verification

`python .github/scripts/test_native_printing.py -v` uses the installed MSVC and
Windows SDK on Windows x64/ARM64. It compiles with `/W4 /WX /permissive-`, renders
real WARP/DXGI surfaces, reads back colors and fitted placement with asymmetric
printable bounds, checks portrait/landscape and changed paper sizes, verifies the
preview allocation bound, and rejects invalid/late requests. The actual Direct2D
print control serializes to an **in-memory** package target; reopening the XPS
checks its single-page dimensions/content without involving a printer or spooler.
Completion-state tests cover cancelled, abandoned, failed and submitted events.
Rust tests cover source RGB/BGRA conversion, physical dimensions and ABI outcomes.

The same script retains macOS Intel/ARM SDK compile/link checks and Linux
missing-display/cancel-only GTK checks. It never opens a Windows print UI in CI.

For manual Windows verification, build/run the production application, create
an original built-in map, open Print and Continue, and inspect the **system**
preview. Change job-local paper/orientation in that dialog, inspect the complete
map and margins, then Cancel and verify Thinoda reports cancellation and remains
usable. Do not press Print or change printer defaults/device configuration.
Physical output, driver-specific finishing and interactive Windows ARM64 UI
behavior require separately approved hardware testing, not a simulated success.

### Observed Windows x64 check (2026-10-18)

The production debug app's original Mind Map template (Central Topic and four
colored branches) rendered inside the German Windows 11 system dialog, including
the 1/1 page indicator. Changing OS landscape to portrait preserved every topic
and the fixed source-page aspect ratio. In another invocation, source margins
were changed from 12 to 24 mm and OS advanced settings changed A4 to Legal;
the resulting landscape preview showed the complete map on the changed paper.
Copies, color, duplex, collation, tray, quality and paper controls remained
available. The system dialog's Cancel returned to Thinoda with **Printing
cancelled**, retained the source A4/landscape settings and 24 mm margin, and
closing the app preview restored editor controls. No Print action, spool job,
printer-default/device change or external viewer was used. This check used the
OS dark theme; other OS themes and ARM64 interactive behavior are not claimed as
manually exercised.
