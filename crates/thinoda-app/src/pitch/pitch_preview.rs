use super::*;

#[cfg(test)]
#[path = "preview_tests.rs"]
mod tests;

#[cfg(test)]
thread_local! {
    // Count actual thumbnail resolver calls, not allocated cards, on this UI/test thread.
    pub(super) static THUMBNAIL_RESOLVES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// No image worker or stale texture: the editor paints the same resolved scene as Pitch.
#[derive(Default)]
pub(super) struct SlidePreview;

impl SlidePreview {
    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        sheet: &Sheet,
        layout: &thinoda_core::Layout,
        index: usize,
        available: egui::Vec2,
    ) -> Option<TopicId> {
        self.paint(ui, sheet, layout, index, available, true)
    }

    pub(super) fn thumbnail(
        &mut self,
        ui: &mut egui::Ui,
        sheet: &Sheet,
        layout: &thinoda_core::Layout,
        index: usize,
        available: egui::Vec2,
    ) -> bool {
        self.paint(ui, sheet, layout, index, available, false)
            .is_some()
    }

    fn paint(
        &mut self,
        ui: &mut egui::Ui,
        sheet: &Sheet,
        layout: &thinoda_core::Layout,
        index: usize,
        available: egui::Vec2,
        interactive: bool,
    ) -> Option<TopicId> {
        let slide = sheet.presentation.slides.get(index)?;
        // Allocate first so the ScrollArea's viewport can reject offscreen thumbnails.
        // No resolved layout or cloned paint sheet is kept for any slide between frames.
        // The selected workspace stage always stays live, even when clipped by scrolling.
        let (region, response) =
            ui.allocate_exact_size(available.max(egui::vec2(1.0, 1.0)), egui::Sense::click());
        if !interactive && !ui.is_rect_visible(region) {
            return None;
        }
        #[cfg(test)]
        if !interactive {
            THUMBNAIL_RESOLVES.with(|count| count.set(count.get() + 1));
        }
        let resolved = match thinoda_core::resolve_presentation_slide(sheet, slide, layout) {
            Ok(resolved) => resolved,
            Err(_) => {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    t("Slide source is unavailable."),
                );
                return None;
            }
        };
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::Role::Image,
                true,
                if interactive {
                    t("Slide preview")
                } else {
                    &slide.title
                },
            )
        });
        let painter = ui.painter_at(region);
        painter.rect_filled(region, 8.0, Palette::of_ui(ui).surface_alt);
        let inner = region.shrink(12.0);
        let zoom = (inner.width() / resolved.frame.size.x.max(1.0))
            .min(inner.height() / resolved.frame.size.y.max(1.0));
        let center = resolved.frame.center();
        let xf = thinoda_canvas::Xf {
            origin: inner.center() - egui::vec2(center.x, center.y) * zoom,
            zoom,
        };
        let crop = xf.rect(resolved.frame).intersect(inner);
        let paint_sheet = presentation_paint_sheet(sheet, &resolved);
        thinoda_canvas::paint_map(
            &painter.with_clip_rect(crop),
            &paint_sheet,
            &resolved.layout,
            xf,
            &thinoda_canvas::PaintOptions {
                dark: Palette::of_ui(ui).dark,
                ..Default::default()
            },
        );
        let hit = response
            .hover_pos()
            .filter(|pos| interactive && crop.contains(*pos))
            .and_then(|pos| preview_topic_at(&paint_sheet, &resolved.layout, xf.sheet(pos)));
        if let Some((_, bounds)) = hit {
            painter.rect_stroke(
                xf.rect(bounds).intersect(crop),
                4.0,
                egui::Stroke::new(2.0, Palette::of_ui(ui).accent),
                egui::StrokeKind::Inside,
            );
            response
                .clone()
                .on_hover_text(t("Click a topic to hide its subtree."));
        }
        if response.clicked() && widgets::touch_activation_allowed(ui.ctx()) {
            if interactive {
                hit.map(|(topic, _)| topic)
            } else {
                Some(sheet.root)
            }
        } else {
            None
        }
    }
}

/// Hit only topics, but use their actual rotation and any scene frame clip.
fn preview_topic_at(
    sheet: &Sheet,
    layout: &thinoda_core::Layout,
    point: thinoda_core::Vec2,
) -> Option<(TopicId, Rect)> {
    let mut scene = thinoda_core::build_scene(sheet, layout);
    scene
        .items
        .retain(|item| matches!(item.id, thinoda_core::SceneId::Topic(_)));
    let thinoda_core::SceneHit::Topic(topic) = scene.hit_test_tolerance(point, 0.0)? else {
        return None;
    };
    let bounds = scene
        .item(thinoda_core::SceneId::Topic(topic))?
        .visible_bounds()?;
    Some((topic, bounds))
}
