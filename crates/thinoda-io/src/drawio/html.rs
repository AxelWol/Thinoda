//! Extract the diagram from an HTML viewer without executing or loading the page.

use super::{IoError, MAX_BYTES, MAX_XML_ELEMENTS};
use crate::DrawIoHtmlError;
use lol_html::{HtmlRewriter, MemorySettings, Settings, element, errors::RewritingError};
use serde::Deserialize;

#[derive(Deserialize)]
struct Viewer {
    xml: Option<String>,
}

pub(super) fn extract(bytes: &[u8]) -> Result<String, IoError> {
    extract_with_memory_limit(bytes, MAX_BYTES)
}

fn extract_with_memory_limit(bytes: &[u8], memory_limit: usize) -> Result<String, IoError> {
    if bytes.len() > MAX_BYTES {
        return Err(DrawIoHtmlError::SizeLimit.into());
    }
    std::str::from_utf8(bytes).map_err(|_| DrawIoHtmlError::InvalidEncoding)?;

    let mut elements = 0usize;
    let mut viewer = None;
    {
        let settings = Settings::new()
            .with_memory_settings(MemorySettings::new().with_max_allowed_memory_usage(memory_limit))
            .append_element_content_handler(element!("*", |el| {
                elements += 1;
                if elements > MAX_XML_ELEMENTS {
                    return Err(Box::new(DrawIoHtmlError::StructureLimit));
                }
                if el.tag_name() != "div" {
                    return Ok(());
                }
                let Some(class) = el.get_attribute("class") else {
                    return Ok(());
                };
                if !html_escape::decode_html_entities(&class)
                    .split_ascii_whitespace()
                    .any(|name| name == "mxgraph")
                {
                    return Ok(());
                }
                if viewer.is_some() {
                    return Err(Box::new(DrawIoHtmlError::MultipleViewers));
                }
                viewer = Some(el.get_attribute("data-mxgraph"));
                Ok(())
            }));
        let mut rewriter = HtmlRewriter::new(settings, |_: &[u8]| {});
        for chunk in bytes.chunks(16 * 1024) {
            rewriter.write(chunk).map_err(parse_error)?;
        }
        rewriter.end().map_err(parse_error)?;
    }

    let data = viewer.flatten().ok_or(DrawIoHtmlError::NoViewer)?;
    let decoded = html_escape::decode_html_entities(&data);
    let viewer: Viewer =
        serde_json::from_str(&decoded).map_err(|_| DrawIoHtmlError::InvalidViewer)?;
    viewer
        .xml
        .filter(|xml| !xml.trim().is_empty())
        .ok_or_else(|| DrawIoHtmlError::NoEmbeddedDiagram.into())
}

fn parse_error(error: RewritingError) -> IoError {
    match error {
        RewritingError::MemoryLimitExceeded(_) => DrawIoHtmlError::SizeLimit,
        RewritingError::ContentHandlerError(error) => error
            .downcast_ref::<DrawIoHtmlError>()
            .copied()
            .unwrap_or(DrawIoHtmlError::InvalidMarkup),
        _ => DrawIoHtmlError::InvalidMarkup,
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_enforces_its_memory_budget() {
        let html = format!("<div title=\"{}\"></div>", "x".repeat(4096));
        assert!(matches!(
            extract_with_memory_limit(html.as_bytes(), 1024),
            Err(IoError::DrawIoHtml(DrawIoHtmlError::SizeLimit))
        ));
    }

    #[test]
    fn parser_rejects_too_many_elements() {
        let html = "<i></i>".repeat(MAX_XML_ELEMENTS + 1);
        assert!(matches!(
            extract(html.as_bytes()),
            Err(IoError::DrawIoHtml(DrawIoHtmlError::StructureLimit))
        ));
    }
}
