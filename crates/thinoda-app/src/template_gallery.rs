//! Built-in template previews, generated from the actual documents that Create opens.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use egui::{Color32, Painter, Pos2, Stroke, StrokeKind, Ui, vec2};
use thinoda_canvas::{PaintOptions, Xf, color32};
use thinoda_core::{
    CanvasObjectKind, GridKind, Layout, Sheet, SheetRootMode,
    board::{Band, shape_geometry},
    connector_path,
    layout::{ApproxMeasure, layout_sheet},
    templates::{self, TemplateCategory, TemplateInfo},
};

use crate::i18n::{Lang, t, t_in};

pub(crate) enum GalleryAction {
    Navigate(TemplateCategory),
    Create(&'static str),
}

type Sample = (Sheet, Layout);
type SampleKey = (bool, u8, &'static str);

#[derive(Default)]
struct SampleCache {
    entries: HashMap<SampleKey, Arc<Sample>>,
}

impl SampleCache {
    fn get(&mut self, dark: bool, language: Lang, id: &'static str) -> Arc<Sample> {
        Arc::clone(
            self.entries
                .entry((dark, language as u8, id))
                .or_insert_with(|| {
                    let sheet =
                        templates::instantiate_localized(id, &|s| t_in(language, s).to_owned())
                            .sheets
                            .remove(0);
                    let layout = layout_sheet(&sheet, &ApproxMeasure, dark);
                    Arc::new((sheet, layout))
                }),
        )
    }
}

#[cfg(test)]
thread_local! {
    static SAMPLE_REQUESTS: std::cell::RefCell<Vec<&'static str>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Cache each visible sample by template, appearance and UI language.
fn sample(dark: bool, language: Lang, id: &'static str) -> Arc<Sample> {
    #[cfg(test)]
    SAMPLE_REQUESTS.with(|requests| requests.borrow_mut().push(id));
    static CACHE: OnceLock<Mutex<SampleCache>> = OnceLock::new();
    CACHE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(dark, language, id)
}

fn is_board(sheet: &Sheet) -> bool {
    sheet.root_mode == SheetRootMode::Hidden
        || !sheet.canvas.objects.is_empty()
        || sheet.zones.iter().any(|zone| zone.color.is_some())
}

/// Thumbnail of a board: tinted frames, shapes and connectors around the topic layer.
fn paint_board_thumbnail(
    painter: &Painter,
    sheet: &Sheet,
    layout: &Layout,
    rect: egui::Rect,
    dark: bool,
) {
    let background =
        thinoda_core::theme::Theme::for_settings(&sheet.theme).background(&sheet.theme, dark);
    painter.rect_filled(rect, 4, color32(background));
    let rect = rect.shrink(5.0);
    let mut bounds = layout.bounds;
    if bounds.size.x <= 0.0 || bounds.size.y <= 0.0 {
        bounds = thinoda_core::Rect {
            min: thinoda_core::Vec2::new(-200.0, -150.0),
            size: thinoda_core::Vec2::new(400.0, 300.0),
        };
    }
    let zoom = (rect.width() / bounds.size.x).min(rect.height() / bounds.size.y);
    let center = bounds.center();
    let xf = Xf {
        origin: rect.center() - vec2(center.x, center.y) * zoom,
        zoom,
    };
    let painter = painter.with_clip_rect(rect);
    let ink = if dark {
        Color32::from_gray(170)
    } else {
        Color32::from_gray(85)
    };
    if sheet.board.grid != GridKind::None {
        let step = (sheet.board.grid_size * 2.0 * zoom).max(6.0);
        let dot = ink.gamma_multiply(0.35);
        let mut y = rect.top() + step / 2.0;
        while y < rect.bottom() {
            let mut x = rect.left() + step / 2.0;
            while x < rect.right() {
                painter.circle_filled(Pos2::new(x, y), 0.6, dot);
                x += step;
            }
            y += step;
        }
    }
    for zone in &sheet.zones {
        let r = xf.rect(zone.frame);
        let fill = zone.color.map_or(ink.gamma_multiply(0.04), color32);
        painter.rect(r, 2.0, fill, Stroke::new(0.5, ink), StrokeKind::Inside);
        painter.with_clip_rect(r.shrink(1.0)).text(
            r.left_top() + vec2(3.0, 2.0),
            egui::Align2::LEFT_TOP,
            &zone.name,
            egui::FontId::proportional((14.0 * zoom).clamp(5.0, 9.0)),
            ink,
        );
    }
    let objects = |band: Band| {
        for object in sheet.canvas.objects.iter().filter(|o| o.band == band) {
            match &object.kind {
                CanvasObjectKind::Shape { shape, .. } => {
                    let outline = shape_geometry(*shape, object.rect, object.rotation).outline;
                    let points = outline.into_iter().map(|p| xf.pos(p)).collect();
                    let stroke = object.style.stroke.map_or(ink, color32);
                    let fill = object.style.fill.map_or(Color32::TRANSPARENT, color32);
                    painter.add(egui::Shape::convex_polygon(
                        points,
                        fill,
                        Stroke::new(0.8, stroke),
                    ));
                }
                CanvasObjectKind::Connector(connector) => {
                    let points: Vec<_> = connector_path(sheet, layout, connector)
                        .into_iter()
                        .map(|p| xf.pos(p))
                        .collect();
                    let color = connector.color.map_or(ink, color32);
                    painter.add(egui::Shape::line(points, Stroke::new(0.8, color)));
                }
                CanvasObjectKind::Ink(stroke) => {
                    let points = stroke.points.iter().map(|p| xf.pos(*p)).collect();
                    painter.add(egui::Shape::line(
                        points,
                        Stroke::new((stroke.width * zoom).max(0.5), color32(stroke.color)),
                    ));
                }
            }
        }
    };
    objects(Band::Below);
    thinoda_canvas::paint_map(
        &painter,
        sheet,
        layout,
        xf,
        &PaintOptions {
            dark,
            omit_zones: true,
            ..Default::default()
        },
    );
    objects(Band::Above);
}

fn cards(
    ui: &mut Ui,
    entries: &[&TemplateInfo],
    choice: &mut Option<&'static str>,
    recent: bool,
) -> Option<&'static str> {
    let mut create = None;
    let width = ui.available_width();
    let columns = ((width + 8.0) / 172.0).floor().max(1.0) as usize;
    let card_width = (width - 8.0 * (columns - 1) as f32) / columns as f32;
    egui::Grid::new(("template_grid", recent))
        .num_columns(columns)
        .spacing(egui::vec2(8.0, 8.0))
        .show(ui, |ui| {
            for (index, template) in entries.iter().enumerate() {
                ui.push_id(template.id, |ui| {
                    let dark = ui.visuals().dark_mode;
                    let language = crate::i18n::lang();
                    let (response, activated) = thinoda_ui::widgets::preview_card(
                        ui,
                        t_in(language, template.name),
                        *choice == Some(template.id),
                        card_width,
                        94.0,
                        |ui, rect| {
                            let sample = sample(dark, language, template.id);
                            let (sheet, layout) = sample.as_ref();
                            if is_board(sheet) {
                                paint_board_thumbnail(ui.painter(), sheet, layout, rect, dark)
                            } else {
                                thinoda_canvas::paint_thumbnail(
                                    ui.painter(),
                                    sheet,
                                    layout,
                                    rect,
                                    dark,
                                )
                            }
                        },
                    );
                    if recent {
                        response.clone().accessible_name(format!(
                            "{}: {}",
                            t("Recent template"),
                            t(template.name)
                        ));
                    }
                    if activated {
                        *choice = Some(template.id);
                    }
                    if activated && response.double_clicked() {
                        create = Some(template.id);
                    }
                });
                if (index + 1) % columns == 0 {
                    ui.end_row();
                }
            }
        });
    create
}

pub(crate) fn show(
    ui: &mut Ui,
    category: Option<TemplateCategory>,
    choice: &mut Option<&'static str>,
    recent: &[String],
) -> Option<GalleryAction> {
    let mut action = None;
    let recent: Vec<_> = recent
        .iter()
        .filter_map(|id| templates::TEMPLATES.iter().find(|info| info.id == id))
        .filter(|info| category.is_none_or(|category| category == info.category))
        .take(8)
        .collect();
    if !recent.is_empty() {
        ui.heading(t("Recently used templates"));
        if let Some(id) = ui
            .push_id("recent_templates", |ui| cards(ui, &recent, choice, true))
            .inner
        {
            action = Some(GalleryAction::Create(id));
        }
        ui.add_space(16.0);
    }
    for group in TemplateCategory::ALL {
        if category.is_some_and(|category| category != group) {
            continue;
        }
        let entries: Vec<_> = templates::TEMPLATES
            .iter()
            .filter(|info| info.category == group)
            .collect();
        ui.horizontal(|ui| {
            ui.heading(t(group.label()));
            if category.is_none() {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = format!("{}: {}", t("Browse templates"), t(group.label()));
                    if ui
                        .small_button(thinoda_ui::icons::CHEVRON_RIGHT)
                        .on_hover_text(&label)
                        .accessible_name(label)
                        .clicked()
                    {
                        action = Some(GalleryAction::Navigate(group));
                    }
                    ui.label(entries.len().to_string());
                });
            }
        });
        if let Some(id) = ui
            .push_id(group.label(), |ui| cards(ui, &entries, choice, false))
            .inner
        {
            action = Some(GalleryAction::Create(id));
        }
        ui.add_space(16.0);
    }
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_match_every_template_and_are_cached_per_language_and_appearance() {
        let mut cache = SampleCache::default();
        for language in Lang::ALL {
            for dark in [false, true] {
                for info in templates::TEMPLATES {
                    let preview = cache.get(dark, language, info.id);
                    let (sheet, layout) = preview.as_ref();
                    assert_eq!(
                        sheet,
                        templates::instantiate_localized(info.id, &|s| {
                            t_in(language, s).to_owned()
                        })
                        .active()
                    );
                    let board = info.category == TemplateCategory::Boards;
                    assert_eq!(is_board(sheet), board, "{}", info.id);
                    let hidden = usize::from(sheet.root_mode == SheetRootMode::Hidden);
                    assert_eq!(layout.topics.len(), sheet.topics.len() - hidden);
                    if info.id != "blank-canvas" {
                        assert!(layout.bounds.size.x.is_finite() && layout.bounds.size.x > 0.0);
                        assert!(layout.bounds.size.y.is_finite() && layout.bounds.size.y > 0.0);
                    }
                    assert!(Arc::ptr_eq(&preview, &cache.get(dark, language, info.id)));
                }
            }
        }
        assert_eq!(
            cache.entries.len(),
            templates::TEMPLATES.len() * Lang::ALL.len() * 2
        );
        assert!(!Arc::ptr_eq(
            &cache.get(false, Lang::En, "meeting-board"),
            &cache.get(true, Lang::En, "meeting-board")
        ));
        assert!(!Arc::ptr_eq(
            &cache.get(false, Lang::En, "meeting-board"),
            &cache.get(false, Lang::De, "meeting-board")
        ));
    }

    #[test]
    fn only_visible_previews_are_resolved_and_scrolling_builds_new_ones() {
        let ctx = egui::Context::default();
        let mut choice = None;
        let mut render = |category, offset, recent: &[String]| {
            SAMPLE_REQUESTS.with(|requests| requests.borrow_mut().clear());
            let mut max_offset = 0.0_f32;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(480.0, 360.0))),
                    ..Default::default()
                },
                |ui| {
                    let scroll = egui::ScrollArea::vertical()
                        .id_salt("gallery_visibility_test")
                        .max_height(280.0)
                        .vertical_scroll_offset(offset)
                        .show(ui, |ui| {
                            show(ui, category, &mut choice, recent);
                        });
                    max_offset = (scroll.content_size.y - scroll.inner_rect.height()).max(0.0);
                },
            );
            output.textures_delta.clear();
            (
                SAMPLE_REQUESTS.with(|requests| requests.borrow().clone()),
                max_offset,
            )
        };
        let (initial, _) = render(None, 0.0, &[]);
        assert!(!initial.is_empty());
        assert!(initial.len() < templates::TEMPLATES.len());
        assert!(!initial.contains(&"meeting-board"));
        let mut unique = initial.clone();
        unique.sort_unstable();
        unique.dedup();
        eprintln!(
            "Initial gallery requested {} of {} templates",
            unique.len(),
            templates::TEMPLATES.len()
        );

        let recent = ["meeting-board".to_owned()];
        let (with_recent, _) = render(None, 0.0, &recent);
        assert!(with_recent.contains(&"meeting-board"));

        let (boards, max_offset) = render(Some(TemplateCategory::Boards), 0.0, &[]);
        assert!(boards.contains(&"blank-canvas"));
        assert!(!boards.contains(&"meeting-board"));
        let (bottom, _) = render(Some(TemplateCategory::Boards), max_offset, &[]);
        assert!(bottom.contains(&"meeting-board"), "{bottom:?}");
    }

    #[test]
    fn board_thumbnails_paint_frames_shapes_and_connectors() {
        let ctx = egui::Context::default();
        let rect = egui::Rect::from_min_size(Pos2::ZERO, vec2(160.0, 94.0));
        let mut counts = HashMap::new();
        let mut cache = SampleCache::default();
        for info in templates::TEMPLATES {
            if info.category != TemplateCategory::Boards {
                continue;
            }
            let sample = cache.get(false, Lang::En, info.id);
            let (sheet, layout) = sample.as_ref();
            let mut output = ctx.run_ui(Default::default(), |ui| {
                paint_board_thumbnail(ui.painter(), sheet, layout, rect, false);
            });
            output.textures_delta.clear();
            counts.insert(info.id, output.shapes.len());
        }
        // Frames, shapes and connectors add primitives beyond the background and grid.
        for id in [
            "flowchart",
            "kanban-board",
            "business-model-canvas",
            "swot-board",
            "affinity-diagram",
            "eisenhower-matrix",
            "meeting-board",
        ] {
            assert!(counts[id] > counts["blank-canvas"], "{id}: {counts:?}");
        }
    }

    #[test]
    fn new_frame_board_thumbnails_fit_every_frame() {
        let rect = egui::Rect::from_min_size(Pos2::ZERO, vec2(160.0, 94.0));
        let visible = rect.shrink(5.0).expand(0.01);
        for id in ["affinity-diagram", "eisenhower-matrix", "meeting-board"] {
            let wb = templates::instantiate(id);
            let sheet = wb.active();
            for dark in [false, true] {
                let layout = layout_sheet(sheet, &ApproxMeasure, dark);
                let ctx = egui::Context::default();
                let mut output = ctx.run_ui(Default::default(), |ui| {
                    paint_board_thumbnail(ui.painter(), sheet, &layout, rect, dark);
                });
                output.textures_delta.clear();
                for zone in &sheet.zones {
                    let fill = color32(zone.color.unwrap());
                    let matches: Vec<_> = output
                        .shapes
                        .iter()
                        .filter_map(|shape| match &shape.shape {
                            egui::Shape::Rect(frame) if frame.fill == fill => Some(frame.rect),
                            _ => None,
                        })
                        .collect();
                    assert_eq!(matches.len(), 1, "{id}: {}", zone.name);
                    assert!(
                        visible.contains(matches[0].min) && visible.contains(matches[0].max),
                        "{id}: {} is outside the thumbnail: {:?}",
                        zone.name,
                        matches[0]
                    );
                }
            }
        }
    }
}
