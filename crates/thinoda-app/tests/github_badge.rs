//! The bottom-right "Thinoda on GitHub" mark opens the project page in every
//! language, theme, scale and layout. Separate process because the UI language
//! is process-wide.

use egui::{Key, accesskit::Role};
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{
    app::{Cmd, Services, ThinodaApp},
    i18n::t,
};
use thinoda_core::Workbook;
use thinoda_platform::settings::{Language, Settings, ThemeMode};

const PROJECT: &str = "https://github.com/AxelWol/Thinoda";
const LABEL: &str = "Thinoda on GitHub";
/// Window frame inset plus the status bar's inner margin, in points.
const EDGE_SLACK: f32 = 18.0;

/// Mirrors `eframe::App::ui`: draw, then hand the frame's `OpenUrl` commands to the opener.
struct Shell {
    app: ThinodaApp,
    opened: Vec<String>,
}

fn harness(
    size: egui::Vec2,
    scale: f32,
    language: Language,
    theme: ThemeMode,
) -> Harness<'static, Shell> {
    let mut app = ThinodaApp::new(
        Settings {
            language,
            theme,
            ui_scale: scale,
            show_format_panel: false,
            show_topic_toolbar: false,
            show_topic_add_buttons: false,
            ..Default::default()
        },
        Services::default(),
        &[],
    );
    let mut book = Workbook::default();
    for _ in 1..12 {
        book.sheets.push(Workbook::default().sheets.remove(0));
    }
    for (index, sheet) in book.sheets.iter_mut().enumerate() {
        sheet.title = format!("Sheet {index}: a long sheet name");
    }
    app.new_doc(book);
    let mut h = Harness::builder().with_size(size * scale).build_ui_state(
        |ui, s: &mut Shell| {
            s.app.draw(ui);
            s.opened.extend(ThinodaApp::take_open_urls(ui.ctx()));
        },
        Shell {
            app,
            opened: Vec::new(),
        },
    );
    h.ctx
        .set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
    h.run_steps(4);
    h.set_size(size);
    h.run_steps(6);
    h
}

fn badge(h: &Harness<'_, Shell>) -> egui::Rect {
    let rect = h.get_by_role_and_label(Role::Button, t(LABEL)).rect();
    let content = h.ctx.content_rect();
    let tolerance = 1.0 / h.ctx.pixels_per_point();
    assert!(
        content.expand(tolerance).contains_rect(rect),
        "badge {rect:?} outside {content:?}"
    );
    assert!(rect.width() >= 16.0 && rect.height() >= 16.0, "{rect:?}");
    rect
}

fn rect(h: &Harness<'_, Shell>, label: &str) -> egui::Rect {
    h.get_by_role_and_label(Role::Button, label).rect()
}

fn assert_left_of(left: egui::Rect, right: egui::Rect, what: &str) {
    assert!(
        left.right() <= right.left() + 0.5,
        "{what}: {left:?} overlaps badge {right:?}"
    );
}

/// No visible (clip-intersected) text may be painted over the badge.
fn assert_no_text_over(h: &Harness<'_, Shell>, mark: egui::Rect, what: &str) {
    fn texts<'a>(shape: &'a egui::Shape, result: &mut Vec<&'a egui::epaint::TextShape>) {
        match shape {
            egui::Shape::Text(text) => result.push(text),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| texts(shape, result)),
            _ => {}
        }
    }
    let inner = mark.shrink(1.0 / h.ctx.pixels_per_point());
    for clipped in &h.output().shapes {
        let mut painted = Vec::new();
        texts(&clipped.shape, &mut painted);
        for text in painted {
            let visible = egui::Rect::from_min_size(text.pos, text.galley.size())
                .intersect(clipped.clip_rect);
            assert!(
                !visible.is_positive() || !visible.intersects(inner),
                "{what}: {:?} painted at {visible:?} over badge {mark:?}",
                text.galley.text()
            );
        }
    }
}

fn assert_bottom_right_edge(h: &Harness<'_, Shell>, rect: egui::Rect, slack: f32) {
    let content = h.ctx.content_rect();
    assert!(
        content.right() - rect.right() <= slack && content.bottom() - rect.bottom() <= slack,
        "badge {rect:?} is not at the lower-right edge of {content:?}"
    );
}

fn click_opens_project(h: &mut Harness<'_, Shell>, ctx: &str) {
    let before = h.state().opened.len();
    h.get_by_role_and_label(Role::Button, t(LABEL)).click();
    h.run_steps(2);
    assert_eq!(h.state().opened[before..], [PROJECT], "{ctx}");
    assert!(
        !h.output()
            .platform_output
            .commands
            .iter()
            .any(|c| matches!(c, egui::OutputCommand::OpenUrl(_))),
        "{ctx}: opened links are consumed, not left for eframe to drop"
    );
}

#[test]
fn github_badge_sits_bottom_right_and_opens_project_in_all_layouts() {
    assert!(thinoda_platform::system::is_openable_url(PROJECT));
    for language in Language::ALL {
        assert!(!t(LABEL).trim().is_empty(), "{language:?}");
        for theme in [ThemeMode::Light, ThemeMode::Dark] {
            for scale in [1.0, 2.0] {
                for size in [
                    egui::vec2(320.0, 240.0),
                    egui::vec2(640.0, 480.0),
                    egui::vec2(1000.0, 700.0),
                ] {
                    let ctx = format!("{language:?}/{theme:?}/{scale}/{size:?}");
                    let mut h = harness(size, scale, language, theme);
                    if size.x < thinoda_app::brand::GITHUB_BADGE_MIN_BAR_WIDTH {
                        assert!(
                            h.query_by_role_and_label(Role::Button, t(LABEL)).is_none(),
                            "{ctx}: narrow status bars keep their room for sheets and controls"
                        );
                        continue;
                    }
                    let state = h.state().app.doc().unwrap().doc.state_id();
                    let mark = badge(&h);
                    assert_bottom_right_edge(&h, mark, EDGE_SLACK);
                    for label in ["Statistics", "Zoom", "New Sheet"] {
                        h.get_by_role_and_label(Role::Button, t(label)).focus();
                        h.run_steps(6);
                        assert_eq!(badge(&h), mark, "{ctx}: focusing {label} moved the badge");
                        assert_no_text_over(&h, mark, &format!("{ctx}: {label}"));
                    }
                    assert_left_of(rect(&h, t("New Sheet")), mark, &ctx);

                    let app_ctx = h.ctx.clone();
                    h.state_mut().app.exec(&app_ctx, Cmd::ToggleStatusBar);
                    h.run_steps(6);
                    assert!(h.query_by_role_and_label(Role::Button, t("Zoom")).is_none());
                    let mark = badge(&h);
                    assert_bottom_right_edge(&h, mark, EDGE_SLACK);
                    assert_left_of(rect(&h, t("New Sheet")), mark, &ctx);
                    assert_no_text_over(&h, mark, &format!("{ctx}: no status bar"));
                    click_opens_project(&mut h, &ctx);
                    assert_eq!(
                        h.state().app.doc().unwrap().doc.state_id(),
                        state,
                        "{ctx}: the link must not edit the document"
                    );
                }
            }
        }

        // Home screen: same affordance, clear of the template actions.
        let mut h = harness(egui::vec2(640.0, 480.0), 1.0, language, ThemeMode::Light);
        let app_ctx = h.ctx.clone();
        h.state_mut().app.exec(&app_ctx, Cmd::Home);
        h.run_steps(6);
        let mark = badge(&h);
        let content = h.ctx.content_rect();
        assert!(
            content.right() - mark.right() <= 48.0,
            "{language:?}: {mark:?}"
        );
        assert!(mark.bottom() > content.center().y, "{language:?}: {mark:?}");
        let create = rect(&h, t("Create"));
        assert_left_of(create, mark, &format!("{language:?}: Create"));
        assert!(
            create.y_range().intersects(mark.y_range()),
            "{language:?}: badge {mark:?} not in the Create row {create:?}"
        );
        assert_no_text_over(&h, mark, &format!("{language:?}: home"));
        click_opens_project(&mut h, &format!("{language:?}/home"));
    }

    // Keyboard activation and a translated tooltip naming the destination.
    let mut h = harness(
        egui::vec2(1000.0, 700.0),
        1.0,
        Language::German,
        ThemeMode::Dark,
    );
    assert_eq!(t(LABEL), "Thinoda auf GitHub");
    h.get_by_role_and_label(Role::Button, t(LABEL)).focus();
    h.run_steps(2);
    assert!(h.get_by_role_and_label(Role::Button, t(LABEL)).is_focused());
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert_eq!(h.state().opened, [PROJECT]);
    h.get_by_role_and_label(Role::Button, t(LABEL)).hover();
    for _ in 0..60 {
        h.step();
    }
    let tooltip = format!("{}\n{PROJECT}", t(LABEL));
    assert!(
        h.query_by_label(&tooltip).is_some(),
        "tooltip {tooltip:?} not shown"
    );
}
