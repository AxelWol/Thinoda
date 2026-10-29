use thinoda_core::notes::{
    MAX_PROJECTION_BYTES, MAX_PROJECTION_DEPTH, MAX_PROJECTION_EVENTS, NoteContent, NoteDocument,
    NoteListPosition, NoteParagraph, NoteRun, NoteRunStyle, formatted_document,
};

#[derive(Default)]
struct Html {
    name: String,
    start: Option<u32>,
    children: Vec<Html>,
    text: String,
}

/// Consume the NOTE envelope without letting its contents affect the map tree.
/// This is XML tokenization only: no HTML execution, DTD/entity expansion or IO.
/// Keep the readable fallback even after the bounded rich conversion is abandoned.
pub(crate) fn read(
    reader: &mut quick_xml::Reader<&[u8]>,
    supported_envelope: bool,
) -> Result<(NoteContent, bool), crate::IoError> {
    use quick_xml::events::Event;
    let begin = reader.buffer_position();
    let mut tree = supported_envelope.then(|| vec![Html::default()]);
    let mut depth = 0usize;
    let mut events = 0usize;
    let mut fallback = String::new();
    loop {
        let event = reader
            .read_event()
            .map_err(|error| crate::IoError::Xml(error.to_string()))?;
        events += 1;
        if reader.buffer_position() - begin > MAX_PROJECTION_BYTES as u64
            || events > MAX_PROJECTION_EVENTS
            || depth > MAX_PROJECTION_DEPTH
        {
            tree = None;
        }
        match event {
            Event::Start(ref element) | Event::Empty(ref element) => {
                let name = element.name().as_ref().to_ascii_lowercase();
                if name == "br" {
                    fallback.push('\n');
                }
                let mut node = Html {
                    name: name.clone(),
                    ..Default::default()
                };
                if !matches!(
                    name.as_str(),
                    "html"
                        | "head"
                        | "body"
                        | "p"
                        | "b"
                        | "strong"
                        | "i"
                        | "em"
                        | "strike"
                        | "s"
                        | "del"
                        | "code"
                        | "br"
                        | "ol"
                        | "ul"
                        | "li"
                ) {
                    tree = None;
                }
                for attribute in element.attributes() {
                    let attribute =
                        attribute.map_err(|error| crate::IoError::Xml(error.to_string()))?;
                    let value = attribute
                        .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                        .map_err(|error| crate::IoError::Xml(error.to_string()))?;
                    if name == "ol" && attribute.key.as_ref().eq_ignore_ascii_case("start") {
                        node.start = value.parse::<u32>().ok().filter(|n| *n <= i32::MAX as u32);
                        if node.start.is_none() {
                            tree = None;
                        }
                    } else {
                        tree = None;
                    }
                }
                if matches!(event, Event::Start(_)) {
                    depth += 1;
                    if depth > MAX_PROJECTION_DEPTH {
                        tree = None;
                    }
                    if let Some(stack) = &mut tree {
                        stack.push(node);
                    }
                } else if let Some(stack) = &mut tree {
                    stack.last_mut().unwrap().children.push(node);
                }
            }
            Event::End(element) => {
                if depth == 0 && element.name().as_ref().eq_ignore_ascii_case("richcontent") {
                    break;
                }
                depth = depth.saturating_sub(1);
                let name = element.name().as_ref().to_ascii_lowercase();
                if matches!(name.as_str(), "p" | "li") && !fallback.ends_with('\n') {
                    fallback.push('\n');
                }
                if let Some(stack) = &mut tree {
                    let node = stack.pop().unwrap();
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(node);
                    } else {
                        return Err(crate::IoError::Xml("unbalanced rich note".into()));
                    }
                }
            }
            Event::Text(text) => {
                let text = text.xml_content(quick_xml::XmlVersion::Implicit1_0);
                append_text(&mut tree, &mut fallback, &text);
            }
            Event::CData(text) => {
                let text = text.xml_content(quick_xml::XmlVersion::Implicit1_0);
                append_text(&mut tree, &mut fallback, &text);
            }
            Event::GeneralRef(reference) => {
                let value = reference
                    .resolve_char_ref()
                    .map_err(|error| crate::IoError::Xml(error.to_string()))?
                    .map(|character| character.to_string())
                    .or_else(|| {
                        quick_xml::escape::resolve_predefined_entity(reference.as_ref())
                            .map(str::to_owned)
                    })
                    .ok_or_else(|| crate::IoError::Xml("unknown XML entity".into()))?;
                append_text(&mut tree, &mut fallback, &value);
            }
            Event::Eof => return Err(crate::IoError::Xml("unclosed rich note".into())),
            // Comments, processing instructions and DTDs are not fidelity evidence.
            _ => tree = None,
        }
    }
    if let Some(stack) = tree
        && stack.len() == 1
        && let Some(result) = import_html(&stack[0])
    {
        return Ok(result);
    }
    Ok((NoteContent::Plain(fallback.trim().to_owned()), true))
}

fn append_text(tree: &mut Option<Vec<Html>>, fallback: &mut String, text: &str) {
    fallback.push_str(text);
    if let Some(stack) = tree {
        let parent = stack.last_mut().unwrap();
        if let Some(last) = parent.children.last_mut()
            && last.name.is_empty()
        {
            last.text.push_str(text);
        } else {
            parent.children.push(Html {
                text: text.to_owned(),
                ..Default::default()
            });
        }
    }
}

fn significant(node: &Html) -> impl Iterator<Item = &Html> {
    node.children.iter().filter(|child| {
        !child.name.is_empty()
            || !child
                .text
                .chars()
                .all(|c| matches!(c, ' ' | '\t' | '\r' | '\n'))
    })
}

fn import_html(root: &Html) -> Option<(NoteContent, bool)> {
    let roots: Vec<_> = significant(root).collect();
    let [html] = roots.as_slice() else {
        return None;
    };
    if html.name != "html" {
        return None;
    }
    let children: Vec<_> = significant(html).collect();
    let body = match children.as_slice() {
        [body] => *body,
        [head, body] if head.name == "head" && significant(head).next().is_none() => *body,
        _ => return None,
    };
    if body.name != "body" {
        return None;
    }
    let blocks: Vec<_> = significant(body).collect();
    // The historical plain-note envelope is deliberately NOT inferred as Markdown.
    // Keep literal punctuation, raw newlines and repeated spaces in these notes.
    if let [paragraph] = blocks.as_slice()
        && paragraph.name == "p"
        && paragraph.children.iter().all(|child| child.name.is_empty())
    {
        let text: String = paragraph
            .children
            .iter()
            .map(|child| child.text.as_str())
            .collect();
        return Some((NoteContent::Plain(text), false));
    }
    let mut document = NoteDocument::default();
    blocks_to_document(body, &mut document, &mut Vec::new())?;
    // HTML collapses these spaces; do not claim a visual/semantic round trip.
    for paragraph in &document.paragraphs {
        let text: String = paragraph.runs.iter().map(|run| run.text.as_str()).collect();
        if text.contains("  ") || text.contains('\t') {
            return None;
        }
    }
    let source = markdown(&document)?;
    let reparsed = formatted_document(&source).ok()?;
    // The serializer is not a second Markdown parser. Require the actual shared
    // parser to prove all text, styles, breaks and list positions survived.
    if canonical(reparsed) != canonical(document) {
        return None;
    }
    Some((NoteContent::markdown(source), false))
}

fn begin_paragraph(lists: &mut [(usize, bool)]) -> NoteParagraph {
    let depth = lists.len();
    let list = lists.last_mut().map(|(list, pending)| {
        let position = NoteListPosition {
            list: *list,
            depth,
            marker: *pending,
        };
        *pending = false;
        position
    });
    NoteParagraph {
        runs: Vec::new(),
        list,
    }
}

fn blocks_to_document(
    node: &Html,
    document: &mut NoteDocument,
    lists: &mut Vec<(usize, bool)>,
) -> Option<()> {
    for child in significant(node) {
        match child.name.as_str() {
            "p" => {
                let mut paragraph = begin_paragraph(lists);
                inline(child, NoteRunStyle::default(), &mut paragraph.runs)?;
                document.paragraphs.push(paragraph);
            }
            "ol" | "ul" => {
                if lists.len() >= 9 {
                    return None;
                }
                if lists.last().is_some_and(|(_, pending)| *pending) {
                    document.paragraphs.push(begin_paragraph(lists));
                }
                let id = document.lists.len();
                document.lists.push(if child.name == "ol" {
                    Some(child.start.unwrap_or(1))
                } else {
                    None
                });
                lists.push((id, false));
                let mut items = 0;
                for item in significant(child) {
                    if item.name != "li" {
                        return None;
                    }
                    lists.last_mut()?.1 = true;
                    // Common HTML permits inline children directly inside li.
                    // Mixed inline/block children are rejected, not reordered.
                    if item.children.iter().all(|node| is_inline(&node.name)) {
                        let mut paragraph = begin_paragraph(lists);
                        inline(item, NoteRunStyle::default(), &mut paragraph.runs)?;
                        document.paragraphs.push(paragraph);
                    } else {
                        blocks_to_document(item, document, lists)?;
                    }
                    if lists.last()?.1 {
                        document.paragraphs.push(begin_paragraph(lists));
                    }
                    items += 1;
                }
                if items == 0 {
                    return None;
                }
                lists.pop();
            }
            _ => return None,
        }
    }
    Some(())
}

fn is_inline(name: &str) -> bool {
    matches!(
        name,
        "" | "b" | "strong" | "i" | "em" | "strike" | "s" | "del" | "code" | "br"
    )
}

fn inline(node: &Html, style: NoteRunStyle, runs: &mut Vec<NoteRun>) -> Option<()> {
    for child in &node.children {
        let mut style = style;
        match child.name.as_str() {
            "" => {
                if child.text.chars().any(|c| c.is_control()) {
                    return None;
                }
                runs.push(NoteRun {
                    text: child.text.clone(),
                    style,
                });
                continue;
            }
            "br" if child.children.is_empty() => {
                runs.push(NoteRun {
                    text: "\n".into(),
                    style,
                });
                continue;
            }
            "b" | "strong" => style.bold = true,
            "i" | "em" => style.italic = true,
            "strike" | "s" | "del" => style.strike = true,
            "code" => style.code = true,
            _ => return None,
        }
        inline(child, style, runs)?;
    }
    Some(())
}

fn canonical(mut document: NoteDocument) -> NoteDocument {
    for paragraph in &mut document.paragraphs {
        let mut runs: Vec<NoteRun> = Vec::new();
        for run in std::mem::take(&mut paragraph.runs) {
            if run.text.is_empty() {
                continue;
            }
            if let Some(last) = runs.last_mut()
                && last.style == run.style
            {
                last.text.push_str(&run.text);
            } else {
                runs.push(run);
            }
        }
        paragraph.runs = runs;
    }
    document
}

fn markdown(document: &NoteDocument) -> Option<String> {
    let mut output = String::new();
    let mut widths: Vec<usize> = Vec::new();
    let mut counters = document.lists.clone();
    let mut empty_parent = None;
    let mut last_lists: [Option<(usize, bool)>; 9] = [None; 9];
    for paragraph in &canonical(document.clone()).paragraphs {
        let (prefix, indent) = if let Some(position) = paragraph.list {
            widths.truncate(position.depth);
            if position.marker {
                let last = &mut last_lists[position.depth - 1];
                let alternate = match *last {
                    Some((id, alternate)) if id == position.list => alternate,
                    Some((_, alternate)) => !alternate,
                    None => false,
                };
                *last = Some((position.list, alternate));
                let marker = if let Some(counter) = &mut counters[position.list] {
                    let delimiter = if alternate { ')' } else { '.' };
                    let marker = format!("{counter}{delimiter} ");
                    *counter = counter.checked_add(1)?;
                    marker
                } else {
                    if alternate { "+ " } else { "- " }.into()
                };
                widths.truncate(position.depth - 1);
                let indent: usize = widths.iter().sum();
                widths.push(marker.len());
                (
                    format!("{}{marker}", " ".repeat(indent)),
                    widths.iter().sum(),
                )
            } else {
                let indent = widths.iter().sum();
                (" ".repeat(indent), indent)
            }
        } else {
            widths.clear();
            (String::new(), 0)
        };
        let mut text = String::new();
        let mut open: Vec<&str> = Vec::new();
        for run in &paragraph.runs {
            let style = run.style;
            let desired: Vec<_> = [
                (style.bold, "**"),
                (style.italic, "*"),
                (style.strike, "~~"),
            ]
            .into_iter()
            .filter_map(|(enabled, delimiter)| enabled.then_some(delimiter))
            .collect();
            let common = open
                .iter()
                .zip(&desired)
                .take_while(|(a, b)| a == b)
                .count();
            for delimiter in open[common..].iter().rev() {
                text.push_str(delimiter);
            }
            for delimiter in &desired[common..] {
                text.push_str(delimiter);
            }
            open = desired;
            if style.code {
                if run.text.contains('\n') {
                    return None;
                }
                let longest = run
                    .text
                    .split(|c| c != '`')
                    .map(str::len)
                    .max()
                    .unwrap_or(0);
                let fence = "`".repeat(longest + 1);
                let pad = run.text.starts_with(['`', ' ']) || run.text.ends_with(['`', ' ']);
                text.push_str(&fence);
                if pad {
                    text.push(' ');
                }
                text.push_str(&run.text);
                if pad {
                    text.push(' ');
                }
                text.push_str(&fence);
            } else {
                for (offset, character) in run.text.char_indices() {
                    match character {
                        '\n' => text.push_str("\\\n"),
                        // Entities protect delimiter-edge spaces and indentation.
                        ' ' => text.push_str("&#32;"),
                        c if c.is_ascii_punctuation() => {
                            text.push('\\');
                            text.push(c);
                        }
                        // Delimiter flanking is determined from source characters,
                        // not decoded entities. Protect adjacent style transitions,
                        // including intraword emphasis next to strike delimiters.
                        c if offset == 0 || offset + c.len_utf8() == run.text.len() => {
                            text.push_str(&format!("&#x{:x};", c as u32));
                        }
                        c => text.push(c),
                    }
                }
            }
        }
        for delimiter in open.iter().rev() {
            text.push_str(delimiter);
        }
        if !output.is_empty() {
            if empty_parent.is_some_and(|depth| paragraph.list.is_some_and(|p| p.depth > depth)) {
                output.push('\n');
            } else {
                output.push_str("\n\n");
            }
        }
        empty_parent = paragraph
            .list
            .filter(|p| p.marker && paragraph.runs.is_empty())
            .map(|p| p.depth);
        output.push_str(&prefix);
        output.push_str(&text.replace('\n', &format!("\n{}", " ".repeat(indent))));
        if output.len() > MAX_PROJECTION_BYTES {
            return None;
        }
    }
    Some(output)
}

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
