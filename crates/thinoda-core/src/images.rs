//! Local PNG/JPEG only. Never resolves a URL or a filesystem reference.
use crate::{Sheet, Vec2, resources::MAX_RESOURCE_BYTES};
use image::{ImageDecoder, ImageEncoder, ImageFormat, ImageReader, metadata::Orientation};
use std::io::{Cursor, Write};

pub const MAX_DIMENSION: u32 = 4096;
pub const MAX_PIXELS: u64 = 4 * 1024 * 1024;
pub const MAX_DISPLAY_SIZE: f32 = 1024.0;

#[derive(Debug)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub media_type: &'static str,
}

impl DecodedImage {
    /// Bounded interactive preview; the document retains its original bytes.
    pub fn preview(self) -> Self {
        let image = image::RgbaImage::from_raw(self.width, self.height, self.rgba)
            .expect("decoder produced complete RGBA pixels");
        let image = if self.width > 512 || self.height > 512 {
            image::DynamicImage::ImageRgba8(image)
                .thumbnail(512, 512)
                .into_rgba8()
        } else {
            image
        };
        Self {
            width: image.width(),
            height: image.height(),
            rgba: image.into_raw(),
            media_type: self.media_type,
        }
    }
}

fn reader(bytes: &[u8], format: ImageFormat) -> ImageReader<Cursor<&[u8]>> {
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    reader
}

struct ImageMetadata {
    format: ImageFormat,
    media_type: &'static str,
    orientation: Orientation,
}

fn inspect(bytes: &[u8]) -> Result<ImageMetadata, String> {
    if bytes.is_empty() || bytes.len() > MAX_RESOURCE_BYTES {
        return Err("image must contain 1 byte to 8 MiB".into());
    }
    let format = image::guess_format(bytes).map_err(|_| "unsupported image; use PNG or JPEG")?;
    let media_type = match format {
        ImageFormat::Png => "image/png",
        ImageFormat::Jpeg => "image/jpeg",
        _ => return Err("unsupported image; use PNG or JPEG".into()),
    };
    // Header inspection precedes full pixel allocation, including RGBA conversion.
    let mut decoder = reader(bytes, format)
        .into_decoder()
        .map_err(|e| format!("invalid image header: {e}"))?;
    let (width, height) = decoder.dimensions();
    if width == 0
        || height == 0
        || width > MAX_DIMENSION
        || height > MAX_DIMENSION
        || u64::from(width) * u64::from(height) > MAX_PIXELS
    {
        return Err("image exceeds 4096 pixels per side or 4 megapixels".into());
    }
    let orientation = decoder
        .orientation()
        .map_err(|e| format!("invalid image metadata: {e}"))?;
    Ok(ImageMetadata {
        format,
        media_type,
        orientation,
    })
}

fn decode_pixels(bytes: &[u8], metadata: &ImageMetadata) -> Result<image::RgbaImage, String> {
    let mut image = reader(bytes, metadata.format)
        .decode()
        .map_err(|e| format!("invalid image: {e}"))?;
    image.apply_orientation(metadata.orientation);
    Ok(image.into_rgba8())
}

pub fn decode(bytes: &[u8]) -> Result<DecodedImage, String> {
    let metadata = inspect(bytes)?;
    let image = decode_pixels(bytes, &metadata)?;
    Ok(DecodedImage {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
        media_type: metadata.media_type,
    })
}

struct LimitedImageOutput {
    bytes: Vec<u8>,
    limit: usize,
    overflowed: bool,
}

impl Write for LimitedImageOutput {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if buf.len() > self.limit.saturating_sub(self.bytes.len()) {
            self.overflowed = true;
            return Err(std::io::Error::other(
                "normalized images exceed the visual export size limit",
            ));
        }
        self.bytes.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Whether visual consumers need pixels normalized to avoid differing EXIF handling.
pub fn requires_visual_normalization(bytes: &[u8]) -> Result<bool, String> {
    Ok(inspect(bytes)?.orientation != Orientation::NoTransforms)
}

/// Re-encodes transformed pixels for visual consumers that may handle EXIF differently.
/// `None` retains the source bytes and their metadata unchanged.
pub fn normalized_visual_image(bytes: &[u8], limit: usize) -> Result<Option<Vec<u8>>, String> {
    let metadata = inspect(bytes)?;
    if metadata.orientation == Orientation::NoTransforms {
        return Ok(None);
    }
    let image = decode_pixels(bytes, &metadata)?;
    let mut output = LimitedImageOutput {
        bytes: Vec::new(),
        limit,
        overflowed: false,
    };
    image::codecs::png::PngEncoder::new(&mut output)
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|error| format!("could not normalize image orientation: {error}"))?;
    // The PNG encoder can encounter a write error while finalizing its stream.
    if output.overflowed {
        return Err("normalized images exceed the visual export size limit".into());
    }
    Ok(Some(output.bytes))
}

/// Default fit, without upscaling small source images.
pub fn default_size(width: u32, height: u32) -> Vec2 {
    let scale = (160.0 / width as f32).min(100.0 / height as f32).min(1.0);
    Vec2::new(width as f32 * scale, height as f32 * scale)
}

pub fn valid_size(size: Vec2) -> bool {
    size.x.is_finite()
        && size.y.is_finite()
        && size.x > 0.0
        && size.y > 0.0
        && size.x <= MAX_DISPLAY_SIZE
        && size.y <= MAX_DISPLAY_SIZE
}

/// Replacing/removing frees only the old image when its last topic reference goes away.
pub fn set_image(
    sheet: &mut Sheet,
    topic: crate::TopicId,
    image: Option<(crate::resources::Resource, Vec2)>,
) -> Result<(), String> {
    let old = sheet
        .topic(topic)
        .ok_or("image topic is missing")?
        .image
        .clone();
    // Drop the old reference first; Document's edit transaction restores it on failure.
    let t = sheet
        .topics
        .get_mut(&topic)
        .ok_or("image topic is missing")?;
    t.image = None;
    t.image_size = None;
    t.image_below = false;
    if let Some(old) = old
        && !sheet.topics.values().any(|t| {
            t.image.as_ref() == Some(&old)
                || t.attachment.as_ref().is_some_and(|a| a.resource == old)
        })
        && let Some(id) = old.strip_prefix(crate::resources::RESOURCE_PREFIX)
    {
        sheet.resources.entries.remove(id);
    }
    if let Some((resource, size)) = image {
        if !valid_size(size) {
            return Err("invalid image display size".into());
        }
        let media_type = resource.media_type.clone();
        let reference = sheet.resources.insert(resource)?;
        // An opaque attachment may already own these exact bytes. Only an explicit,
        // bounded image decode may promote its generic metadata; filename suffixes
        // never establish the type. Per-topic attachment filenames remain unchanged.
        if let Some(id) = reference.strip_prefix(crate::resources::RESOURCE_PREFIX)
            && let Some(stored) = sheet.resources.entries.get_mut(id)
            && stored.media_type == "application/octet-stream"
            && matches!(media_type.as_str(), "image/png" | "image/jpeg")
        {
            if decode(stored.bytes())?.media_type != media_type {
                return Err("image media type does not match its content".into());
            }
            stored.media_type = media_type;
        }
        let t = sheet
            .topics
            .get_mut(&topic)
            .ok_or("image topic is missing")?;
        t.image = Some(reference);
        t.image_size = Some(size);
    }
    Ok(())
}

/// Visual outputs must not silently omit unsupported resource kinds. Unreferenced
/// PNG/JPEG blobs retained by older edits have no visible placement.
pub fn validate_visual(sheet: &Sheet) -> Result<std::collections::BTreeMap<String, Vec2>, String> {
    if sheet.topics.values().any(|t| t.attachment.is_some()) {
        return Err("visual output does not support attachment topics yet".into());
    }
    let mut dimensions = std::collections::BTreeMap::new();
    for (id, resource) in &sheet.resources.entries {
        let decoded = decode(resource.bytes())?;
        if resource.media_type != decoded.media_type {
            return Err("image media type does not match its content".into());
        }
        dimensions.insert(
            format!("{}{id}", crate::resources::RESOURCE_PREFIX),
            Vec2::new(decoded.width as f32, decoded.height as f32),
        );
    }
    let mut referenced = std::collections::BTreeSet::new();
    for topic in sheet.topics.values() {
        if let Some(reference) = &topic.image {
            sheet.resources.get(reference)?;
            referenced.insert(reference);
        }
    }
    dimensions.retain(|reference, _| referenced.contains(reference));
    Ok(dimensions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Document, Workbook, resources::Resource};
    use image::{ExtendedColorType, ImageEncoder, codecs::jpeg::JpegEncoder};

    fn encoded(format: ImageFormat) -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            40,
            20,
            image::Rgb([240, 20, 30]),
        ))
        .write_to(&mut bytes, format)
        .unwrap();
        bytes.into_inner()
    }

    fn oriented_image(format: ImageFormat, orientation: u8) -> Vec<u8> {
        let pixels = image::RgbImage::from_fn(80, 40, |x, y| {
            image::Rgb(match (x < 40, y < 20) {
                (true, true) => [240, 20, 30],
                (false, true) => [20, 230, 40],
                (true, false) => [30, 40, 240],
                (false, false) => [235, 220, 20],
            })
        });
        let exif = vec![
            b'I',
            b'I',
            42,
            0,
            8,
            0,
            0,
            0,
            1,
            0,
            0x12,
            0x01,
            3,
            0,
            1,
            0,
            0,
            0,
            orientation,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ];
        let mut bytes = Vec::new();
        match format {
            ImageFormat::Jpeg => {
                let mut encoder = JpegEncoder::new_with_quality(&mut bytes, 100);
                encoder.set_exif_metadata(exif).unwrap();
                encoder
                    .encode(pixels.as_raw(), 80, 40, ExtendedColorType::Rgb8)
                    .unwrap();
            }
            ImageFormat::Png => {
                let mut encoder = image::codecs::png::PngEncoder::new(&mut bytes);
                encoder.set_exif_metadata(exif).unwrap();
                encoder
                    .write_image(pixels.as_raw(), 80, 40, ExtendedColorType::Rgb8)
                    .unwrap();
            }
            _ => panic!("unsupported fixture"),
        }
        bytes
    }

    #[test]
    fn camera_orientations_rotate_and_mirror_decoded_pixels() {
        let expected = [
            [0, 1, 2, 3],
            [1, 0, 3, 2],
            [3, 2, 1, 0],
            [2, 3, 0, 1],
            [0, 2, 1, 3],
            [2, 0, 3, 1],
            [3, 1, 2, 0],
            [1, 3, 0, 2],
        ];
        for format in [ImageFormat::Jpeg, ImageFormat::Png] {
            for (index, corners) in expected.iter().enumerate() {
                let orientation = index as u8 + 1;
                let bytes = oriented_image(format, orientation);
                let raw = image::load_from_memory(&bytes).unwrap().into_rgba8();
                let source = [
                    raw.get_pixel(10, 10).0,
                    raw.get_pixel(69, 10).0,
                    raw.get_pixel(10, 29).0,
                    raw.get_pixel(69, 29).0,
                ];
                let decoded = decode(&bytes).unwrap();
                let dimensions = if orientation >= 5 { (40, 80) } else { (80, 40) };
                assert_eq!(
                    (decoded.width, decoded.height),
                    dimensions,
                    "{format:?} orientation {orientation}"
                );
                let image = image::RgbaImage::from_raw(decoded.width, decoded.height, decoded.rgba)
                    .unwrap();
                for ((x, y), &source_index) in [
                    (10, 10),
                    (dimensions.0 - 11, 10),
                    (10, dimensions.1 - 11),
                    (dimensions.0 - 11, dimensions.1 - 11),
                ]
                .into_iter()
                .zip(corners)
                {
                    assert_eq!(
                        image.get_pixel(x, y).0,
                        source[source_index],
                        "{format:?} orientation {orientation} at {x},{y}"
                    );
                }
            }
        }
    }

    #[test]
    fn oriented_visual_encoding_is_lossless_bounded_and_leaves_identity_unchanged() {
        for format in [ImageFormat::Png, ImageFormat::Jpeg] {
            for orientation in [0, 1, 2, 5, 6, 8, 9] {
                let original = oriented_image(format, orientation);
                let decoded = decode(&original).unwrap();
                assert_eq!(
                    requires_visual_normalization(&original).unwrap(),
                    !matches!(orientation, 0 | 1 | 9)
                );
                let normalized = normalized_visual_image(&original, 1024 * 1024).unwrap();
                if matches!(orientation, 0 | 1 | 9) {
                    assert!(normalized.is_none());
                    assert!(normalized_visual_image(&original, 0).unwrap().is_none());
                } else {
                    let normalized = normalized.unwrap();
                    let png = image::load_from_memory_with_format(&normalized, ImageFormat::Png)
                        .unwrap()
                        .into_rgba8();
                    assert_eq!((png.width(), png.height()), (decoded.width, decoded.height));
                    assert_eq!(png.as_raw(), &decoded.rgba);
                    assert!(normalized_visual_image(&original, normalized.len() - 1).is_err());
                    assert_eq!(
                        normalized_visual_image(&original, normalized.len()).unwrap(),
                        Some(normalized)
                    );
                }
                assert_eq!(
                    decode(&original).unwrap().rgba,
                    decoded.rgba,
                    "the source image must remain unchanged"
                );
            }
        }
        assert!(normalized_visual_image(b"invalid", 1024).is_err());
        assert!(requires_visual_normalization(b"invalid").is_err());
    }

    #[test]
    fn bounded_png_jpeg_decode_and_preview() {
        for format in [ImageFormat::Png, ImageFormat::Jpeg] {
            let bytes = encoded(format);
            let decoded = decode(&bytes).unwrap();
            assert_eq!((decoded.width, decoded.height), (40, 20));
            assert_eq!(decoded.rgba.len(), 40 * 20 * 4);
            assert_eq!(default_size(400, 200), Vec2::new(160.0, 80.0));
            assert_eq!(default_size(40, 20), Vec2::new(40.0, 20.0));
            assert_eq!(decoded.preview().rgba.len(), 40 * 20 * 4);
            assert!(decode(&bytes[..bytes.len() / 2]).is_err());
        }

        for bytes in [b"GIF89a".as_slice(), b"<svg/>", b"", b"not an image"] {
            assert!(decode(bytes).is_err());
        }
        assert!(decode(&vec![0; MAX_RESOURCE_BYTES + 1]).is_err());
    }

    #[test]
    fn image_after_opaque_attachment_promotes_only_verified_type_and_undo_restores_it() {
        let bytes = encoded(ImageFormat::Png);
        let mut sheet = crate::Sheet::new("Evidence", "Root");
        let root = sheet.root;
        let attachment =
            crate::attachments::insert(&mut sheet, root, "evidence.bin", bytes.clone()).unwrap();
        let reference = sheet
            .topic(attachment)
            .unwrap()
            .attachment
            .as_ref()
            .unwrap()
            .resource
            .clone();
        let mut doc = Document::new(Workbook::new(sheet));
        doc.edit_sheet("Image", |s| {
            set_image(
                s,
                root,
                Some((
                    Resource::new("image.png", "image/png", bytes.clone()).unwrap(),
                    default_size(40, 20),
                )),
            )
            .map_err(crate::edit::EditError::Invalid)
        })
        .unwrap();
        assert_eq!(doc.sheet().resources.entries.len(), 1);
        assert_eq!(
            doc.sheet().resources.get(&reference).unwrap().media_type,
            "image/png"
        );
        assert_eq!(
            doc.sheet().resources.get(&reference).unwrap().bytes(),
            bytes
        );
        assert_eq!(
            doc.sheet()
                .topic(attachment)
                .unwrap()
                .attachment
                .as_ref()
                .unwrap()
                .name,
            "evidence.bin"
        );
        doc.undo();
        assert_eq!(
            doc.sheet().resources.get(&reference).unwrap().media_type,
            "application/octet-stream"
        );
        let before = doc.workbook().clone();
        assert!(
            doc.edit_sheet("Wrong type", |s| {
                set_image(
                    s,
                    root,
                    Some((
                        Resource::new("image.jpg", "image/jpeg", bytes).unwrap(),
                        default_size(40, 20),
                    )),
                )
                .map_err(crate::edit::EditError::Invalid)
            })
            .is_err()
        );
        assert_eq!(doc.workbook(), &before);
    }

    #[test]
    fn oversized_header_is_rejected_before_decode_allocation() {
        // A tiny compressed grayscale image with too many pixels; the header is
        // valid and decode must reject it despite its small encoded size.
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::ImageLuma8(image::GrayImage::new(4096, 1025))
            .write_to(&mut bytes, ImageFormat::Png)
            .unwrap();
        assert!(
            decode(bytes.get_ref())
                .unwrap_err()
                .contains("4 megapixels")
        );
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::ImageLuma8(image::GrayImage::new(4097, 1))
            .write_to(&mut bytes, ImageFormat::Png)
            .unwrap();
        assert!(decode(bytes.get_ref()).is_err());
    }

    #[test]
    fn image_edits_preserve_shared_bytes_clipboard_and_undo() {
        let mut doc = Document::new(Workbook::default());
        let id = doc.sheet().root;
        let resource =
            Resource::new("evidence.png", "image/png", encoded(ImageFormat::Png)).unwrap();
        doc.edit_sheet("Image", |s| {
            set_image(s, id, Some((resource, Vec2::new(40.0, 20.0))))
                .map_err(crate::edit::EditError::Invalid)
        })
        .unwrap();
        let with_image = doc.sheet().clone();
        let clipboard = crate::clipboard::encode_sheet(doc.sheet()).unwrap();
        assert_eq!(
            crate::clipboard::decode_sheet(&clipboard).unwrap(),
            with_image
        );
        let child = doc.edit_sheet("Duplicate", |s| s.duplicate(id)).unwrap();
        doc.edit_sheet("Remove", |s| {
            set_image(s, id, None).map_err(crate::edit::EditError::Invalid)
        })
        .unwrap();
        assert_eq!(doc.sheet().resources.entries.len(), 1);
        assert!(doc.sheet().topic(child).unwrap().image.is_some());
        doc.undo();
        assert!(doc.sheet().topic(id).unwrap().image.is_some());
        doc.redo();
        doc.edit_sheet("Remove", |s| {
            set_image(s, child, None).map_err(crate::edit::EditError::Invalid)
        })
        .unwrap();
        assert!(doc.sheet().resources.is_empty());
        doc.undo();
        assert_eq!(doc.sheet().resources.entries.len(), 1);
    }

    #[test]
    fn geometry_is_defaulted_bounded_and_transactional() {
        let mut doc = Document::new(Workbook::default());
        let root = doc.sheet().root;
        for size in [
            Vec2::new(f32::NAN, 1.0),
            Vec2::new(f32::INFINITY, 1.0),
            Vec2::new(-1.0, 1.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1025.0, 1.0),
        ] {
            let before = doc.workbook().clone();
            assert!(
                doc.edit_sheet("Bad size", |s| {
                    s.topic_mut(root).unwrap().image_size = Some(size);
                    Ok(())
                })
                .is_err()
            );
            assert_eq!(doc.workbook(), &before);
        }
        let json = serde_json::to_value(doc.sheet().root_topic()).unwrap();
        assert!(json.get("image_size").is_none());
        assert!(json.get("image_below").is_none());
        let topic: crate::Topic = serde_json::from_value(json).unwrap();
        assert_eq!(topic.image_size, None);
        assert!(!topic.image_below);
    }

    #[test]
    fn image_layout_is_centered_above_or_below_and_resizes() {
        let mut sheet = Sheet::new("Image", "Selected topic");
        let root = sheet.root;
        sheet.topic_mut(root).unwrap().image = Some("legacy.png".into());
        for dark in [false, true] {
            for below in [false, true] {
                for size in [Vec2::new(40.0, 20.0), Vec2::new(320.0, 160.0)] {
                    let topic = sheet.topic_mut(root).unwrap();
                    topic.image_size = Some(size);
                    topic.image_below = below;
                    topic.labels = vec!["Label".into()];
                    let layout =
                        crate::layout::layout_sheet(&sheet, &crate::layout::ApproxMeasure, dark);
                    let topic = &layout.topics[&root];
                    let image = topic.image_rect.unwrap();
                    assert_eq!(image.size, size);
                    assert!((image.center().x - topic.rect.center().x).abs() < 1.0);
                    if below {
                        assert!(image.min.y > topic.title_rect.max().y);
                        assert!(image.min.y > topic.labels[0].1.max().y);
                    } else {
                        assert!(image.max().y < topic.title_rect.min.y);
                    }
                    assert!(image.min.y >= topic.rect.min.y);
                    assert!(image.max().y <= topic.rect.max().y);
                }
            }
        }
    }
}
