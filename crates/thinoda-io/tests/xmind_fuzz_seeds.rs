#[path = "../../../fuzz/fuzz_targets/support.rs"]
mod support;
#[path = "../../../fuzz/fuzz_targets/xmind_support.rs"]
mod xmind_support;

use thinoda_io::{Format, ImportWarning, import_bytes_with_report};

#[test]
fn svg_picture_seeds_reach_image_conversion_and_preserve_attachments() {
    for seed in [
        include_bytes!("../../../fuzz/corpus/xmind/image-svg").as_slice(),
        include_bytes!("../../../fuzz/corpus/xmind/image-svgz").as_slice(),
    ] {
        let package = xmind_support::package_input(seed).unwrap();
        let report = import_bytes_with_report(Format::XMind, &package).unwrap();
        assert!(report.warnings.contains(&ImportWarning::Resources));
        let sheet = report.workbook.active();
        assert!(sheet.root_topic().image.is_some());
        let attachment = sheet.root_topic().attachment.as_ref().unwrap();
        let original = &seed[seed.iter().position(|&byte| byte == b'\n').unwrap() + 1..];
        assert_eq!(
            sheet.resources.get(&attachment.resource).unwrap().bytes(),
            original
        );
        support::roundtrip(Format::XMind, &package);
    }
}

#[test]
fn malformed_svgz_seed_keeps_the_original_attachment() {
    let seed = b"IMAGE_SVGZ\n\x1f\x8b\x08";
    let package = xmind_support::package_input(seed).unwrap();
    let report = import_bytes_with_report(Format::XMind, &package).unwrap();
    assert!(report.warnings.contains(&ImportWarning::Resources));
    let sheet = report.workbook.active();
    assert!(sheet.root_topic().image.is_none());
    let attachment = sheet.root_topic().attachment.as_ref().unwrap();
    assert_eq!(
        sheet.resources.get(&attachment.resource).unwrap().bytes(),
        &seed[b"IMAGE_SVGZ\n".len()..]
    );
}
