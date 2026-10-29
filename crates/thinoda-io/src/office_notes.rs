use thinoda_core::notes::{NoteDocument, NoteRun};

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
