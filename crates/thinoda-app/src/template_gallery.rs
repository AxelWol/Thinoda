//! Built-in template previews, generated from the actual documents that Create opens.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};

use egui::{Color32, Painter, Pos2, Stroke, StrokeKind, Ui, vec2};
use thinoda_canvas::{PaintOptions, Xf, color32};
use thinoda_core::{
    GridKind, Layout, Sheet, SheetRootMode,
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
    recency: VecDeque<SampleKey>,
}

impl SampleCache {
    fn limit() -> usize {
        (templates::TEMPLATES.len() * 2).max(1)
    }

    fn get(&mut self, dark: bool, language: Lang, id: &'static str) -> Arc<Sample> {
        let key = (dark, language as u8, id);
        if let Some(sample) = self.entries.get(&key).cloned() {
            let position = self
                .recency
                .iter()
                .position(|entry| *entry == key)
                .expect("cached preview must have a recency entry");
            self.recency.remove(position);
            self.recency.push_back(key);
            return sample;
        }

        let sheet = templates::instantiate_localized(id, &|s| t_in(language, s).to_owned())
            .sheets
            .remove(0);
        let layout = layout_sheet(&sheet, &ApproxMeasure, dark);
        let sample = Arc::new((sheet, layout));
        if self.entries.len() >= Self::limit() {
            let oldest = self.recency.pop_front().expect("preview cache is nonempty");
            self.entries
                .remove(&oldest)
                .expect("recency entry must have a cached preview");
        }
        self.entries.insert(key, Arc::clone(&sample));
        self.recency.push_back(key);
        sample
    }
}

#[cfg(test)]
thread_local! {
    static SAMPLE_REQUESTS: std::cell::RefCell<Vec<&'static str>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Cache visible samples by template, appearance and UI language, bounded to
/// two full template sets.
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
        || sheet
            .zones
            .iter()
            .any(|zone| zone.color.is_some() || zone.background_color.is_some())
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
        let frame = zone.color.map_or(ink, color32);
        let fill = zone
            .background_color
            .or(zone.color)
            .map_or(ink.gamma_multiply(0.04), |color| {
                color32(color).gamma_multiply(0.16)
            });
        let title_color = color32(thinoda_core::Color::highest_contrast_on(
            crate::ui::to_color(fill).over(background),
        ));
        painter.rect(r, 2.0, fill, Stroke::new(0.5, frame), StrokeKind::Inside);
        painter.with_clip_rect(r.shrink(1.0)).text(
            r.left_top() + vec2(3.0, 2.0),
            egui::Align2::LEFT_TOP,
            &zone.name,
            egui::FontId::proportional((14.0 * zoom).clamp(5.0, 9.0)),
            title_color,
        );
    }
    thinoda_canvas::paint_map(
        &painter,
        sheet,
        layout,
        xf,
        &PaintOptions {
            dark,
            accent: thinoda_ui::Palette::of(painter.ctx()).accent,
            omit_zones: true,
            ..Default::default()
        },
    );
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
                    assert!(cache.entries.len() <= SampleCache::limit());
                    assert_eq!(cache.recency.len(), cache.entries.len());
                }
            }
        }
        assert_eq!(cache.entries.len(), SampleCache::limit());
        assert_eq!(cache.recency.len(), cache.entries.len());
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
    fn preview_cache_evicts_least_recently_used_without_invalidating_samples() {
        let mut cache = SampleCache::default();
        let first = templates::TEMPLATES[0].id;
        let second = templates::TEMPLATES[1].id;
        let protected = cache.get(false, Lang::En, first);
        let evicted = cache.get(false, Lang::En, second);
        for info in &templates::TEMPLATES[2..] {
            cache.get(false, Lang::En, info.id);
        }
        for info in templates::TEMPLATES {
            cache.get(true, Lang::En, info.id);
        }
        assert_eq!(cache.entries.len(), SampleCache::limit());
        assert!(Arc::ptr_eq(&protected, &cache.get(false, Lang::En, first)));
        cache.get(false, Lang::De, first);
        assert!(cache.entries.contains_key(&(false, Lang::En as u8, first)));
        assert!(!cache.entries.contains_key(&(false, Lang::En as u8, second)));
        assert_eq!(cache.entries.len(), SampleCache::limit());
        assert_eq!(cache.recency.len(), cache.entries.len());

        let rebuilt = cache.get(false, Lang::En, second);
        assert!(!Arc::ptr_eq(&evicted, &rebuilt));
        assert_eq!(evicted.as_ref(), rebuilt.as_ref());
        assert_eq!(cache.entries.len(), SampleCache::limit());
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
        let last_board = templates::TEMPLATES
            .iter()
            .rev()
            .find(|info| info.category == TemplateCategory::Boards)
            .unwrap();
        assert!(bottom.contains(&last_board.id), "{bottom:?}");
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
            "swimlane",
            "venn-diagram",
            "empathy-map",
            "storyboard",
            "user-journey-map",
            "moodboard",
        ] {
            assert!(counts[id] > counts["blank-canvas"], "{id}: {counts:?}");
        }
    }

    #[test]
    fn board_thumbnail_uses_the_current_accent_for_automatic_strokes() {
        let mut sheet = crate::visual_export::test_helpers::sheet();
        sheet
            .add_canvas_object(thinoda_core::CanvasObject::shape(
                thinoda_core::ShapeKind::Rectangle,
                thinoda_core::Rect::from_center_size(
                    thinoda_core::Vec2::new(100.0, 130.0),
                    thinoda_core::Vec2::new(80.0, 40.0),
                ),
            ))
            .unwrap();
        for dark in [false, true] {
            let accent = Color32::from_rgb(221, 12, 179);
            let ctx = crate::visual_export::test_helpers::context(accent, dark);
            let layout = layout_sheet(&sheet, &ApproxMeasure, dark);
            let rect = egui::Rect::from_min_size(Pos2::ZERO, vec2(160.0, 94.0));
            for accent in [accent, Color32::TRANSPARENT] {
                crate::visual_export::test_helpers::set_accent(&ctx, accent, dark);
                let mut output = ctx.run_ui(crate::visual_export::test_helpers::input(), |ui| {
                    paint_board_thumbnail(ui.painter(), &sheet, &layout, rect, dark);
                });
                output.textures_delta.clear();
                crate::visual_export::test_helpers::assert_stroke(
                    &output,
                    thinoda_canvas::automatic_board_stroke(accent, dark),
                );
                assert!(output.shapes.iter().any(|shape| matches!(
                    &shape.shape,
                    egui::Shape::Path(path)
                        if path.closed
                            && path.stroke.color == egui::epaint::ColorMode::Solid(
                                thinoda_canvas::automatic_board_stroke(accent, dark)
                            )
                )));
            }
        }
    }

    #[test]
    fn venn_thumbnail_preserves_translucent_overlapping_shapes() {
        let wb = templates::instantiate("venn-diagram");
        let sheet = wb.active();
        let rect = egui::Rect::from_min_size(Pos2::ZERO, vec2(160.0, 94.0));
        let visible = rect.shrink(5.0).expand(0.01);
        for dark in [false, true] {
            let layout = layout_sheet(sheet, &ApproxMeasure, dark);
            let ctx = egui::Context::default();
            let mut output = ctx.run_ui(Default::default(), |ui| {
                paint_board_thumbnail(ui.painter(), sheet, &layout, rect, dark);
            });
            output.textures_delta.clear();
            for object in &sheet.canvas.objects {
                let fill = color32(object.style.fill.unwrap()).gamma_multiply(object.style.opacity);
                assert!(fill.a() < 255);
                let paths: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Path(path) if path.fill == fill => Some(path),
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    paths.len(),
                    1,
                    "thumbnail must paint circle {} exactly once",
                    object.id.0
                );
                assert!(
                    paths[0].points.iter().all(|point| visible.contains(*point)),
                    "circle {} is clipped in {dark:?} theme",
                    object.id.0
                );
            }
        }
    }

    #[test]
    fn board_thumbnail_paints_objects_once_in_layer_order_with_frame_clipping() {
        use thinoda_core::{
            Band, CanvasConnector, CanvasObject, Color, Endpoint, InkStroke, InkTool, Vec2,
        };

        fn has_color(shape: &egui::Shape, color: Color32) -> bool {
            match shape {
                egui::Shape::Path(path) => {
                    path.fill == color
                        || matches!(path.stroke.color, egui::epaint::ColorMode::Solid(c) if c == color)
                }
                egui::Shape::Mesh(mesh) => mesh.vertices.iter().any(|vertex| vertex.color == color),
                egui::Shape::Circle(circle) => circle.fill == color,
                egui::Shape::LineSegment { stroke, .. } => stroke.color == color,
                _ => false,
            }
        }

        let mut wb = templates::instantiate("venn-diagram");
        let sheet = wb.active_mut();
        sheet.canvas.objects[1].band = Band::Above;
        let circle_colors: Vec<_> = sheet
            .canvas
            .objects
            .iter()
            .map(|circle| color32(circle.style.fill.unwrap()).gamma_multiply(circle.style.opacity))
            .collect();

        let frame_rect =
            thinoda_core::Rect::from_min_size(Vec2::new(0.0, 0.0), Vec2::new(280.0, 360.0));
        let frame = sheet.create_board_frame("Clip frame", frame_rect).unwrap();
        let zone = sheet
            .zones
            .iter_mut()
            .find(|zone| zone.id == frame)
            .unwrap();
        zone.clip = true;
        zone.color = Some(Color::rgba(100, 40, 180, 110));
        let frame_color = color32(zone.color.unwrap());
        let frame_fill = frame_color.gamma_multiply(0.16);

        let connector_colors = [Color::rgb(230, 25, 150), Color::rgb(25, 210, 170)];
        let ink_colors = [Color::rgb(200, 80, 15), Color::rgb(90, 45, 230)];
        for (index, band) in [Band::Below, Band::Above].into_iter().enumerate() {
            let x = if index == 0 { 60.0 } else { 520.0 };
            let mut connector = CanvasConnector::new(
                Endpoint::Point(Vec2::new(x, 280.0)),
                Endpoint::Point(Vec2::new(x + 170.0, 300.0)),
            );
            connector.arrow_end = false;
            connector.color = Some(connector_colors[index]);
            let mut connector = CanvasObject::connector(connector);
            connector.band = band;
            sheet.add_canvas_object(connector).unwrap();

            let mut ink = CanvasObject::ink(InkStroke::new(
                InkTool::Pen,
                ink_colors[index],
                20.0,
                vec![Vec2::new(x, 310.0), Vec2::new(x + 100.0, 320.0)],
            ));
            ink.band = band;
            sheet.add_canvas_object(ink).unwrap();
        }
        wb.validate().unwrap();

        let rect = egui::Rect::from_min_size(Pos2::ZERO, vec2(160.0, 94.0));
        let visible = rect.shrink(5.0);
        for dark in [false, true] {
            let sheet = wb.active();
            let layout = layout_sheet(sheet, &ApproxMeasure, dark);
            let zoom = (visible.width() / layout.bounds.size.x)
                .min(visible.height() / layout.bounds.size.y);
            let center = layout.bounds.center();
            let xf = Xf {
                origin: visible.center() - vec2(center.x, center.y) * zoom,
                zoom,
            };
            let frame_clip = visible.intersect(xf.rect(frame_rect));
            let ctx = egui::Context::default();
            let mut output = ctx.run_ui(Default::default(), |ui| {
                paint_board_thumbnail(ui.painter(), sheet, &layout, rect, dark);
            });
            output.textures_delta.clear();

            let single_index = |color| {
                let indices: Vec<_> = output
                    .shapes
                    .iter()
                    .enumerate()
                    .filter(|(_, clipped)| has_color(&clipped.shape, color))
                    .map(|(index, _)| index)
                    .collect();
                assert_eq!(indices.len(), 1, "color {color:?}, dark={dark}");
                indices[0]
            };
            let below = [
                single_index(circle_colors[0]),
                single_index(color32(connector_colors[0])),
                single_index(color32(ink_colors[0])),
            ];
            let above = [
                single_index(circle_colors[1]),
                single_index(color32(connector_colors[1])),
                single_index(color32(ink_colors[1])),
            ];
            let topic = output
                .shapes
                .iter()
                .position(|shape| {
                    matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "Shared")
                })
                .expect("shared card must be painted");
            let frame_indices: Vec<_> = output
                .shapes
                .iter()
                .enumerate()
                .filter_map(|(index, shape)| {
                    matches!(&shape.shape, egui::Shape::Rect(frame) if frame.fill == frame_fill)
                        .then_some(index)
                })
                .collect();
            assert_eq!(frame_indices.len(), 1);
            let frame_index = frame_indices[0];
            let grid_color = if dark {
                Color32::from_gray(170)
            } else {
                Color32::from_gray(85)
            }
            .gamma_multiply(0.35);
            assert!(output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Circle(dot) if dot.fill == grid_color)
            }));
            assert!(
                below
                    .iter()
                    .all(|index| frame_index < *index && *index < topic)
            );
            assert!(above.iter().all(|index| topic < *index));
            for index in below {
                assert_eq!(output.shapes[index].clip_rect, frame_clip);
            }
            for index in above {
                assert_eq!(output.shapes[index].clip_rect, visible);
            }
        }
    }

    #[test]
    fn new_frame_board_thumbnails_fit_every_frame() {
        let rect = egui::Rect::from_min_size(Pos2::ZERO, vec2(160.0, 94.0));
        let visible = rect.shrink(5.0).expand(0.01);
        for id in [
            "affinity-diagram",
            "eisenhower-matrix",
            "meeting-board",
            "swimlane",
            "empathy-map",
            "storyboard",
            "user-journey-map",
        ] {
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
                    let fill =
                        color32(zone.background_color.or(zone.color).unwrap()).gamma_multiply(0.16);
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
