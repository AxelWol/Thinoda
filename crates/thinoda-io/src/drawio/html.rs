//! Extract the diagram from an HTML viewer without executing or loading the page.

use super::{IoError, MAX_BYTES, MAX_XML_ELEMENTS, decode_compressed, percent_decode};
use crate::DrawIoHtmlError;
use lol_html::{HtmlRewriter, MemorySettings, Settings, element, end_tag, errors::RewritingError};
use serde::Deserialize;
use std::{cell::RefCell, rc::Rc};

#[derive(Deserialize)]
struct Viewer {
    xml: Option<String>,
}

enum ViewerSource {
    Inline(String),
    Legacy {
        open: bool,
        closed: bool,
        data_start: Option<usize>,
        data_end: Option<usize>,
    },
}

pub(super) fn extract(bytes: &[u8]) -> Result<String, IoError> {
    extract_with_memory_limit(bytes, MAX_BYTES)
}

fn extract_with_memory_limit(bytes: &[u8], memory_limit: usize) -> Result<String, IoError> {
    if bytes.len() > MAX_BYTES {
        return Err(DrawIoHtmlError::SizeLimit.into());
    }
    let html = std::str::from_utf8(bytes).map_err(|_| DrawIoHtmlError::InvalidEncoding)?;

    let mut elements = 0usize;
    let viewer = Rc::new(RefCell::new(None));
    {
        let viewer = Rc::clone(&viewer);
        let settings = Settings::new()
            .with_memory_settings(MemorySettings::new().with_max_allowed_memory_usage(memory_limit))
            .append_element_content_handler(element!("*", move |el| {
                elements += 1;
                if elements > MAX_XML_ELEMENTS {
                    return Err(Box::new(DrawIoHtmlError::StructureLimit));
                }
                if el.tag_name() != "div" {
                    return Ok(());
                }
                let is_viewer = el.get_attribute("class").is_some_and(|class| {
                    html_escape::decode_html_entities(&class)
                        .split_ascii_whitespace()
                        .any(|name| name == "mxgraph")
                });
                if is_viewer {
                    let data = el.get_attribute("data-mxgraph");
                    {
                        let mut viewer = viewer.borrow_mut();
                        if viewer.is_some() {
                            return Err(Box::new(DrawIoHtmlError::MultipleViewers));
                        }
                        *viewer = Some(match data {
                            Some(data) => ViewerSource::Inline(data),
                            None => ViewerSource::Legacy {
                                open: true,
                                closed: false,
                                data_start: None,
                                data_end: None,
                            },
                        });
                    }
                    if matches!(*viewer.borrow(), Some(ViewerSource::Legacy { .. })) {
                        let viewer = Rc::clone(&viewer);
                        el.on_end_tag(end_tag!(move |_| {
                            if let Some(ViewerSource::Legacy { open, closed, .. }) =
                                viewer.borrow_mut().as_mut()
                            {
                                *open = false;
                                *closed = true;
                            }
                            Ok(())
                        }))?;
                    }
                    return Ok(());
                }

                let capture = {
                    let mut viewer = viewer.borrow_mut();
                    if let Some(ViewerSource::Legacy {
                        open: true,
                        data_start,
                        ..
                    }) = viewer.as_mut()
                    {
                        if data_start.is_none() {
                            *data_start = Some(el.source_location().bytes().end);
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                };
                if capture {
                    let viewer = Rc::clone(&viewer);
                    el.on_end_tag(end_tag!(move |end| {
                        if let Some(ViewerSource::Legacy { data_end, .. }) =
                            viewer.borrow_mut().as_mut()
                        {
                            *data_end = Some(end.source_location().bytes().start);
                        }
                        Ok(())
                    }))?;
                }
                Ok(())
            }));
        let mut rewriter = HtmlRewriter::new(settings, |_: &[u8]| {});
        for chunk in bytes.chunks(16 * 1024) {
            rewriter.write(chunk).map_err(parse_error)?;
        }
        rewriter.end().map_err(parse_error)?;
    }

    let viewer = viewer.borrow();
    match viewer.as_ref().ok_or(DrawIoHtmlError::NoViewer)? {
        ViewerSource::Inline(data) => {
            let decoded = html_escape::decode_html_entities(data);
            let viewer: Viewer =
                serde_json::from_str(&decoded).map_err(|_| DrawIoHtmlError::InvalidViewer)?;
            viewer
                .xml
                .filter(|xml| !xml.trim().is_empty())
                .ok_or_else(|| DrawIoHtmlError::NoEmbeddedDiagram.into())
        }
        ViewerSource::Legacy {
            closed,
            data_start,
            data_end,
            ..
        } => {
            if !closed {
                return Err(DrawIoHtmlError::InvalidMarkup.into());
            }
            let Some(start) = data_start else {
                return Err(DrawIoHtmlError::NoEmbeddedDiagram.into());
            };
            let end = data_end.ok_or(DrawIoHtmlError::InvalidMarkup)?;
            // Parser offsets preserve character references split across input chunks.
            let raw = html
                .get(*start..end)
                .ok_or(DrawIoHtmlError::InvalidMarkup)?;
            if raw.contains('<') {
                return Err(DrawIoHtmlError::InvalidMarkup.into());
            }
            let decoded = html_escape::decode_html_entities(raw);
            let data = decoded.trim().trim_start_matches('\u{feff}').trim_start();
            if data.is_empty() {
                return Err(DrawIoHtmlError::NoEmbeddedDiagram.into());
            }
            if data.starts_with('<') {
                Ok(data.to_owned())
            } else if data.starts_with('%') {
                percent_decode(data).ok_or_else(|| DrawIoHtmlError::InvalidViewer.into())
            } else {
                decode_compressed(data).map_err(|_| DrawIoHtmlError::InvalidViewer.into())
            }
        }
    }
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
