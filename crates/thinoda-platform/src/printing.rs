//! Synchronous native printer UI and submission. Never opens another application.
//!
//! The caller must invoke [`print`] on its window/event-loop thread. The borrowed
//! window handle cannot outlive that call. Native UI runs a modal loop; callers
//! must also keep their application editor modal until this function returns.

use std::ffi::{CStr, CString, c_char, c_int, c_void};

use raw_window_handle::{RawWindowHandle, WindowHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintOutcome {
    Cancelled,
    /// Accepted by the OS print subsystem, not confirmation of physical output.
    Submitted,
}

/// Opaque, bounded, top-down opaque BGRA page (native bridges retain no pointers).
pub struct PrintPage {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
    size_mm: [f64; 2],
    dpi: u32,
}

impl PrintPage {
    /// Converts preview RGB pixels without changing colors, margins or orientation.
    /// Supports A4/A3/Letter/Legal in either orientation. May be prepared on a
    /// worker. `dpi` is source resolution, not driver quality.
    pub fn from_rgb(
        rgb: &[u8],
        width: u32,
        height: u32,
        size_mm: [f64; 2],
        dpi: u32,
    ) -> Result<Self, String> {
        let count = u64::from(width) * u64::from(height);
        if width == 0
            || height == 0
            || count > 18_000_000
            || count * 3 != rgb.len() as u64
            || !matches!(dpi, 150 | 300)
            || size_mm
                .iter()
                .any(|v| !v.is_finite() || !(100.0..=500.0).contains(v))
        {
            return Err("Invalid native print page dimensions, resolution or pixels".into());
        }
        let short = size_mm[0].min(size_mm[1]);
        let long = size_mm[0].max(size_mm[1]);
        if ![
            [210.0, 297.0],
            [297.0, 420.0],
            [215.9, 279.4],
            [215.9, 355.6],
        ]
        .iter()
        .any(|paper| (paper[0] - short).abs() < 0.01 && (paper[1] - long).abs() < 0.01)
        {
            return Err("Unsupported physical print paper size".into());
        }
        // Reject mismatched physical geometry rather than silently stretching.
        if (f64::from(width) - size_mm[0] * f64::from(dpi) / 25.4).abs() > 0.51
            || (f64::from(height) - size_mm[1] * f64::from(dpi) / 25.4).abs() > 0.51
        {
            return Err("Print page pixels do not match its physical size and resolution".into());
        }
        let mut pixels = Vec::with_capacity(count as usize * 4);
        for p in rgb.as_chunks::<3>().0 {
            pixels.extend_from_slice(&[p[2], p[1], p[0], 255]);
        }
        Ok(Self {
            pixels,
            width,
            height,
            size_mm,
            dpi,
        })
    }
}

unsafe extern "C" {
    fn thinoda_print(
        owner: *mut c_void,
        owner_kind: c_int,
        pixels: *const u8,
        width: c_int,
        height: c_int,
        width_mm: f64,
        height_mm: f64,
        dpi: c_int,
        title: *const c_char,
        error: *mut c_char,
        error_len: usize,
    ) -> c_int;
}

fn outcome(code: c_int, error: &[c_char]) -> Result<PrintOutcome, String> {
    match code {
        0 => Ok(PrintOutcome::Cancelled),
        1 => Ok(PrintOutcome::Submitted),
        _ => {
            // The buffer is initialized and bridges always leave a trailing NUL.
            let detail = unsafe { CStr::from_ptr(error.as_ptr()) }.to_string_lossy();
            Err(if detail.is_empty() {
                format!("Native printing failed (status {code})")
            } else {
                detail.into_owned()
            })
        }
    }
}

/// Shows printer selection and driver settings, then submits only on confirmation.
/// The OS owns copies, duplex, device resolution, color management and job handling.
/// The whole source page is fitted, centered and never distorted in the chosen
/// printable area; OS paper/orientation changes do not reflow the map.
pub fn print(
    owner: WindowHandle<'_>,
    page: &PrintPage,
    title: &str,
) -> Result<PrintOutcome, String> {
    let title = CString::new(title).map_err(|_| "Print title contains a NUL character")?;
    let (owner, owner_kind) = match owner.as_raw() {
        RawWindowHandle::Win32(h) => (h.hwnd.get() as *mut c_void, 1),
        RawWindowHandle::AppKit(h) => (h.ns_view.as_ptr(), 2),
        RawWindowHandle::Xlib(h) => (h.window as usize as *mut c_void, 3),
        RawWindowHandle::Xcb(h) => (h.window.get() as usize as *mut c_void, 3),
        RawWindowHandle::Wayland(h) => (h.surface.as_ptr(), 4),
        _ => return Err("Unsupported native print window system".into()),
    };
    let mut error = [0 as c_char; 1024];
    // SAFETY: all buffers are bounded and live for the entire synchronous call;
    // WindowHandle borrows the live window. No bridge retains pointers or calls
    // Rust during its modal loop. Entry points verify their native UI thread.
    let code = unsafe {
        thinoda_print(
            owner,
            owner_kind,
            page.pixels.as_ptr(),
            page.width as c_int,
            page.height as c_int,
            page.size_mm[0],
            page.size_mm[1],
            page.dpi as c_int,
            title.as_ptr(),
            error.as_mut_ptr(),
            error.len(),
        )
    };
    outcome(code, &error)
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "C" {
        fn thinoda_print_placement(
            page_w: f64,
            page_h: f64,
            area_w: f64,
            area_h: f64,
            out: *mut f64,
        ) -> c_int;
    }

    #[test]
    fn native_bridges_fit_entire_page_on_changed_paper_without_distortion() {
        for page in [
            [210.0, 297.0],
            [297.0, 210.0],
            [297.0, 420.0],
            [215.9, 355.6],
        ] {
            for area in [[200.0, 280.0], [280.0, 200.0], [8.0, 100.0], [500.0, 500.0]] {
                let mut out = [0.0; 4];
                // SAFETY: four-element destination lives through the call; no UI.
                assert_eq!(
                    unsafe {
                        thinoda_print_placement(
                            page[0],
                            page[1],
                            area[0],
                            area[1],
                            out.as_mut_ptr(),
                        )
                    },
                    1
                );
                let [x, y, w, h] = out;
                assert!(x >= -1e-10 && y >= -1e-10);
                assert!((x * 2.0 + w - area[0]).abs() < 1e-10);
                assert!((y * 2.0 + h - area[1]).abs() < 1e-10);
                assert!((w / h - page[0] / page[1]).abs() < 1e-10);
            }
        }
        for value in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let mut out = [0.0; 4];
            // SAFETY: same bounded output as above; pure geometry helper.
            assert_eq!(
                unsafe { thinoda_print_placement(210.0, 297.0, value, 280.0, out.as_mut_ptr()) },
                0
            );
        }
    }

    fn page(
        rgb: &[u8],
        width: u32,
        height: u32,
        mm: [f64; 2],
        dpi: u32,
    ) -> Result<PrintPage, String> {
        PrintPage::from_rgb(rgb, width, height, mm, dpi)
    }

    #[test]
    fn conversion_preserves_colors_and_source_geometry() {
        let mut rgb = vec![255; 1240 * 1754 * 3];
        rgb[..6].copy_from_slice(&[10, 90, 150, 80, 80, 80]);
        let p = page(&rgb, 1240, 1754, [210.0, 297.0], 150).unwrap();
        assert_eq!(&p.pixels[..8], &[150, 90, 10, 255, 80, 80, 80, 255]);
        assert_eq!(p.pixels.len(), 1240 * 1754 * 4);
        assert_eq!(p.size_mm, [210.0, 297.0]);
    }

    #[test]
    fn invalid_input_is_rejected_before_any_native_ui_or_job() {
        for (w, h, mm, dpi) in [
            (0, 0, [210.0, 297.0], 150),
            (u32::MAX, u32::MAX, [210.0, 297.0], 150),
            (1, 1, [f64::NAN, 297.0], 150),
            (1, 1, [210.0, 297.0], 600),
            (1, 1, [210.0, 297.0], 150),
        ] {
            assert!(page(&[0, 0, 0], w, h, mm, dpi).is_err());
        }
        assert!(page(&[], 1240, 1754, [210.0, 297.0], 150).is_err());
        assert!(
            page(&vec![0; 591 * 591 * 3], 591, 591, [100.0, 100.0], 150)
                .err()
                .unwrap()
                .contains("Unsupported physical")
        );
    }

    #[test]
    fn cancellation_submission_and_failure_are_distinct() {
        let empty = [0; 8];
        assert_eq!(outcome(0, &empty), Ok(PrintOutcome::Cancelled));
        assert_eq!(outcome(1, &empty), Ok(PrintOutcome::Submitted));
        assert!(outcome(-1, &empty).unwrap_err().contains("failed"));
        let error: Vec<c_char> = b"driver unavailable\0"
            .iter()
            .map(|v| *v as c_char)
            .collect();
        assert_eq!(outcome(-1, &error).unwrap_err(), "driver unavailable");
    }

    #[test]
    fn native_entry_rejects_wrong_thread_or_missing_window_without_showing_ui() {
        // Rust's test harness invokes tests on worker threads. AppKit/GTK reject
        // these before initialization; Win32 rejects the missing owner first.
        let pixels = [255; 4];
        let mut error = [0; 1024];
        // SAFETY: bounded buffers live through the synchronous call. Null owner
        // is an explicitly rejected input; no native window or job is created.
        let result = unsafe {
            thinoda_print(
                std::ptr::null_mut(),
                0,
                pixels.as_ptr(),
                1,
                1,
                210.0,
                297.0,
                150,
                c"Test".as_ptr(),
                error.as_mut_ptr(),
                error.len(),
            )
        };
        assert_eq!(result, -1);
        assert!(outcome(result, &error).unwrap_err().contains("thread"));
    }
}
