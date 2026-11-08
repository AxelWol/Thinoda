use thinoda_io::{Format, import_drawio_link};

use super::support;

pub fn exercise(data: &[u8]) {
    if data.len() > support::MAX_INPUT_SIZE {
        return;
    }

    support::roundtrip(Format::DrawIo, data);
    support::roundtrip(Format::DrawIoHtml, data);
    if let Ok(link) = std::str::from_utf8(data)
        && let Ok(report) = import_drawio_link(link)
    {
        support::roundtrip_report(report);
    }
}
