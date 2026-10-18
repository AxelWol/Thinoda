use std::io::{Cursor, Write};

pub fn package_input(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() > super::support::MAX_INPUT_SIZE {
        return None;
    }
    let (name, content) = if let Some(content) = data.strip_prefix(b"JSON\n") {
        ("content.json", content)
    } else if let Some(content) = data.strip_prefix(b"CONTENT_XML\n") {
        ("content.xml", content)
    } else {
        return Some(data.to_vec());
    };

    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    archive
        .start_file(name, zip::write::SimpleFileOptions::default())
        .expect("fixed XMind seed entry is valid");
    archive
        .write_all(content)
        .expect("writing to an in-memory archive cannot fail");
    Some(
        archive
            .finish()
            .expect("fixed XMind seed archive is valid")
            .into_inner(),
    )
}
