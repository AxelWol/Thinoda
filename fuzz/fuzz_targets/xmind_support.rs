use std::io::{Cursor, Write};

pub fn package_input(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() > super::support::MAX_INPUT_SIZE {
        return None;
    }
    let (name, content, image) = if let Some(content) = data.strip_prefix(b"JSON\n") {
        ("content.json", content, None)
    } else if let Some(content) = data.strip_prefix(b"CONTENT_XML\n") {
        ("content.xml", content, None)
    } else if let Some(image) = data.strip_prefix(b"IMAGE_SVG\n") {
        (
            "content.json",
            IMAGE_SVG_CONTENT,
            Some(("resources/picture.svg", image)),
        )
    } else if let Some(image) = data.strip_prefix(b"IMAGE_SVGZ\n") {
        (
            "content.json",
            IMAGE_SVGZ_CONTENT,
            Some(("resources/picture.svgz", image)),
        )
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
    if let Some((path, bytes)) = image {
        archive
            .start_file(path, zip::write::SimpleFileOptions::default())
            .expect("fixed XMind image entry is valid");
        archive
            .write_all(bytes)
            .expect("writing to an in-memory archive cannot fail");
    }
    Some(
        archive
            .finish()
            .expect("fixed XMind seed archive is valid")
            .into_inner(),
    )
}

const IMAGE_SVG_CONTENT: &[u8] = br#"[{"rootTopic":{"title":"Root","href":"xap:resources/picture.svg","image":{"src":"xap:resources/picture.svg"}}}]"#;
const IMAGE_SVGZ_CONTENT: &[u8] = br#"[{"rootTopic":{"title":"Root","href":"xap:resources/picture.svgz","image":{"src":"xap:resources/picture.svgz"}}}]"#;
