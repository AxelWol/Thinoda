use egui::{Key, accesskit::Role};
use egui_kittest::{
    Harness,
    kittest::{By, NodeT, Queryable},
};
use thinoda_app::{
    app::{Cmd, Services, ThinodaApp},
    i18n::t,
};
use thinoda_core::{Sheet, Workbook};
use thinoda_io::{Format, export_bytes};
use thinoda_platform::settings::{Language, Settings};

const WARNING: &str = "This format exports formatted notes as Markdown source, not rich text. The complete source is retained, but formatting will not be displayed. The source workbook is unchanged. Save as Thinoda to preserve note formats.";

fn book() -> Workbook {
    let mut sheet = Sheet::new("Notes", "Research");
    sheet
        .set_markdown_notes(sheet.root, Some("**café**\n\n3. First\tTab".into()))
        .unwrap();
    Workbook::new(sheet)
}

fn click(h: &mut Harness<ThinodaApp>, label: &str) {
    h.query_all(By::new().role(Role::Button).label(label))
        .find(|node| !node.accesskit_node().is_disabled())
        .unwrap()
        .click();
    h.run_steps(3);
}

#[test]
fn note_warning_is_localized_cancelable_and_blocks_edits_in_every_language() {
    for language in Language::ALL {
        let mut app = ThinodaApp::new(
            Settings {
                language,
                ..Default::default()
            },
            Services::default(),
            &[],
        );
        let book = book();
        app.new_doc(book.clone());
        let revision = app.doc().unwrap().doc.state_id();
        let mut h = Harness::builder()
            .with_size(egui::vec2(1280.0, 1000.0))
            .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        for format in [Format::Csv, Format::Opml] {
            let ctx = h.ctx.clone();
            h.state_mut().exec(&ctx, Cmd::Export(format));
            h.run_steps(3);
            assert_eq!(h.state().dialogs.note_export, Some((format, false)));
            assert!(h.query_by_label(t(WARNING)).is_some(), "{language:?}");
            h.key_press(Key::Insert);
            h.run_steps(3);
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &book);
            assert_eq!(h.state().doc().unwrap().doc.state_id(), revision);
            click(&mut h, t("Cancel"));
            assert!(h.state().dialogs.note_export.is_none());
            assert_eq!(h.state().doc().unwrap().doc.workbook(), &book);
        }
    }
}

#[test]
fn stale_note_confirmation_is_discarded_after_revision_sheet_or_document_changes() {
    for change in 0..3 {
        let mut app = ThinodaApp::headless();
        app.new_doc(book());
        let ctx = egui::Context::default();
        app.exec(&ctx, Cmd::Export(Format::Csv));
        assert!(app.dialogs.note_export.is_some());
        match change {
            0 => app
                .doc_mut()
                .unwrap()
                .doc
                .edit_sheet("Change", |sheet| sheet.set_title(sheet.root, "Changed"))
                .unwrap(),
            1 => {
                app.doc_mut()
                    .unwrap()
                    .doc
                    .edit_book("Sheet", |book| {
                        book.sheets.push(Sheet::new("Other", "Other"));
                        book.active_sheet = 1;
                        Ok(())
                    })
                    .unwrap();
            }
            _ => app.new_doc(Workbook::default()),
        }
        let before = app.doc().unwrap().doc.workbook().clone();
        let mut h = Harness::builder().build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
        h.run_steps(3);
        assert!(h.state().dialogs.note_export.is_none());
        assert_eq!(h.state().doc().unwrap().doc.workbook(), &before);
    }
}

#[test]
fn symbol_and_note_confirmations_are_independent() {
    let mut book = book();
    let root = book.active().root;
    let topic = book.active_mut().topic_mut(root).unwrap();
    topic.markers.push("priority-1".into());
    topic.marker_colors.insert(0, thinoda_core::Color::BLACK);
    let app = ThinodaApp::headless();
    let mut h = Harness::builder()
        .with_context_setup(|ctx| {
            ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu)
        })
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    h.state_mut().new_doc(book.clone());
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    let ctx = h.ctx.clone();
    h.state_mut().exec(&ctx, Cmd::Export(Format::Opml));
    h.run_steps(3);
    assert!(h.state().dialogs.symbol_export.is_some());
    assert!(h.state().dialogs.note_export.is_none());
    click(&mut h, t("Export"));
    assert!(h.state().dialogs.symbol_export.is_none());
    assert_eq!(h.state().dialogs.note_export, Some((Format::Opml, true)));
    click(&mut h, t("Cancel"));
    assert_eq!(h.state().doc().unwrap().doc.workbook(), &book);
}

#[test]
fn cli_requires_note_specific_flag_and_preserves_existing_destination_on_refusal() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("notes.thinoda");
    let bytes = export_bytes(&book(), Format::Thinoda, &Default::default()).unwrap();
    std::fs::write(&input, &bytes).unwrap();
    for extension in ["csv", "opml"] {
        let output = directory.path().join(format!("copy.{extension}"));
        for flags in [
            vec![],
            vec!["--allow-lossy"],
            vec!["--allow-symbol-color-loss"],
            vec!["--allow-note-format-loss"],
        ] {
            std::fs::write(&output, b"existing destination").unwrap();
            let result = std::process::Command::new(env!("CARGO_BIN_EXE_thinoda"))
                .arg("--convert")
                .arg(&input)
                .arg(&output)
                .args(&flags)
                .output()
                .unwrap();
            let accepted = flags.contains(&"--allow-note-format-loss");
            assert_eq!(
                result.status.success(),
                accepted,
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            if !accepted {
                assert!(
                    String::from_utf8_lossy(&result.stderr).contains("--allow-note-format-loss")
                );
                assert_eq!(std::fs::read(&output).unwrap(), b"existing destination");
            }
            assert_eq!(std::fs::read(&input).unwrap(), bytes);
        }
    }
}

#[test]
fn cli_combines_all_consents_and_rejects_unknown_flags() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("colored.thinoda");
    let output = directory.path().join("colored.opml");
    let mut book = book();
    let root = book.active().root;
    let topic = book.active_mut().topic_mut(root).unwrap();
    topic.markers.push("priority-1".into());
    topic.marker_colors.insert(0, thinoda_core::Color::BLACK);
    std::fs::write(
        &input,
        export_bytes(&book, Format::Thinoda, &Default::default()).unwrap(),
    )
    .unwrap();
    for (flags, accepted) in [
        (vec!["--allow-note-format-loss"], false),
        (vec!["--allow-symbol-color-loss"], false),
        (vec!["--allow-lossy", "--allow-symbol-color-loss"], false),
        (
            vec!["--allow-symbol-color-loss", "--allow-note-format-loss"],
            true,
        ),
        (
            vec![
                "--allow-lossy",
                "--allow-symbol-color-loss",
                "--allow-note-format-loss",
            ],
            true,
        ),
        (
            vec![
                "--allow-note-format-loss",
                "--allow-symbol-color-loss",
                "--allow-lossy",
            ],
            true,
        ),
        (vec!["--allow-note-format-loss", "--unknown"], false),
    ] {
        std::fs::write(&output, b"existing").unwrap();
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_thinoda"))
            .arg("--convert")
            .arg(&input)
            .arg(&output)
            .args(flags)
            .output()
            .unwrap();
        assert_eq!(
            result.status.success(),
            accepted,
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        if !accepted {
            assert_eq!(std::fs::read(&output).unwrap(), b"existing");
        }
        let help = std::process::Command::new(env!("CARGO_BIN_EXE_thinoda"))
            .arg("--help")
            .output()
            .unwrap();
        assert!(help.status.success());
        assert!(String::from_utf8_lossy(&help.stdout).contains("--allow-note-format-loss"));
    }
}
