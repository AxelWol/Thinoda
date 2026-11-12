use thinoda_core::{
    ApproxMeasure, Sheet, Vec2, clipboard, layout_sheet,
    stencil_object::{find_package, new_object},
};

fn stencil_sheet() -> Sheet {
    let mut sheet = Sheet::new("Stencils", "Root");
    let package = find_package("thinoda.general").unwrap();
    let mut object = new_object(
        package,
        package.definition("message").unwrap(),
        Vec2::new(200.0, 120.0),
    );
    object
        .stencil
        .as_mut()
        .unwrap()
        .parameters
        .insert("dx".into(), 12.0);
    sheet.add_canvas_object(object).unwrap();
    sheet
}

#[test]
fn stencil_sheet_clipboard_requires_version_twenty_two() {
    let sheet = stencil_sheet();
    let text = clipboard::encode_sheet(&sheet).unwrap();
    assert!(text.starts_with("THINODA-SHEET-22\n"));
    assert_eq!(clipboard::decode_sheet(&text).unwrap(), sheet);
    assert!(clipboard::decode_sheet(&text.replacen("SHEET-22", "SHEET-21", 1)).is_err());
}

#[test]
fn stencil_board_clipboard_requires_version_nineteen_and_keeps_reference() {
    let sheet = stencil_sheet();
    let layout = layout_sheet(&sheet, &ApproxMeasure, false);
    let ids: Vec<_> = sheet.canvas.objects.iter().map(|o| o.id).collect();
    let payload = clipboard::copy_board_selection(&sheet, &layout, &[], &ids)
        .unwrap()
        .unwrap();
    let text = clipboard::encode_board_payload(&payload).unwrap();
    assert!(text.starts_with("THINODA-BOARD-CLIPBOARD-19\n"));
    assert!(
        clipboard::decode_board_payload(&text.replacen("CLIPBOARD-19", "CLIPBOARD-18", 1)).is_err()
    );
    let decoded = clipboard::decode_board_payload(&text).unwrap();
    let mut destination = Sheet::new("Paste", "Root");
    clipboard::paste_board_selection(&mut destination, &decoded, Vec2::new(50.0, 50.0)).unwrap();
    assert_eq!(
        destination.canvas.objects[0].stencil,
        sheet.canvas.objects[0].stencil
    );
}
