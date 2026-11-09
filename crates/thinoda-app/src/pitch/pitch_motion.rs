use super::*;
use std::collections::HashSet;

const DURATION: f64 = 0.35;

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub(super) enum Transition {
    #[default]
    None,
    Fade,
    Slide,
    Push,
    Zoom,
}

impl Transition {
    pub(super) const ALL: [Self; 5] = [Self::None, Self::Fade, Self::Slide, Self::Push, Self::Zoom];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Fade => "Fade",
            Self::Slide => "Slide",
            Self::Push => "Push",
            Self::Zoom => "Zoom",
        }
    }
}

#[derive(Clone)]
struct Scene {
    index: usize,
    step: usize,
    frame: Rect,
    layout: Arc<thinoda_core::Layout>,
    zones: Vec<thinoda_core::Zone>,
    canvas: Arc<thinoda_core::CanvasLayer>,
}

pub(super) struct Playback {
    pub transition: Transition,
    pub reduce_motion: bool,
    pub build_topics: bool,
    pub step: usize,
    current: Option<Scene>,
    outgoing: Option<Scene>,
    started: f64,
    direction: f32,
}

impl Default for Playback {
    fn default() -> Self {
        Self {
            transition: Transition::None,
            reduce_motion: true,
            build_topics: false,
            step: usize::MAX,
            current: None,
            outgoing: None,
            started: 0.0,
            direction: 1.0,
        }
    }
}

impl Playback {
    pub(super) fn refresh(&mut self) {
        self.current = None;
        self.outgoing = None;
    }

    pub(super) fn topic_order(
        sheet: &Sheet,
        layout: &thinoda_core::Layout,
        index: usize,
    ) -> Vec<TopicId> {
        let Some(slide) = sheet.presentation.slides.get(index) else {
            return Vec::new();
        };
        let Ok(resolved) = thinoda_core::resolve_presentation_slide(sheet, slide, layout) else {
            return Vec::new();
        };
        let frame = resolved.frame;
        thinoda_core::build_scene(sheet, &resolved.layout)
            .items
            .into_iter()
            .filter_map(|item| {
                let thinoda_core::SceneId::Topic(id) = item.id else {
                    return None;
                };
                item.visible_bounds()
                    .is_some_and(|bounds| bounds.intersects(&frame))
                    .then_some(id)
            })
            .collect()
    }

    pub(super) fn navigate(
        &mut self,
        sheet: &Sheet,
        layout: &thinoda_core::Layout,
        selected: &mut usize,
        forward: bool,
    ) {
        let steps = Self::topic_order(sheet, layout, *selected).len().max(1);
        if self.build_topics {
            self.step = self.step.min(steps);
        }
        if self.build_topics && forward && self.step < steps {
            self.step += 1;
        } else if self.build_topics && !forward && self.step > 1 {
            self.step -= 1;
        } else if forward && *selected + 1 < sheet.presentation.slides.len() {
            *selected += 1;
            self.step = if self.build_topics { 1 } else { usize::MAX };
        } else if !forward && *selected > 0 {
            *selected -= 1;
            self.step = usize::MAX;
        }
    }

    fn scene(
        &self,
        sheet: &Sheet,
        layout: &Arc<thinoda_core::Layout>,
        index: usize,
    ) -> Result<Scene, String> {
        let slide = sheet
            .presentation
            .slides
            .get(index)
            .ok_or_else(|| t("Slide source is unavailable.").to_string())?;
        let mut resolved = thinoda_core::resolve_presentation_slide(sheet, slide, layout)?;
        let order = Self::topic_order(sheet, layout, index);
        let step = if self.build_topics {
            self.step.min(order.len().max(1))
        } else {
            usize::MAX
        };
        if self.build_topics && step < order.len() {
            let visible: HashSet<_> = order.iter().take(step).copied().collect();
            let composed = &mut resolved.layout;
            composed.topics.retain(|id, _| visible.contains(id));
            composed.order.retain(|id| visible.contains(id));
            composed.connectors.retain(|connector| {
                visible.contains(&connector.parent) && visible.contains(&connector.child)
            });
            composed.relationships.retain(|relationship| {
                sheet.relationships.iter().any(|source| {
                    source.id == relationship.id
                        && visible.contains(&source.from)
                        && visible.contains(&source.to)
                })
            });
            // Group annotations can disclose unrevealed text; reveal them with the complete slide.
            composed.boundaries.clear();
            composed.summaries.clear();
            resolved.zones.clear();
        }
        let paint_sheet = presentation_paint_sheet(sheet, &resolved);
        Ok(Scene {
            index,
            step,
            frame: resolved.frame,
            layout: Arc::new(resolved.layout),
            zones: paint_sheet.zones,
            canvas: Arc::new(paint_sheet.canvas),
        })
    }

    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        sheet: &Sheet,
        layout: &Arc<thinoda_core::Layout>,
        index: usize,
        size: egui::Vec2,
        dark: bool,
    ) {
        let now = ui.input(|input| input.time);
        let step = if self.build_topics {
            self.step
                .min(Self::topic_order(sheet, layout, index).len().max(1))
        } else {
            usize::MAX
        };
        if self
            .current
            .as_ref()
            .is_none_or(|scene| scene.index != index || scene.step != step)
        {
            let target = match self.scene(sheet, layout, index) {
                Ok(scene) => scene,
                Err(_) => {
                    self.refresh();
                    ui.colored_label(
                        ui.visuals().error_fg_color,
                        t("Slide source is unavailable."),
                    );
                    return;
                }
            };
            self.direction = if self.current.as_ref().is_some_and(|scene| {
                scene.index > index || scene.index == index && scene.step > step
            }) {
                -1.0
            } else {
                1.0
            };
            self.outgoing = self.current.replace(target);
            self.started = now;
        }
        let progress = if self.reduce_motion || self.transition == Transition::None {
            1.0
        } else {
            ((now - self.started) / DURATION).clamp(0.0, 1.0) as f32
        };
        let eased = progress * progress * (3.0 - 2.0 * progress);
        let (region, response) =
            ui.allocate_exact_size(size.max(egui::vec2(1.0, 1.0)), egui::Sense::hover());
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, t("Slide preview")));
        let painter = ui.painter_at(region);
        painter.rect_filled(region, 0.0, background(sheet, dark));
        if let Some(current) = &self.current {
            if let Some(outgoing) = &self.outgoing
                && progress < 1.0
            {
                match self.transition {
                    Transition::None => {}
                    Transition::Fade => {
                        paint_scene(&painter, region, sheet, outgoing, dark, 1.0 - eased);
                        paint_scene(&painter, region, sheet, current, dark, eased);
                    }
                    Transition::Slide | Transition::Push => {
                        let distance = region.width() * self.direction;
                        let old_offset = if self.transition == Transition::Push {
                            -distance * eased
                        } else {
                            0.0
                        };
                        paint_scene(
                            &painter,
                            region.translate(egui::vec2(old_offset, 0.0)),
                            sheet,
                            outgoing,
                            dark,
                            1.0,
                        );
                        paint_scene(
                            &painter,
                            region.translate(egui::vec2(distance * (1.0 - eased), 0.0)),
                            sheet,
                            current,
                            dark,
                            1.0,
                        );
                    }
                    Transition::Zoom => {
                        let mut scene = current.clone();
                        scene.frame.min =
                            outgoing.frame.min + (current.frame.min - outgoing.frame.min) * eased;
                        scene.frame.size = outgoing.frame.size
                            + (current.frame.size - outgoing.frame.size) * eased;
                        paint_scene(&painter, region, sheet, &scene, dark, 1.0);
                    }
                }
                ui.ctx().request_repaint();
            } else {
                paint_scene(&painter, region, sheet, current, dark, 1.0);
            }
        }
        if progress >= 1.0 {
            self.outgoing = None;
        }
    }
}

fn paint_scene(
    painter: &egui::Painter,
    region: egui::Rect,
    sheet: &Sheet,
    scene: &Scene,
    dark: bool,
    opacity: f32,
) {
    let zoom = (region.width() / scene.frame.size.x.max(1.0))
        .min(region.height() / scene.frame.size.y.max(1.0));
    let center = scene.frame.center();
    let xf = thinoda_canvas::Xf {
        origin: region.center() - egui::vec2(center.x, center.y) * zoom,
        zoom,
    };
    let crop = xf.rect(scene.frame).intersect(painter.clip_rect());
    let mut painter = painter.with_clip_rect(crop);
    painter.set_opacity(opacity);
    painter.rect_filled(crop, 0.0, background(sheet, dark));
    let mut paint_sheet = sheet.clone();
    paint_sheet.zones = scene.zones.clone();
    paint_sheet.canvas = (*scene.canvas).clone();
    thinoda_canvas::paint_map(
        &painter,
        &paint_sheet,
        &scene.layout,
        xf,
        &thinoda_canvas::PaintOptions {
            dark,
            accent: thinoda_ui::Palette::of(painter.ctx()).accent,
            ..Default::default()
        },
    );
}

fn background(sheet: &Sheet, dark: bool) -> egui::Color32 {
    thinoda_canvas::color32(
        thinoda_core::theme::Theme::for_settings(&sheet.theme).background(&sheet.theme, dark),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playback_uses_the_current_accent_for_automatic_board_strokes() {
        let sheet = crate::visual_export::test_helpers::sheet();
        for dark in [false, true] {
            let accent = egui::Color32::from_rgb(221, 12, 179);
            let ctx = crate::visual_export::test_helpers::context(accent, dark);
            let layout = Arc::new(thinoda_core::layout_sheet(
                &sheet,
                &thinoda_core::ApproxMeasure,
                dark,
            ));
            let mut playback = Playback::default();
            for accent in [accent, egui::Color32::TRANSPARENT] {
                crate::visual_export::test_helpers::set_accent(&ctx, accent, dark);
                let mut output = ctx.run_ui(crate::visual_export::test_helpers::input(), |ui| {
                    playback.show(ui, &sheet, &layout, 0, egui::vec2(400.0, 300.0), dark);
                });
                output.textures_delta.clear();
                crate::visual_export::test_helpers::assert_stroke(
                    &output,
                    thinoda_canvas::automatic_board_stroke(accent, dark),
                );
            }
        }
    }

    #[test]
    fn pitch_viewport_paints_board_objects_with_scene_and_viewport_clips() {
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = thinoda_core::SheetRootMode::Hidden;
        sheet.add_floating("Floating", thinoda_core::Vec2::new(30.0, 60.0));
        let clip = Rect::from_min_size(
            thinoda_core::Vec2::ZERO,
            thinoda_core::Vec2::new(100.0, 100.0),
        );
        sheet.create_board_frame("Frame", clip).unwrap();
        sheet.zones[0].clip = true;
        let mut object = thinoda_core::CanvasObject::shape(
            thinoda_core::ShapeKind::Rectangle,
            Rect::from_min_size(
                thinoda_core::Vec2::new(60.0, 20.0),
                thinoda_core::Vec2::new(60.0, 30.0),
            ),
        );
        object.style.fill = Some(thinoda_core::Color::rgb(225, 20, 30));
        sheet.add_canvas_object(object).unwrap();
        let frame = Rect::from_min_size(
            thinoda_core::Vec2::ZERO,
            thinoda_core::Vec2::new(160.0, 120.0),
        );
        sheet
            .add_presentation_slide("View", PresentationSource::Viewport, frame)
            .unwrap();
        let layout = Arc::new(thinoda_core::layout_sheet(
            &sheet,
            &thinoda_core::ApproxMeasure,
            false,
        ));
        let mut playback = Playback::default();
        let scene = playback.scene(&sheet, &layout, 0).unwrap();
        assert_eq!(scene.frame, frame);
        assert!(!scene.layout.topics.contains_key(&sheet.root));
        assert_eq!(scene.zones.len(), 1);
        assert_eq!(scene.canvas.objects.len(), 1);

        let ctx = Context::default();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 300.0),
                )),
                ..Default::default()
            },
            |ui| playback.show(ui, &sheet, &layout, 0, egui::vec2(320.0, 240.0), false),
        );
        let red = egui::Color32::from_rgb(225, 20, 30);
        let clipped = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Path(path) if path.fill == red => Some((shape.clip_rect, path)),
                _ => None,
            })
            .expect("board shape must be painted by the Pitch stage");
        assert!(
            clipped
                .1
                .points
                .iter()
                .any(|point| point.x > clipped.0.right()),
            "the crossing shape must be clipped at the frame boundary"
        );
        assert!(
            clipped
                .1
                .points
                .iter()
                .any(|point| clipped.0.contains(*point))
        );
        output.textures_delta.clear();
    }

    #[test]
    fn pitch_source_scenes_filter_other_zones_objects_in_editor_and_player() {
        use thinoda_core::{
            CanvasConnector, CanvasObject, Color, Endpoint, InkStroke, InkTool, PresentationScope,
            ShapeKind, SheetRootMode, Vec2,
        };

        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let root = sheet.add_floating("Root", Vec2::new(80.0, 80.0));
        let frame = Rect::from_min_size(Vec2::ZERO, Vec2::new(400.0, 300.0));
        let zone = sheet.create_zone("First", vec![root], frame).unwrap();
        let other = sheet.create_board_frame("Other", frame).unwrap();
        let ink = sheet
            .add_canvas_object(CanvasObject::ink(InkStroke::new(
                InkTool::Pen,
                Color::rgb(10, 20, 30),
                4.0,
                vec![Vec2::new(20.0, 20.0), Vec2::new(60.0, 20.0)],
            )))
            .unwrap();
        let foreign = sheet
            .add_canvas_object(CanvasObject::shape(ShapeKind::Rectangle, frame))
            .unwrap();
        let free = sheet
            .add_canvas_object(CanvasObject::shape(ShapeKind::Ellipse, frame))
            .unwrap();
        let connector = sheet
            .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
                Endpoint::Point(Vec2::new(30.0, 30.0)),
                Endpoint::Object(foreign),
            )))
            .unwrap();
        let free_connector = sheet
            .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
                Endpoint::Point(Vec2::new(40.0, 40.0)),
                Endpoint::Object(free),
            )))
            .unwrap();
        let indirect = sheet
            .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
                Endpoint::Point(Vec2::new(50.0, 50.0)),
                Endpoint::Object(connector),
            )))
            .unwrap();
        let free_indirect = sheet
            .add_canvas_object(CanvasObject::connector(CanvasConnector::new(
                Endpoint::Point(Vec2::new(60.0, 60.0)),
                Endpoint::Object(free_connector),
            )))
            .unwrap();
        sheet
            .set_zone_members(zone, vec![root], vec![ink, connector, indirect])
            .unwrap();
        sheet
            .set_zone_members(other, vec![], vec![foreign])
            .unwrap();
        for source in [
            PresentationSource::Zone(zone),
            PresentationSource::Branch(root),
            PresentationSource::Zone(zone),
            PresentationSource::Viewport,
        ] {
            sheet
                .add_presentation_slide("Scene", source, frame)
                .unwrap();
        }
        sheet.presentation.slides[0].scope = PresentationScope::Source;
        sheet.presentation.slides[1].scope = PresentationScope::Source;
        sheet.presentation.slides[2].scope = PresentationScope::Context;
        sheet.validate().unwrap();
        let before = sheet.clone();
        let layout = Arc::new(thinoda_core::layout_sheet(
            &sheet,
            &thinoda_core::ApproxMeasure,
            false,
        ));

        for (index, expected) in [
            HashSet::from([ink, free, free_connector, free_indirect]),
            HashSet::from([free, free_connector, free_indirect]),
            HashSet::from([
                ink,
                foreign,
                free,
                connector,
                free_connector,
                indirect,
                free_indirect,
            ]),
            HashSet::from([
                ink,
                foreign,
                free,
                connector,
                free_connector,
                indirect,
                free_indirect,
            ]),
        ]
        .into_iter()
        .enumerate()
        {
            let resolved = thinoda_core::resolve_presentation_slide(
                &sheet,
                &sheet.presentation.slides[index],
                &layout,
            )
            .unwrap();
            let editor_sheet = presentation_paint_sheet(&sheet, &resolved);
            let editor_objects: HashSet<_> = editor_sheet
                .canvas
                .objects
                .iter()
                .map(|object| object.id)
                .collect();
            let scene = Playback::default().scene(&sheet, &layout, index).unwrap();
            let player_objects: HashSet<_> = scene
                .canvas
                .objects
                .iter()
                .map(|object| object.id)
                .collect();
            assert_eq!(editor_objects, expected, "editor slide {index}");
            assert_eq!(player_objects, expected, "player slide {index}");
            let expected_order: Vec<_> = sheet
                .canvas
                .objects
                .iter()
                .map(|object| object.id)
                .filter(|id| expected.contains(id))
                .collect();
            assert_eq!(
                editor_sheet
                    .canvas
                    .objects
                    .iter()
                    .map(|object| object.id)
                    .collect::<Vec<_>>(),
                expected_order
            );
            assert_eq!(
                scene
                    .canvas
                    .objects
                    .iter()
                    .map(|object| object.id)
                    .collect::<Vec<_>>(),
                expected_order
            );
        }
        assert_eq!(sheet, before);
    }

    #[test]
    fn pitch_hidden_and_unrevealed_topics_do_not_reveal_scene_connectors() {
        let mut sheet = Sheet::new("Board", "");
        sheet.root_mode = thinoda_core::SheetRootMode::Hidden;
        let root = sheet.add_floating("Root", thinoda_core::Vec2::new(80.0, 80.0));
        let child = sheet.add_child(root, None, "Hidden child");
        let zone = sheet
            .create_zone(
                "Clipping frame",
                vec![root],
                Rect::from_min_size(
                    thinoda_core::Vec2::ZERO,
                    thinoda_core::Vec2::new(400.0, 300.0),
                ),
            )
            .unwrap();
        sheet.zones[0].clip = true;
        sheet.zones[0].background_color = Some(thinoda_core::Color::rgb(190, 30, 70));
        let mut connector = thinoda_core::CanvasConnector::new(
            thinoda_core::Endpoint::Topic(child),
            thinoda_core::Endpoint::Point(thinoda_core::Vec2::new(180.0, 30.0)),
        );
        connector.label = "Hidden connector".into();
        let connector = sheet
            .add_canvas_object(thinoda_core::CanvasObject::connector(connector))
            .unwrap();
        let dependent = sheet
            .add_canvas_object(thinoda_core::CanvasObject::connector(
                thinoda_core::CanvasConnector::new(
                    thinoda_core::Endpoint::Object(connector),
                    thinoda_core::Endpoint::Point(thinoda_core::Vec2::new(200.0, 40.0)),
                ),
            ))
            .unwrap();
        sheet
            .add_presentation_slide(
                "View",
                PresentationSource::Viewport,
                Rect::from_min_size(
                    thinoda_core::Vec2::new(-100.0, -100.0),
                    thinoda_core::Vec2::new(600.0, 500.0),
                ),
            )
            .unwrap();
        let layout = Arc::new(thinoda_core::layout_sheet(
            &sheet,
            &thinoda_core::ApproxMeasure,
            false,
        ));
        sheet.presentation.slides[0].hidden_topics.push(child);
        let scene = Playback::default().scene(&sheet, &layout, 0).unwrap();
        assert!(
            scene
                .canvas
                .objects
                .iter()
                .all(|item| item.id != connector && item.id != dependent)
        );
        let retained_clip = scene.zones.iter().find(|item| item.id == zone).unwrap();
        assert!(retained_clip.clip);
        assert!(retained_clip.name.is_empty());
        assert_eq!(retained_clip.color.unwrap().a, 0);
        assert!(retained_clip.background_color.is_none());

        sheet.presentation.slides[0].hidden_topics.clear();
        let mut playback = Playback {
            build_topics: true,
            step: 1,
            ..Default::default()
        };
        let first = playback.scene(&sheet, &layout, 0).unwrap();
        assert!(
            first
                .canvas
                .objects
                .iter()
                .all(|item| item.id != connector && item.id != dependent)
        );
        playback.step = usize::MAX;
        let complete = playback.scene(&sheet, &layout, 0).unwrap();
        for id in [connector, dependent] {
            assert!(complete.canvas.objects.iter().any(|item| item.id == id));
        }
    }

    fn player_with_builds() -> PitchPlayer {
        let mut sheet = Sheet::new("Builds", "Center");
        for title in ["Opening", "Middle", "Closing"] {
            let branch = sheet.insert_child(sheet.root, title).unwrap();
            let first = sheet.insert_child(branch, "First detail").unwrap();
            let last = sheet.insert_child(branch, "Last detail").unwrap();
            let id = sheet.alloc_relationship_id();
            sheet.relationships.push(thinoda_core::Relationship {
                id,
                from: first,
                to: last,
                title: "Connection".into(),
                color: None,
                control_points: None,
            });
        }
        sheet.presentation =
            thinoda_io::generate_presentation(&sheet, &Default::default()).unwrap();
        let mut app = super::super::tests::setup(false);
        app.doc_mut()
            .unwrap()
            .doc
            .replace(thinoda_core::Workbook::new(sheet));
        app.open_pitch(&Context::default(), false);
        app.pitch.player.take().unwrap()
    }

    #[test]
    fn builds_keep_geometry_and_reveal_related_content_only_when_complete() {
        let player = player_with_builds();
        let before = (*player.sheet).clone();
        let order = Playback::topic_order(&player.sheet, &player.layout, 0);
        let mut playback = Playback {
            build_topics: true,
            step: 1,
            ..Default::default()
        };
        let scene = playback.scene(&player.sheet, &player.layout, 0).unwrap();
        assert!(order.len() >= 3);
        assert_eq!(scene.layout.order, vec![order[0]]);
        assert_eq!(scene.layout.rect(order[0]), player.layout.rect(order[0]));
        assert!(scene.layout.relationships.is_empty());
        assert!(scene.layout.connectors.is_empty());
        playback.step = 2;
        let scene = playback.scene(&player.sheet, &player.layout, 0).unwrap();
        assert_eq!(scene.layout.order, order[..2]);
        assert_eq!(scene.layout.connectors.len(), 1);
        assert!(scene.layout.relationships.is_empty());
        playback.step = usize::MAX;
        let complete = playback.scene(&player.sheet, &player.layout, 0).unwrap();
        assert_eq!(complete.layout.order, order);
        assert_eq!(*player.sheet, before);
    }

    #[test]
    fn navigation_is_bounded_and_build_steps_are_reversible() {
        let player = player_with_builds();
        let mut playback = Playback::default();
        assert!(playback.reduce_motion);
        let mut selected = 0;
        playback.navigate(&player.sheet, &player.layout, &mut selected, false);
        assert_eq!(selected, 0);
        playback.navigate(&player.sheet, &player.layout, &mut selected, true);
        assert_eq!(selected, 1);
        playback.build_topics = true;
        playback.step = 1;
        let steps = Playback::topic_order(&player.sheet, &player.layout, selected)
            .len()
            .max(1);
        for expected in 2..=steps {
            playback.navigate(&player.sheet, &player.layout, &mut selected, true);
            assert_eq!(playback.step, expected);
            assert_eq!(selected, 1);
        }
        assert!(steps >= 3);
        playback.navigate(&player.sheet, &player.layout, &mut selected, false);
        assert_eq!(selected, 1);
        assert_eq!(playback.step, steps - 1);
        playback.navigate(&player.sheet, &player.layout, &mut selected, true);
        assert_eq!(playback.step, steps);
        playback.navigate(&player.sheet, &player.layout, &mut selected, true);
        assert_eq!(selected, 2);
        playback.step = usize::MAX;
        playback.navigate(&player.sheet, &player.layout, &mut selected, true);
        assert_eq!(selected, 2);
    }

    #[test]
    fn all_transitions_finish_and_motion_off_discards_the_outgoing_scene() {
        let mut app = super::super::tests::setup(true);
        app.open_pitch(&Context::default(), false);
        let player = app.pitch.player.take().unwrap();
        for transition in Transition::ALL {
            for dark in [false, true] {
                for reduce_motion in [false, true] {
                    let ctx = Context::default();
                    let mut playback = Playback {
                        transition,
                        reduce_motion,
                        ..Default::default()
                    };
                    let before = (*player.sheet).clone();
                    for (time, index) in [(0.0, 0), (1.0, 1), (1.175, 1), (1.5, 1)] {
                        let input = egui::RawInput {
                            time: Some(time),
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(360.0, 400.0),
                            )),
                            ..Default::default()
                        };
                        let mut output = ctx.run_ui(input, |ui| {
                            playback.show(
                                ui,
                                &player.sheet,
                                &player.layout,
                                index,
                                egui::vec2(340.0, 300.0),
                                dark,
                            );
                        });
                        assert!(!output.shapes.is_empty());
                        output.textures_delta.clear();
                        assert_eq!(playback.current.as_ref().unwrap().index, index);
                        if reduce_motion || transition == Transition::None || time == 1.5 {
                            assert!(playback.outgoing.is_none());
                        } else if time >= 1.0 {
                            assert!(playback.outgoing.is_some());
                        }
                    }
                    assert_eq!(*player.sheet, before);
                }
            }
        }
    }
}
