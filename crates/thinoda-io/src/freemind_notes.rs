use thinoda_core::notes::{NoteDocument, NoteRun};

fn run_xml(run: &NoteRun, output: &mut String) {
    let tags = [
        ("b", run.style.bold),
        ("i", run.style.italic),
        ("strike", run.style.strike),
        ("code", run.style.code),
    ];
    for (tag, enabled) in tags {
        if enabled {
            output.push_str(&format!("<{tag}>"));
        }
    }
    for (index, line) in run.text.split('\n').enumerate() {
        if index > 0 {
            output.push_str("<br/>");
        }
        output.push_str(&crate::formats::escape(line));
    }
    for (tag, enabled) in tags.into_iter().rev() {
        if enabled {
            output.push_str(&format!("</{tag}>"));
        }
    }
}

/// Render only validated note blocks: no source HTML, URLs or external resources.
pub(crate) fn render(document: &NoteDocument) -> String {
    let mut output = String::from("<html><head></head><body>");
    let mut lists: Vec<usize> = Vec::new();

    fn close(document: &NoteDocument, lists: &mut Vec<usize>, output: &mut String) {
        if let Some(id) = lists.pop() {
            output.push_str("</li>");
            output.push_str(if document.lists[id].is_some() {
                "</ol>"
            } else {
                "</ul>"
            });
        }
    }

    for paragraph in &document.paragraphs {
        if let Some(position) = paragraph.list {
            while lists.len() > position.depth {
                close(document, &mut lists, &mut output);
            }
            if lists.len() == position.depth && lists.last() != Some(&position.list) {
                close(document, &mut lists, &mut output);
            }
            if lists.len() < position.depth {
                if let Some(start) = document.lists[position.list] {
                    output.push_str(&format!("<ol start=\"{start}\">"));
                } else {
                    output.push_str("<ul>");
                }
                output.push_str("<li>");
                lists.push(position.list);
            } else if position.marker {
                output.push_str("</li><li>");
            }
        } else {
            while !lists.is_empty() {
                close(document, &mut lists, &mut output);
            }
        }
        output.push_str("<p>");
        for run in &paragraph.runs {
            run_xml(run, &mut output);
        }
        output.push_str("</p>");
    }
    while !lists.is_empty() {
        close(document, &mut lists, &mut output);
    }
    output.push_str("</body></html>");
    output
}
