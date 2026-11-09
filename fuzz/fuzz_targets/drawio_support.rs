use thinoda_io::{Format, ImportWarning, drawio_clipboard, import_drawio_link};

use super::support;

pub fn exercise(data: &[u8]) {
    if data.len() > support::MAX_INPUT_SIZE {
        return;
    }

    support::roundtrip(Format::DrawIo, data);
    support::roundtrip(Format::DrawIoHtml, data);
    if let Ok(text) = std::str::from_utf8(data) {
        if let Ok(report) = import_drawio_link(text) {
            support::roundtrip_report(report);
        }
        if drawio_clipboard::looks_like_drawio(text)
            && let Ok(pasted) = drawio_clipboard::decode(text)
        {
            pasted
                .payload
                .validate()
                .expect("decoded draw.io clipboard payload must validate");
            if !pasted.losses.is_empty() {
                assert!(pasted.warnings.contains(&ImportWarning::Other));
            }
        }
    }
}
