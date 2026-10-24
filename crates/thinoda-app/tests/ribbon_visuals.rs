#![cfg(all(feature = "gpu-tests", any(target_os = "windows", target_os = "macos")))]

use egui::{accesskit::Role, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use thinoda_app::{ThinodaApp, app::CenterMode};
use thinoda_platform::settings::ThemeMode;

#[test]
fn ribbon_and_outline_render_at_all_detail_levels() {
    let output = std::env::var_os("THINODA_RIBBON_VISUAL_DIR").map(std::path::PathBuf::from);
    if let Some(directory) = &output {
        std::fs::create_dir_all(directory).unwrap();
    }
    for (name, width, theme, outline) in [
        ("ribbon-wide-light", 1600., ThemeMode::Light, false),
        ("ribbon-medium-dark", 720., ThemeMode::Dark, false),
        ("ribbon-narrow-dark", 320., ThemeMode::Dark, false),
        ("outline-wide-light", 1200., ThemeMode::Light, true),
        ("outline-medium-dark", 720., ThemeMode::Dark, true),
        ("outline-narrow-dark", 480., ThemeMode::Dark, true),
    ] {
        let mut app = ThinodaApp::headless();
        app.settings.theme = theme;
        app.settings.show_format_panel = false;
        app.settings.ribbon_collapsed = false;
        if outline {
            app.mode = CenterMode::Outliner;
        }
        let mut h = Harness::builder()
            .with_size(vec2(width, 620.))
            .with_context_setup(move |ctx| {
                thinoda_ui::theme::install(
                    ctx,
                    if theme == ThemeMode::Light {
                        thinoda_ui::theme::ThemeMode::Light
                    } else {
                        thinoda_ui::theme::ThemeMode::Dark
                    },
                    thinoda_ui::tokens::DEFAULT_ACCENT,
                );
            })
            .build_ui_state(
                move |ui, app: &mut ThinodaApp| {
                    thinoda_ui::theme::install(
                        ui.ctx(),
                        if theme == ThemeMode::Light {
                            thinoda_ui::theme::ThemeMode::Light
                        } else {
                            thinoda_ui::theme::ThemeMode::Dark
                        },
                        thinoda_ui::tokens::DEFAULT_ACCENT,
                    );
                    if outline {
                        app.draw(ui);
                    } else {
                        app.show_ribbon(ui);
                        egui::CentralPanel::default().show(ui, |ui| {
                            ui.label("Map workspace");
                        });
                    }
                },
                app,
            );
        h.run_steps(4);
        if !outline {
            h.get_by_role_and_label(Role::Button, "Edit").click();
            h.run_steps(4);
        }
        let captions: &[&str] = if outline {
            &["Insert", "Edit", "Arrange", "Delete"]
        } else if width > 1000.0 {
            &["Undo", "Copy", "Select", "Arrange", "Copy Style", "Edit"]
        } else {
            &[]
        };
        let mut baseline: Option<f32> = None;
        for caption in captions {
            let y = h
                .get_by_role_and_label(Role::Label, caption)
                .rect()
                .center()
                .y;
            if let Some(baseline) = baseline {
                assert!(
                    (y - baseline).abs() < 1.0,
                    "{name}: misaligned group footer {caption}"
                );
            }
            baseline = Some(y);
        }
        assert_eq!(
            h.ctx.theme(),
            if theme == ThemeMode::Light {
                egui::Theme::Light
            } else {
                egui::Theme::Dark
            }
        );
        let image = h.render().expect("GPU ribbon rendering failed");
        if !outline {
            let red = image.get_pixel(12, 55).0[0];
            assert!(
                if theme == ThemeMode::Light {
                    red > 180
                } else {
                    red < 80
                },
                "{name}: incorrect ribbon surface color"
            );
        }
        assert!(
            image.width() >= width as u32 && image.height() >= 620,
            "{name}"
        );
        if let Some(directory) = &output {
            image.save(directory.join(format!("{name}.png"))).unwrap();
        }
    }
}
