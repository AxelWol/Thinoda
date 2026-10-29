use thinoda_core::notes::{NoteDocument, NoteRun};

/// Escape SpreadsheetML's encoded-character syntax as well as XML markup.
pub(crate) fn excel_text(text: &str) -> String {
    let mut literal = String::new();
    for (index, character) in text.char_indices() {
        let suffix = &text.as_bytes()[index..];
        if character == '_'
            && suffix.len() >= 7
            && suffix[1] == b'x'
            && suffix[2..6].iter().all(u8::is_ascii_hexdigit)
            && suffix[6] == b'_'
        {
            literal.push_str("_x005F_");
        } else {
            literal.push(character);
        }
    }
    crate::office::esc(&literal).replace('\r', "&#13;")
}

/// Excel has no paragraph/list objects in cells; markers remain visible text.
pub(crate) fn excel(
    document: &NoteDocument,
) -> Result<String, thinoda_core::notes::NoteExportError> {
    if document
        .paragraphs
        .iter()
        .flat_map(|paragraph| &paragraph.runs)
        .any(|run| run.text.contains('\t'))
    {
        return Err(thinoda_core::notes::NoteExportError::Unsupported);
    }
    let mut output = String::new();
    let mut counters: Vec<_> = document
        .lists
        .iter()
        .map(|start| start.map(u64::from))
        .collect();
    let mut units = 0usize;
    let mut breaks = 0usize;
    let mut append = |text: &str, run: Option<&NoteRun>| {
        units += text.encode_utf16().count();
        breaks += text.bytes().filter(|byte| *byte == b'\n').count();
        if units > 32_767 || breaks > 253 {
            return Err(thinoda_core::notes::NoteExportError::Limit);
        }
        output.push_str("<r><rPr>");
        if let Some(run) = run {
            if run.style.code {
                output.push_str("<rFont val=\"Courier New\"/>");
            }
            if run.style.bold {
                output.push_str("<b/>");
            }
            if run.style.italic {
                output.push_str("<i/>");
            }
            if run.style.strike {
                output.push_str("<strike/>");
            }
        }
        output.push_str("</rPr><t xml:space=\"preserve\">");
        output.push_str(&excel_text(text));
        output.push_str("</t></r>");
        Ok(())
    };
    for (index, paragraph) in document.paragraphs.iter().enumerate() {
        if index > 0 {
            append("\n\n", None)?;
        }
        if let Some(position) = paragraph.list {
            let indent = "  ".repeat(position.depth.saturating_sub(1));
            let marker = if position.marker {
                if let Some(counter) = &mut counters[position.list] {
                    let marker = format!("{counter}. ");
                    *counter += 1;
                    marker
                } else {
                    "\u{2022} ".into()
                }
            } else {
                "  ".into()
            };
            append(&format!("{indent}{marker}"), None)?;
        }
        for run in &paragraph.runs {
            let text = run.text.replace("\r\n", "\n").replace('\r', "\n");
            append(&text, Some(run))?;
        }
    }
    // An empty formatted note is still a valid inline string.
    if output.is_empty() {
        output.push_str("<t/>");
    }
    Ok(output)
}

/// Numbering IDs are document-wide; separate note lists must not share counters.
#[derive(Default)]
pub(crate) struct WordNotes {
    numbering: String,
    count: usize,
}

fn run_xml(run: &NoteRun, output: &mut String) {
    output.push_str("<w:r><w:rPr>");
    if run.style.code {
        output.push_str("<w:rFonts w:ascii=\"Courier New\" w:hAnsi=\"Courier New\"/>");
    }
    if run.style.bold {
        output.push_str("<w:b/><w:bCs/>");
    }
    if run.style.italic {
        output.push_str("<w:i/><w:iCs/>");
    }
    if run.style.strike {
        output.push_str("<w:strike/>");
    }
    output.push_str("</w:rPr>");
    // XML normalizes CRs; Word line breaks and tabs need their own elements.
    let text = run.text.replace("\r\n", "\n").replace('\r', "\n");
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            output.push_str("<w:br/>");
        }
        for (index, text) in line.split('\t').enumerate() {
            if index > 0 {
                output.push_str("<w:tab/>");
            }
            output.push_str("<w:t xml:space=\"preserve\">");
            output.push_str(&crate::office::esc(text));
            output.push_str("</w:t>");
        }
    }
    output.push_str("</w:r>");
}

impl WordNotes {
    pub(crate) fn render(&mut self, document: &NoteDocument, body: &mut String) {
        let offset = self.count;
        for (index, start) in document.lists.iter().enumerate() {
            let id = offset + index + 1;
            let (format, label) = if start.is_some() {
                ("decimal", "%1.")
            } else {
                ("bullet", "&#8226;")
            };
            self.numbering.push_str(&format!(
                "<w:abstractNum w:abstractNumId=\"{id}\"><w:multiLevelType w:val=\"singleLevel\"/><w:lvl w:ilvl=\"0\"><w:start w:val=\"{}\"/><w:numFmt w:val=\"{format}\"/><w:lvlText w:val=\"{label}\"/><w:lvlJc w:val=\"left\"/></w:lvl></w:abstractNum>",
                start.unwrap_or(1)
            ));
        }

        for paragraph in &document.paragraphs {
            body.push_str("<w:p>");
            if let Some(position) = paragraph.list {
                body.push_str("<w:pPr>");
                if position.marker {
                    body.push_str(&format!(
                        "<w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"{}\"/></w:numPr>",
                        offset + position.list + 1
                    ));
                }
                body.push_str(&format!(
                    "<w:ind w:left=\"{}\" w:hanging=\"{}\"/></w:pPr>",
                    position.depth * 360,
                    if position.marker { 180 } else { 0 }
                ));
            }
            for run in &paragraph.runs {
                run_xml(run, body);
            }
            body.push_str("</w:p>");
        }
        self.count += document.lists.len();
    }

    pub(crate) fn numbering(self) -> Option<String> {
        if self.count == 0 {
            return None;
        }
        let mut output = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><w:numbering xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">{}",
            self.numbering
        );
        // All abstract definitions precede concrete instances in the schema.
        for id in 1..=self.count {
            output.push_str(&format!(
                "<w:num w:numId=\"{id}\"><w:abstractNumId w:val=\"{id}\"/></w:num>"
            ));
        }
        output.push_str("</w:numbering>");
        Some(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use thinoda_core::notes::{NoteExportError, NoteParagraph};

    fn document(text: String) -> NoteDocument {
        NoteDocument {
            paragraphs: vec![NoteParagraph {
                runs: vec![NoteRun {
                    text,
                    style: Default::default(),
                }],
                list: None,
            }],
            lists: Vec::new(),
        }
    }

    #[test]
    fn excel_counts_utf16_units_not_bytes_or_unicode_scalars() {
        assert!(excel(&document(format!("{}a", "\u{1f600}".repeat(16_383)))).is_ok());
        assert_eq!(
            excel(&document("\u{1f600}".repeat(16_384))),
            Err(NoteExportError::Limit),
        );
    }

    #[test]
    fn excel_empty_notes_and_literal_encoded_characters_are_valid() {
        assert_eq!(excel(&NoteDocument::default()).unwrap(), "<t/>");
        assert_eq!(
            excel_text("_x000A_ _x005f_ _x123_ _xyz12_ & <\r"),
            "_x005F_x000A_ _x005F_x005f_ _x123_ _xyz12_ &amp; &lt;&#13;",
        );
    }
}
