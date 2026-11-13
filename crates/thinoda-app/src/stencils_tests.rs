use super::*;
use egui::Role;
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_core::{CanvasObjectKind, templates};
use thinoda_platform::settings::FormatTab;

const RECT: &str = "<mxGraphModel><root><mxCell id=\"0\"/><mxCell id=\"1\" parent=\"0\"/><mxCell id=\"2\" value=\"A\" style=\"rounded=0;whiteSpace=wrap;html=1;\" vertex=\"1\" parent=\"1\"><mxGeometry width=\"120\" height=\"60\" as=\"geometry\"/></mxCell></root></mxGraphModel>";
const PACKED_64_KIB: &str = "7cGxDQARAADAkX4B0aoMISFREKIyvkH+7sK86ZTd86ptxAsAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAMBfhG/edMruedU24gM=";

fn library(title: &str) -> String {
    drawio_library::encode_library(&StencilLibrary {
        id: String::new(),
        title: title.into(),
        entries: vec![StencilEntry {
            title: "Team box".into(),
            xml: RECT.into(),
            width: 120.0,
            height: 60.0,
        }],
    })
}

fn rect_entry() -> StencilEntry {
    StencilEntry {
        title: String::new(),
        xml: RECT.into(),
        width: 120.0,
        height: 60.0,
    }
}

fn team_package() -> StencilPackage {
    let mut package = thinoda_core::stencil_object::find_package("thinoda.general")
        .unwrap()
        .clone();
    package.id = "test.team".into();
    package.title = "Team shapes".into();
    for definition in &mut package.definitions {
        definition.title = format!("Team {}", definition.title);
    }
    package
}

#[test]
fn json_package_import_is_atomic_and_rejects_duplicate_or_builtin_ids() {
    let mut panel = loaded_panel();
    let package = team_package();
    let json = package.to_json();
    let (id, unavailable) = panel.import_package(&json).unwrap();
    assert_eq!((id.as_str(), unavailable), ("lib-1", 0));
    assert_eq!(panel.state.packages[0].json, json);
    let before = panel.state.clone();
    let mut updated = package;
    updated.revision += 1;
    assert!(
        panel
            .import_package(&updated.to_json())
            .unwrap_err()
            .contains("already installed")
    );
    let builtin = thinoda_core::stencil_object::find_package("thinoda.general")
        .unwrap()
        .to_json();
    assert!(panel.import_package(&builtin).is_err());
    assert!(panel.import_package("{broken").is_err());
    assert_eq!(panel.state, before);
    assert_eq!(panel.packages.len(), 1);
    assert_eq!(panel.import("Team", &library("Team")).unwrap().0, "lib-2");
}

#[test]
fn installed_packages_survive_reload_and_removal_does_not_change_pinned_objects() {
    let dir = tempfile::tempdir().unwrap();
    let paths = thinoda_platform::Paths::from_root(dir.path());
    paths.ensure().unwrap();
    let mut panel = StencilPanel::default();
    assert_eq!(panel.ensure_loaded(Some(&paths)), None);
    let package = team_package();
    let (id, _) = panel.import_package(&package.to_json()).unwrap();
    panel.save(Some(&paths)).unwrap();
    let mut restored = StencilPanel::default();
    assert_eq!(restored.ensure_loaded(Some(&paths)), None);
    assert_eq!(restored.state, panel.state);
    let section = restored
        .sections()
        .into_iter()
        .find(|section| section.id == id)
        .unwrap();
    let mut destination = Sheet::new("Destination", "Root");
    for (index, entry) in section.entries() {
        let prepared = restored
            .prepared(&section, index, entry, false, &mut 1)
            .unwrap()
            .unwrap();
        paste_board_selection(
            &mut destination,
            &prepared.payload,
            Vec2::new(index as f32 * 200.0, 0.0),
        )
        .unwrap();
    }
    let expected: Vec<_> = destination
        .canvas
        .objects
        .iter()
        .map(|object| thinoda_core::stencil_object::render(object).unwrap())
        .collect();
    assert_eq!(expected[0].ports.len(), 12);
    let serialized = serde_json::to_string(&destination).unwrap();
    restored.remove_library(&id);
    assert!(restored.state.packages.is_empty());
    let mut updated = package;
    updated.revision += 1;
    updated.definitions[0].connections.clear();
    restored.import_package(&updated.to_json()).unwrap();
    let mut offline: Sheet = serde_json::from_str(&serialized).unwrap();
    offline.hydrate_stencil_packages().unwrap();
    offline.validate().unwrap();
    for (object, expected) in offline.canvas.objects.iter().zip(expected) {
        assert_eq!(
            thinoda_core::stencil_object::render(object).unwrap(),
            expected
        );
        assert_eq!(
            object
                .stencil
                .as_ref()
                .unwrap()
                .resolve()
                .unwrap()
                .0
                .revision,
            1
        );
    }
}

#[test]
fn json_package_tiles_support_tag_search_keyboard_drag_and_undo() {
    let mut app = app();
    assert_eq!(app.stencils.ensure_loaded(None), None);
    app.stencils
        .import_package(&team_package().to_json())
        .unwrap();
    let mut h = harness(app);
    search(&mut h, "envelope");
    let tile = h.get_by_role_and_label(Role::Button, "Team Message");
    tile.focus();
    h.key_press(egui::Key::Enter);
    h.run_steps(4);
    assert_eq!(object_count(h.state()), 1);
    let object = &h.state().doc().unwrap().doc.sheet().canvas.objects[0];
    let stencil = object.stencil.as_ref().unwrap();
    assert_eq!(stencil.package, "test.team");
    assert!(stencil.package_resource.is_some());
    // Measured from the published draw.io 32.7.2 message shape and the supplied BPMN screenshot.
    let render = thinoda_core::stencil_object::render(object).unwrap();
    assert_eq!(render.size, Vec2::new(60.0, 40.0));
    assert_eq!(render.paths.len(), 2);
    assert_eq!(
        render.paths[1].subpaths[0].points,
        [Vec2::ZERO, Vec2::new(30.0, 20.0), Vec2::new(60.0, 0.0)]
    );
    let ports = [
        (9.0, 2.0, false),
        (30.0, 0.0, true),
        (51.0, 2.0, false),
        (0.0, 12.0, true),
        (0.0, 20.0, true),
        (0.0, 28.0, true),
        (60.0, 12.0, true),
        (60.0, 20.0, true),
        (60.0, 28.0, true),
        (9.0, 38.0, false),
        (30.0, 40.0, true),
        (51.0, 38.0, false),
    ];
    assert_eq!(render.ports.len(), ports.len());
    for (port, (x, y, perimeter)) in render.ports.iter().zip(ports) {
        assert!((port.at - Vec2::new(x, y)).length() < 0.001);
        assert_eq!(port.perimeter, perimeter);
    }
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    assert_eq!(object_count(h.state()), 0);
    assert!(h.state_mut().doc_mut().unwrap().doc.redo());
    assert_eq!(object_count(h.state()), 1);
    let start = h.get_by_label("Team Message").rect().center();
    let target = h.state().doc().unwrap().view.canvas_rect().center();
    h.hover_at(start);
    h.drag_at(start);
    h.run_steps(1);
    h.hover_at(start + vec2(-20.0, 0.0));
    h.run_steps(2);
    h.hover_at(target);
    h.run_steps(2);
    h.event(egui::Event::PointerButton {
        pos: target,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(4);
    assert_eq!(object_count(h.state()), 2);
    assert!(
        h.state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .canvas
            .objects
            .iter()
            .all(|object| {
                object
                    .stencil
                    .as_ref()
                    .is_some_and(|stencil| stencil.package_resource.is_some())
            })
    );
    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    assert_eq!(object_count(h.state()), 1);
}

#[test]
fn unsupported_json_definitions_keep_diagnostics_and_never_insert_a_fallback() {
    let mut package = team_package();
    package.definitions[0].requires.push("vendor.future".into());
    let mut app = app();
    assert_eq!(app.stencils.ensure_loaded(None), None);
    assert_eq!(
        app.stencils.import_package(&package.to_json()).unwrap().1,
        1
    );
    let mut h = harness(app);
    search(&mut h, "Team Message");
    h.get_by_role_and_label(Role::Button, "Team Message")
        .click();
    h.run_steps(3);
    assert_eq!(object_count(h.state()), 0);
    assert!(
        h.state()
            .toasts
            .texts()
            .any(|text| text.contains("vendor.future"))
    );
}

#[test]
fn importing_json_leaves_its_source_unchanged_and_damaged_saved_packages_are_not_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("team.JSON");
    let json = team_package().to_json();
    std::fs::write(&source, &json).unwrap();
    let mut app = app();
    app.import_stencil_libraries(std::slice::from_ref(&source));
    assert_eq!(std::fs::read_to_string(source).unwrap(), json);
    assert_eq!(app.stencils.packages.len(), 1);

    let paths = thinoda_platform::Paths::from_root(dir.path());
    paths.ensure().unwrap();
    let state = StencilState {
        packages: vec![SavedPackage {
            id: "lib-1".into(),
            json: "{broken".into(),
        }],
        ..Default::default()
    };
    thinoda_platform::stencils::save(&paths, &state).unwrap();
    let before =
        std::fs::read(paths.config_dir.join(thinoda_platform::stencils::FILE_NAME)).unwrap();
    let mut panel = StencilPanel::default();
    assert!(panel.ensure_loaded(Some(&paths)).is_some());
    assert!(panel.save(Some(&paths)).is_err());
    assert_eq!(
        std::fs::read(paths.config_dir.join(thinoda_platform::stencils::FILE_NAME)).unwrap(),
        before
    );
}

fn clipped_entry(x: u32) -> StencilEntry {
    StencilEntry {
        title: "Clipped child".into(),
        xml: format!(
            r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="frame" value="Frame" style="swimlane;thinodaClip=1;" vertex="1" parent="1"><mxGeometry x="100" y="200" width="240" height="120" as="geometry"/></mxCell><mxCell id="child" value="Child" style="shape=rectangle;" vertex="1" parent="frame"><mxGeometry x="{x}" y="30" width="50" height="40" as="geometry"/></mxCell></root></mxGraphModel>"#
        ),
        width: 50.0,
        height: 40.0,
    }
}

fn packed_library(count: usize) -> String {
    let entries = vec![serde_json::json!({"xml": PACKED_64_KIB}); count];
    format!("<mxlibrary>{}</mxlibrary>", serde_json::json!(entries))
}

fn loaded_panel() -> StencilPanel {
    let mut panel = StencilPanel::default();
    assert_eq!(panel.ensure_loaded(None), None);
    panel
}

fn app() -> ThinodaApp {
    let mut app = ThinodaApp::headless();
    app.settings.ai.enabled = false;
    app.settings.show_format_panel = true;
    app.format_tab = FormatTab::Stencils;
    app.new_doc(templates::instantiate("blank-canvas"));
    app
}

fn harness(app: ThinodaApp) -> Harness<'static, ThinodaApp> {
    let mut harness = Harness::builder()
        .with_size(vec2(1200.0, 900.0))
        .with_step_dt(1.0 / 60.0)
        .with_context_setup(|ctx| {
            ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
        })
        .build_ui_state(|ui, app: &mut ThinodaApp| app.draw(ui), app);
    // Thumbnails decode over several frames.
    harness.run_steps(12);
    harness
}

fn object_count(app: &ThinodaApp) -> usize {
    app.doc().unwrap().doc.sheet().canvas.objects.len()
}

fn search(h: &mut Harness<'_, ThinodaApp>, text: &str) {
    h.get_by_label("Search stencils").click();
    h.run_steps(2);
    h.get_by_label("Search stencils").type_text(text);
    h.run_steps(12);
}

fn section_ids(panel: &StencilPanel) -> Vec<String> {
    panel.sections().into_iter().map(|s| s.id).collect()
}

#[test]
fn the_scratchpad_comes_first_followed_by_built_in_sections_and_imports() {
    let mut panel = loaded_panel();
    panel.import("Team", &library("Team")).unwrap();
    assert_eq!(
        section_ids(&panel),
        [
            SCRATCHPAD_ID,
            "general",
            "basic",
            "flowchart",
            "arrows",
            "thinoda.general",
            "thinoda.drawio",
            "lib-1"
        ]
    );
    panel.set_builtin_visible("basic", false);
    assert!(!section_ids(&panel).contains(&"basic".to_owned()));
    panel.set_builtin_visible("basic", true);
    assert!(panel.state.hidden_builtin.is_empty());
}

#[test]
fn imports_keep_the_original_xml_and_reject_unusable_libraries() {
    let mut panel = loaded_panel();
    let text = library("Team");
    let (id, skipped) = panel.import("Team", &text).unwrap();
    assert_eq!((id.as_str(), skipped), ("lib-1", 0));
    let saved = panel.state.libraries.iter().find(|s| s.id == id).unwrap();
    assert_eq!(saved.xml, text);
    assert_eq!(saved.title, "Team");

    assert!(panel.import("Empty", "<mxlibrary>[]</mxlibrary>").is_err());
    assert!(panel.import("Broken", "<html></html>").is_err());
    assert_eq!(panel.state.libraries.len(), 2);

    assert_eq!(panel.import("Team", &text).unwrap().0, "lib-2");
    panel.set_collapsed("lib-1", true);
    panel.remove_library("lib-1");
    assert!(panel.state.libraries.iter().all(|s| s.id != "lib-1"));
    assert!(panel.state.collapsed.is_empty());
    panel.remove_library(SCRATCHPAD_ID);
    assert_eq!(panel.state.libraries[0].id, SCRATCHPAD_ID);
    // Freed IDs are reused rather than growing without bound.
    assert_eq!(panel.import("Team", &text).unwrap().0, "lib-1");
}

#[test]
fn importing_a_library_keeps_the_source_file_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("team.xml");
    let contents = library("Team");
    std::fs::write(&source, &contents).unwrap();
    let mut app = app();

    app.import_stencil_libraries(std::slice::from_ref(&source));

    assert_eq!(std::fs::read_to_string(&source).unwrap(), contents);
    assert!(app.stencils.imported.iter().any(|lib| lib.title == "team"));
}

#[test]
fn the_library_count_is_bounded() {
    let mut panel = loaded_panel();
    let text = library("Team");
    while panel.state.libraries.len() < thinoda_platform::stencils::MAX_LIBRARIES {
        panel.import("Team", &text).unwrap();
    }
    assert!(panel.import("Team", &text).is_err());
}

#[test]
fn scratchpad_entries_persist_and_reload() {
    let directory = tempfile::tempdir().unwrap();
    let paths = thinoda_platform::Paths::from_root(directory.path());
    let mut panel = StencilPanel::default();
    assert_eq!(panel.ensure_loaded(Some(&paths)), None);
    panel.add_to_scratchpad(rect_entry()).unwrap();
    panel.import("Team", &library("Team")).unwrap();
    panel.set_collapsed("general", true);
    panel.save(Some(&paths)).unwrap();

    let mut reloaded = StencilPanel::default();
    assert_eq!(reloaded.ensure_loaded(Some(&paths)), None);
    assert_eq!(reloaded.scratchpad_len(), 1);
    assert_eq!(reloaded.state.collapsed, ["general"]);
    let titles: Vec<_> = reloaded.sections().into_iter().map(|s| s.title).collect();
    assert!(titles.contains(&"Team".to_owned()));

    reloaded.remove_scratchpad_entry(7);
    assert_eq!(reloaded.scratchpad_len(), 1);
    reloaded.remove_scratchpad_entry(0);
    assert_eq!(reloaded.scratchpad_len(), 0);
}

#[test]
fn a_damaged_state_file_is_reported_and_never_overwritten() {
    let directory = tempfile::tempdir().unwrap();
    let paths = thinoda_platform::Paths::from_root(directory.path());
    let file = paths.config_dir.join(thinoda_platform::stencils::FILE_NAME);
    std::fs::create_dir_all(&paths.config_dir).unwrap();
    std::fs::write(&file, b"{ not json").unwrap();

    let mut damaged = StencilPanel::default();
    assert!(damaged.ensure_loaded(Some(&paths)).is_some());
    assert_eq!(section_ids(&damaged)[0], SCRATCHPAD_ID);
    damaged.add_to_scratchpad(rect_entry()).unwrap();
    assert!(damaged.save(Some(&paths)).is_err());
    assert_eq!(std::fs::read(&file).unwrap(), b"{ not json");
}

#[test]
fn damaged_saved_libraries_are_never_overwritten() {
    let partial_scratchpad = format!(
        "<mxlibrary>{}</mxlibrary>",
        serde_json::json!([{"xml": RECT}, {"xml": "not a diagram"}])
    );
    for (id, xml) in [
        ("lib-1", "<mxlibrary>broken</mxlibrary>".to_owned()),
        (SCRATCHPAD_ID, partial_scratchpad),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let paths = thinoda_platform::Paths::from_root(directory.path());
        let state = StencilState {
            libraries: vec![SavedLibrary {
                id: id.into(),
                title: id.into(),
                xml,
            }],
            ..Default::default()
        };
        thinoda_platform::stencils::save(&paths, &state).unwrap();
        let file = paths.config_dir.join(thinoda_platform::stencils::FILE_NAME);
        let before = std::fs::read(&file).unwrap();

        let mut panel = StencilPanel::default();
        assert!(panel.ensure_loaded(Some(&paths)).is_some(), "{id}");
        panel.add_to_scratchpad(rect_entry()).unwrap();
        assert!(panel.save(Some(&paths)).is_err(), "{id}");
        assert_eq!(std::fs::read(&file).unwrap(), before, "{id}");
    }
}

#[test]
fn partially_usable_saved_imports_report_skips_and_preserve_the_source() {
    let directory = tempfile::tempdir().unwrap();
    let paths = thinoda_platform::Paths::from_root(directory.path());
    let xml = format!(
        "<mxlibrary>{}</mxlibrary>",
        serde_json::json!([{"xml": RECT}, {"xml": "not a diagram"}])
    );
    let state = StencilState {
        libraries: vec![SavedLibrary {
            id: "lib-1".into(),
            title: "Partial".into(),
            xml: xml.clone(),
        }],
        ..Default::default()
    };
    thinoda_platform::stencils::save(&paths, &state).unwrap();
    let mut panel = StencilPanel::default();
    let warning = panel.ensure_loaded(Some(&paths)).unwrap();
    assert!(warning.contains("Partial"));
    assert!(warning.contains('1'));
    assert!(panel.persist);
    panel.add_to_scratchpad(rect_entry()).unwrap();
    panel.save(Some(&paths)).unwrap();
    let saved = thinoda_platform::stencils::load(&paths).unwrap();
    assert_eq!(
        saved
            .libraries
            .iter()
            .find(|saved| saved.id == "lib-1")
            .unwrap()
            .xml,
        xml
    );
}

#[test]
fn imported_and_saved_libraries_share_the_decoded_size_budget() {
    let half = packed_library(256);
    let extra = packed_library(1);
    let mut panel = loaded_panel();
    panel.import("first", &half).unwrap();
    panel.import("second", &half).unwrap();
    assert_eq!(panel.retained_bytes(), drawio_library::MAX_DECODED_BYTES);
    let count = panel.state.libraries.len();
    assert!(panel.import("extra", &extra).is_err());
    assert!(panel.add_to_scratchpad(rect_entry()).is_err());
    assert_eq!(panel.state.libraries.len(), count);
    assert_eq!(panel.scratchpad_len(), 0);

    let directory = tempfile::tempdir().unwrap();
    let paths = thinoda_platform::Paths::from_root(directory.path());
    let state = StencilState {
        libraries: vec![
            SavedLibrary {
                id: "lib-1".into(),
                title: "first".into(),
                xml: half.clone(),
            },
            SavedLibrary {
                id: "lib-2".into(),
                title: "second".into(),
                xml: half,
            },
            SavedLibrary {
                id: "lib-3".into(),
                title: "extra".into(),
                xml: extra,
            },
        ],
        ..Default::default()
    };
    thinoda_platform::stencils::save(&paths, &state).unwrap();
    let file = paths.config_dir.join(thinoda_platform::stencils::FILE_NAME);
    let before = std::fs::read(&file).unwrap();
    let mut reloaded = StencilPanel::default();
    let error = reloaded.ensure_loaded(Some(&paths)).unwrap();
    assert!(error.contains("extra"));
    assert!(!reloaded.persist);
    assert_eq!(reloaded.retained_bytes(), drawio_library::MAX_DECODED_BYTES);
    assert!(reloaded.save(Some(&paths)).is_err());
    assert_eq!(std::fs::read(&file).unwrap(), before);
}

#[test]
fn a_batch_shares_work_even_when_a_later_import_fails() {
    let text = packed_library(1);
    let mut panel = loaded_panel();
    let mut work = text.len() * 2 + 65_536 * 2 - 1;
    panel.import_with_work("first", &text, &mut work).unwrap();
    let before = panel.state.libraries.len();
    assert!(panel.import_with_work("second", &text, &mut work).is_err());
    assert_eq!(work, 0);
    assert_eq!(panel.state.libraries.len(), before);
}

#[test]
fn scratchpad_size_failures_leave_the_previous_contents_unchanged() {
    let mut panel = loaded_panel();
    panel.add_to_scratchpad(rect_entry()).unwrap();
    let original = panel.state.libraries[0].xml.clone();
    let mut large = rect_entry();
    large.xml = format!(
        "<mxGraphModel>{}</mxGraphModel>",
        "\"".repeat(4 * 1024 * 1024)
    );
    assert!(panel.add_to_scratchpad(large).is_err());
    assert_eq!(panel.scratchpad_len(), 1);
    assert_eq!(panel.state.libraries[0].xml, original);
}

#[test]
fn imports_cannot_exceed_the_serialized_state_limit() {
    let mut entry = rect_entry();
    entry.xml = format!(
        "<mxGraphModel>{}</mxGraphModel>",
        "\"".repeat(3 * 1024 * 1024)
    );
    let text = drawio_library::encode_library(&StencilLibrary {
        id: String::new(),
        title: "quotes".into(),
        entries: vec![entry],
    });
    assert!(text.len() < drawio_library::MAX_LIBRARY_BYTES);
    let mut panel = loaded_panel();
    for _ in 0..2 {
        panel.import("quotes", &text).unwrap();
    }
    let before = panel.state.libraries.len();
    assert!(panel.import("too many", &text).is_err());
    assert_eq!(panel.state.libraries.len(), before);
    assert!(thinoda_platform::stencils::validate(&panel.state).is_ok());

    panel.add_to_scratchpad(rect_entry()).unwrap();
    panel.add_to_scratchpad(rect_entry()).unwrap();
    let scratchpad_before = Arc::clone(&panel.imported[0]);
    let xml_before = panel.state.libraries[0].xml.clone();
    let mut large = rect_entry();
    large.xml = format!("<mxGraphModel>{}</mxGraphModel>", "\"".repeat(2_500_000));
    let mut scratchpad_candidate = scratchpad_before.as_ref().clone();
    scratchpad_candidate.entries.push(large.clone());
    let candidate_xml = drawio_library::encode_library(&scratchpad_candidate);
    assert!(candidate_xml.len() < drawio_library::MAX_LIBRARY_BYTES);
    let mut candidate_state = panel.state.clone();
    candidate_state.libraries[0].xml = candidate_xml;
    assert_eq!(
        thinoda_platform::stencils::validate(&candidate_state).unwrap_err(),
        thinoda_platform::stencils::SIZE_LIMIT_ERROR
    );
    drop(candidate_state);

    assert!(panel.add_to_scratchpad(large).is_err());
    assert!(Arc::ptr_eq(&panel.imported[0], &scratchpad_before));
    assert_eq!(panel.state.libraries[0].xml, xml_before);
    panel.remove_scratchpad_entry(0);

    let directory = tempfile::tempdir().unwrap();
    let paths = thinoda_platform::Paths::from_root(directory.path());
    thinoda_platform::stencils::save(&paths, &panel.state).unwrap();
    let mut restored = StencilPanel::default();
    assert_eq!(restored.ensure_loaded(Some(&paths)), None);
    assert_eq!(restored.scratchpad_len(), 1);
    assert_eq!(
        restored.imported[0].entries[0].xml,
        scratchpad_before.entries[1].xml
    );
}

#[test]
fn sections_share_libraries_until_the_scratchpad_changes() {
    let mut panel = loaded_panel();
    let first = panel.sections().remove(0);
    let second = panel.sections().remove(0);
    assert!(Arc::ptr_eq(first.library(), second.library()));
    panel.add_to_scratchpad(rect_entry()).unwrap();
    let updated = panel.sections().remove(0);
    assert!(!Arc::ptr_eq(first.library(), updated.library()));
    assert!(first.library().entries.is_empty());
    assert_eq!(updated.library().entries.len(), 1);
}

#[test]
fn preview_cache_evicts_by_estimated_size_and_invalidates_accounting() {
    let mut panel = loaded_panel();
    panel.remember(("a".into(), 0), None, 6, 10);
    panel.remember(("b".into(), 0), None, 4, 10);
    assert_eq!((panel.cache.len(), panel.cache_bytes), (2, 10));
    panel.remember(("c".into(), 0), None, 1, 10);
    assert_eq!((panel.cache.len(), panel.cache_bytes), (1, 1));
    panel.invalidate("c");
    assert_eq!((panel.cache.len(), panel.cache_bytes), (0, 0));
    panel.remember(("large".into(), 0), None, 11, 10);
    assert_eq!((panel.cache.len(), panel.cache_bytes), (1, 11));
    panel.remember(("small".into(), 0), None, 1, 10);
    assert_eq!((panel.cache.len(), panel.cache_bytes), (1, 1));
}

#[test]
fn ellipse_thumbnail_styling_does_not_change_inserted_or_imported_content() {
    let mut panel = loaded_panel();
    let general = panel
        .sections()
        .into_iter()
        .find(|section| section.id == "general")
        .unwrap();
    let (index, double) = general
        .library()
        .entries
        .iter()
        .enumerate()
        .find(|(_, entry)| entry.title == "Double ellipse")
        .unwrap();
    let double = double.clone();
    let imported_xml = drawio_library::encode_library(&StencilLibrary {
        id: String::new(),
        title: "Imported".into(),
        entries: vec![double.clone()],
    });
    panel.import("Imported", &imported_xml).unwrap();
    panel.add_to_scratchpad(double.clone()).unwrap();

    for dark in [false, true] {
        let preview = panel
            .prepared(&general, index, SectionEntry::Drawio(&double), dark, &mut 1)
            .unwrap()
            .unwrap();
        let object = &preview.sheet.canvas.objects[0];
        assert!(matches!(
            object.kind,
            CanvasObjectKind::Shape {
                shape: ShapeKind::DoubleEllipse,
                ..
            }
        ));
        assert_eq!((object.style.fill, object.style.stroke), (None, None));

        let mut inserted = Sheet::new("Inserted", "");
        inserted.root_mode = SheetRootMode::Hidden;
        paste_board_selection(&mut inserted, &preview.payload, Vec2::ZERO).unwrap();
        let object = &inserted.canvas.objects[0];
        assert!(matches!(
            object.kind,
            CanvasObjectKind::Shape {
                shape: ShapeKind::DoubleEllipse,
                ..
            }
        ));
        assert_eq!(
            (object.style.fill, object.style.stroke),
            (
                Some(thinoda_core::Color::WHITE),
                Some(thinoda_core::Color::BLACK)
            )
        );

        for id in ["lib-1", SCRATCHPAD_ID] {
            let section = panel
                .sections()
                .into_iter()
                .find(|section| section.id == id)
                .unwrap();
            let entry = &section.library().entries[0];
            let preview = panel
                .prepared(&section, 0, SectionEntry::Drawio(entry), dark, &mut 1)
                .unwrap()
                .unwrap();
            let object = &preview.sheet.canvas.objects[0];
            assert_eq!(
                (object.style.fill, object.style.stroke),
                (
                    Some(thinoda_core::Color::WHITE),
                    Some(thinoda_core::Color::BLACK)
                ),
                "{id} in dark={dark}"
            );
        }
    }
}

#[test]
fn stencil_preview_bounds_follow_content_when_a_clipping_frame_is_omitted() {
    for x in [220, 350] {
        let preview = prepare(&clipped_entry(x), false, false).unwrap();
        let bounds = thinoda_core::scene::canvas_bounds(&preview.sheet, &preview.layout).unwrap();
        assert_eq!(preview.bounds, bounds);
        assert!(preview.simplified);
    }
}

#[test]
fn clipped_stencils_use_the_same_content_bounds_for_click_and_drag() {
    for x in [220, 350] {
        let entry = clipped_entry(x);
        let expected_bounds = prepare(&entry, false, false).unwrap().bounds;
        let text = drawio_library::encode_library(&StencilLibrary {
            id: String::new(),
            title: "Clipped examples".into(),
            entries: vec![entry],
        });
        let make_harness = || {
            let mut app = app();
            assert_eq!(app.stencils.ensure_loaded(None), None);
            app.stencils.import("Clipped examples", &text).unwrap();
            let mut h = harness(app);
            search(&mut h, "Clipped child");
            h
        };

        let mut h = make_harness();
        let center = h.state().doc().unwrap().view.visible_sheet_center();
        h.get_by_role_and_label(Role::Button, "Clipped child")
            .click();
        h.run_steps(4);
        let object = h
            .state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .canvas
            .objects
            .last()
            .unwrap();
        assert!(
            (object.rect.center() - center).length() < 0.72,
            "click at x={x}"
        );
        assert!(h.state().toasts.texts().any(|text| text.contains(t(
            "Inserted; some draw.io content may have been omitted or simplified"
        ))));

        let mut h = make_harness();
        let start = h.get_by_label("Clipped child").rect().center();
        let doc = h.state().doc().unwrap();
        let canvas = doc.view.canvas_rect();
        let target = canvas.center() + vec2(-100.0, 70.0);
        let xf = Xf {
            origin: canvas.center() + doc.view.offset,
            zoom: doc.view.zoom,
        };
        let mut expected = xf.sheet(target);
        let board = &doc.doc.sheet().board;
        if board.snap {
            expected = Vec2::new(
                (expected.x / board.grid_size).round() * board.grid_size,
                (expected.y / board.grid_size).round() * board.grid_size,
            );
        }
        h.hover_at(start);
        h.drag_at(start);
        h.run_steps(1);
        h.hover_at(start + vec2(-20.0, 0.0));
        h.run_steps(2);
        let drop = egui::DragAndDrop::payload::<thinoda_canvas::StencilDrop>(&h.ctx).unwrap();
        assert_eq!(drop.bounds, expected_bounds);
        assert!(drop.simplified);
        h.hover_at(target);
        h.run_steps(2);
        h.event(egui::Event::PointerButton {
            pos: target,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(4);
        let object = h
            .state()
            .doc()
            .unwrap()
            .doc
            .sheet()
            .canvas
            .objects
            .last()
            .unwrap();
        assert!(
            (object.rect.center() - expected).length() < 0.72,
            "drag at x={x}"
        );
        assert!(h.state().toasts.texts().any(|text| text.contains(t(
            "Inserted; some draw.io content may have been omitted or simplified"
        ))));
        assert!(h.state_mut().doc_mut().unwrap().doc.undo());
        assert_eq!(object_count(h.state()), 0);
    }
}

#[test]
fn canceling_a_simplified_stencil_drag_does_not_report_insertion() {
    let text = drawio_library::encode_library(&StencilLibrary {
        id: String::new(),
        title: "Clipped examples".into(),
        entries: vec![clipped_entry(220)],
    });
    let mut app = app();
    assert_eq!(app.stencils.ensure_loaded(None), None);
    app.stencils.import("Clipped examples", &text).unwrap();
    let mut h = harness(app);
    search(&mut h, "Clipped child");
    let start = h.get_by_label("Clipped child").rect().center();
    h.hover_at(start);
    h.drag_at(start);
    h.run_steps(1);
    h.hover_at(start + vec2(-20.0, 0.0));
    h.run_steps(2);
    assert!(egui::DragAndDrop::payload::<thinoda_canvas::StencilDrop>(&h.ctx).is_some());
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert!(egui::DragAndDrop::payload::<thinoda_canvas::StencilDrop>(&h.ctx).is_none());
    assert_eq!(object_count(h.state()), 0);
    assert!(!h.state().toasts.texts().any(|text| text.contains(t(
        "Inserted; some draw.io content may have been omitted or simplified"
    ))));
}

#[test]
fn an_unusable_stencil_is_reported_instead_of_inserted() {
    let mut entry = rect_entry();
    entry.title = "Empty stencil".into();
    entry.xml = "<mxGraphModel><root><mxCell id=\"0\"/><mxCell id=\"1\" parent=\"0\"/></root></mxGraphModel>".into();
    let text = drawio_library::encode_library(&StencilLibrary {
        id: String::new(),
        title: "Empty examples".into(),
        entries: vec![entry],
    });
    let mut app = app();
    assert_eq!(app.stencils.ensure_loaded(None), None);
    app.stencils.import("Empty examples", &text).unwrap();
    let mut h = harness(app);
    search(&mut h, "Empty stencil");
    let tile = h.get_by_label("Empty stencil");
    h.hover_at(tile.rect().center());
    h.run_steps(60);
    assert!(
        h.query_by_label("Some library entries could not be used")
            .is_some()
    );
    h.get_by_role_and_label(Role::Button, "Empty stencil")
        .click();
    h.run_steps(2);
    assert_eq!(object_count(h.state()), 0);
    assert!(
        h.state()
            .toasts
            .texts()
            .any(|text| text == "Some library entries could not be used")
    );
}

#[test]
fn ellipse_stencil_tiles_keep_transparent_theme_aware_outlines_on_hover() {
    for dark in [false, true] {
        for (title, outlines) in [("Ellipse", 1), ("Circle", 1), ("Double ellipse", 2)] {
            for hover in [false, true] {
                let mut panel = loaded_panel();
                let section = panel
                    .sections()
                    .into_iter()
                    .find(|section| section.id == "general")
                    .unwrap();
                let (index, entry) = section
                    .library()
                    .entries
                    .iter()
                    .enumerate()
                    .find(|(_, entry)| entry.title == title)
                    .unwrap();
                let prepared = panel
                    .prepared(&section, index, SectionEntry::Drawio(entry), dark, &mut 1)
                    .unwrap()
                    .unwrap();
                let bounds = prepared.bounds;
                let mut harness =
                    Harness::builder()
                        .with_size(vec2(180.0, 180.0))
                        .build_ui(move |ui| {
                            thinoda_ui::theme::install(
                                ui.ctx(),
                                if dark {
                                    thinoda_ui::theme::ThemeMode::Dark
                                } else {
                                    thinoda_ui::theme::ThemeMode::Light
                                },
                                thinoda_ui::tokens::DEFAULT_ACCENT,
                            );
                            let (_, rect) = ui.allocate_space(vec2(TILE, TILE));
                            stencil_tile(
                                ui,
                                rect,
                                "general",
                                index,
                                title,
                                Some(&prepared),
                                None,
                                dark,
                                false,
                            );
                        });
                harness.run_steps(3);
                let tile = harness.get_by_label(title).rect();
                if hover {
                    harness.event(egui::Event::PointerMoved(tile.center()));
                    harness.run_steps(3);
                }
                let sample = tile.shrink(2.0).shrink(4.0);
                let zoom = (sample.width() / bounds.size.x.max(1.0))
                    .min(sample.height() / bounds.size.y.max(1.0))
                    .min(2.0);
                let color = thinoda_canvas::automatic_board_stroke(egui::Color32::default(), dark);
                let paths: Vec<_> = harness
                    .output()
                    .shapes
                    .iter()
                    .filter_map(|clipped| match &clipped.shape {
                        egui::Shape::Path(path)
                            if path.points.len() >= 48
                                && path.points.iter().all(|point| tile.contains(*point)) =>
                        {
                            Some(path)
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    paths.len(),
                    outlines,
                    "{title} in dark={dark}, hover={hover}"
                );
                assert_eq!(paths.iter().filter(|path| path.closed).count(), 1);
                for path in paths {
                    assert_eq!(path.fill, egui::Color32::TRANSPARENT);
                    assert_eq!(path.stroke.width, (2.0 * zoom).max(0.5));
                    assert_eq!(
                        path.stroke.color,
                        egui::epaint::ColorMode::Solid(color),
                        "{title} in dark={dark}, hover={hover}"
                    );
                }
            }
        }
    }
}

#[test]
fn every_built_in_title_is_a_translated_catalog_key() {
    for library in drawio_library::builtin_libraries() {
        assert!(builtin_key(&library.title).is_some(), "{}", library.title);
        for entry in &library.entries {
            assert!(builtin_key(&entry.title).is_some(), "{}", entry.title);
        }
    }
    for key in BUILTIN_TITLES {
        assert!(
            crate::i18n::has_translations(key),
            "missing translation {key}"
        );
    }
    assert!(crate::i18n::has_translations(
        "Stencil libraries exceed the size or processing limit"
    ));
}

#[test]
fn the_stencils_tab_inserts_a_search_result_as_one_undoable_edit() {
    let mut h = harness(app());
    assert!(h.state().stencils_available());
    assert!(h.query_by_label("Scratchpad").is_some());
    assert!(h.query_by_label("Flowchart").is_some());
    search(&mut h, "cylin");
    assert!(h.query_by_label("Process").is_none());
    let before = object_count(h.state());

    // Like draw.io, a title may appear in several sections; General comes first.
    h.get_all_by_role_and_label(Role::Button, "Cylinder")
        .next()
        .unwrap()
        .click();
    h.run_steps(4);
    let app = h.state();
    assert_eq!(object_count(app), before + 1);
    let object = app
        .doc()
        .unwrap()
        .doc
        .sheet()
        .canvas
        .objects
        .last()
        .unwrap();
    assert!(matches!(
        object.kind,
        CanvasObjectKind::Shape {
            shape: ShapeKind::Cylinder,
            ..
        }
    ));

    assert!(h.state_mut().doc_mut().unwrap().doc.undo());
    h.run_steps(2);
    assert_eq!(object_count(h.state()), before);
}

#[test]
fn a_search_without_matches_says_so() {
    let mut h = harness(app());
    search(&mut h, "zzzz-no-such-stencil");
    assert!(h.query_by_label("No results").is_some());
}

#[test]
fn board_selections_round_trip_through_the_scratchpad() {
    let mut app = app();
    let doc = app.doc_mut().unwrap();
    let id = doc
        .doc
        .add_canvas_object(thinoda_core::CanvasObject::shape(
            ShapeKind::Hexagon,
            thinoda_core::Rect::from_center_size(Vec2::ZERO, Vec2::new(100.0, 80.0)),
        ))
        .unwrap();
    doc.view.select_board_object(id);
    assert!(app.add_selection_to_scratchpad());
    assert_eq!(app.stencils.scratchpad_len(), 1);

    let mut h = harness(app);
    let before = object_count(h.state());
    h.get_by_role_and_label(Role::Button, "Untitled").click();
    h.run_steps(4);
    let app = h.state();
    assert_eq!(object_count(app), before + 1);
    assert!(matches!(
        app.doc()
            .unwrap()
            .doc
            .sheet()
            .canvas
            .objects
            .last()
            .unwrap()
            .kind,
        CanvasObjectKind::Shape {
            shape: ShapeKind::Hexagon,
            ..
        }
    ));
}

#[test]
fn adding_without_a_selection_warns_and_keeps_the_scratchpad_empty() {
    let mut app = app();
    app.doc_mut().unwrap().view.board_selection.clear();
    assert!(!app.add_selection_to_scratchpad());
    assert_eq!(app.stencils.scratchpad_len(), 0);
}

#[test]
fn parametric_shape_entries_insert_as_rendered_stencil_objects() {
    let library = drawio_library::builtin_libraries()
        .iter()
        .find(|library| library.id == "thinoda.general")
        .expect("data-defined package section");
    assert_eq!(
        library.entries.len(),
        thinoda_core::stencil::builtin_packages()[0]
            .definitions
            .len()
    );
    for entry in &library.entries {
        let prepared = prepare(entry, false, true).unwrap();
        assert!(!prepared.simplified, "{}", entry.title);
        let [object] = prepared.sheet.canvas.objects.as_slice() else {
            panic!("{}: one object expected", entry.title);
        };
        let stencil = object.stencil.as_ref().expect("stencil reference");
        assert_eq!(stencil.package, "thinoda.general");
        assert!(thinoda_core::stencil_object::render(object).is_some());
    }
}
