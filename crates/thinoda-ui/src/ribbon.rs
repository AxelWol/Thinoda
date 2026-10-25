//! Original, font-independent line icons and adaptive ribbon command controls.

use egui::{Align2, FontId, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};

use crate::{Palette, widgets};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Glyph {
    File,
    Folder,
    Save,
    Clipboard,
    Copy,
    Cut,
    Undo,
    Redo,
    Topic,
    Child,
    Edit,
    Up,
    Down,
    Indent,
    Outdent,
    Delete,
    Link,
    Format,
    Layout,
    Search,
    Print,
    Export,
    Settings,
    Help,
    Spark,
    Sheet,
    Zoom,
    ZoomOut,
    ZoomReset,
    Fit,
    Boundary,
    Summary,
    Todo,
    Tag,
    Notes,
    Outline,
    StatusBar,
    Symbols,
    Keyboard,
    Book,
    Info,
    Message,
    News,
    Merge,
    Mask,
    Relationship,
    Template,
    History,
    Play,
    Chevron,
    Camera,
    Close,
    Power,
    Home,
    Pin,
    Window,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Badge {
    Plus,
    Minus,
    Pencil,
    Stack,
    Left,
    Right,
    Up,
    Down,
    Reset,
    Check,
    Root,
    Branch,
    Before,
    Parent,
    Floating,
    SelectAll,
    SelectNone,
    Invert,
    AlignLeft,
    AlignCenter,
    AlignRight,
    AlignTop,
    AlignMiddle,
    AlignBottom,
    DistributeHorizontal,
    DistributeVertical,
    Expand,
    Collapse,
    Eye,
    Spark,
    Save,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CommandIcon {
    pub glyph: Glyph,
    pub badge: Option<Badge>,
    pub tone: IconTone,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IconTone {
    Neutral,
    File,
    Edit,
    Create,
    Format,
    Ai,
    View,
    Help,
    Danger,
}

impl IconTone {
    pub fn color(self, dark: bool, primary: bool) -> egui::Color32 {
        let (light_primary, light_secondary, dark_primary, dark_secondary) = match self {
            Self::Neutral | Self::View => (0x236baa, 0x526478, 0x86bcf1, 0xb0bdcf),
            Self::File => (0x005ea6, 0x00716b, 0x73b8ff, 0x56d7cc),
            Self::Edit => (0x4f46b9, 0x2569a8, 0xafa4ff, 0x7eb6ef),
            Self::Create => (0x237140, 0x007a76, 0x79d79a, 0x5bcfc8),
            Self::Format => (0x8040a5, 0xa13c75, 0xd8a1f3, 0xeca0cb),
            Self::Ai => (0x703bd0, 0x007576, 0xcba8ff, 0x5ed6d1),
            Self::Help => (0x00757e, 0x2b68a7, 0x6dd2d6, 0x91bbe8),
            Self::Danger => (0xb42330, 0xa54a00, 0xff8e99, 0xffc086),
        };
        let rgb = match (dark, primary) {
            (false, true) => light_primary,
            (false, false) => light_secondary,
            (true, true) => dark_primary,
            (true, false) => dark_secondary,
        };
        egui::Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
    }
}

impl From<Glyph> for CommandIcon {
    fn from(glyph: Glyph) -> Self {
        Self {
            glyph,
            badge: None,
            tone: IconTone::Neutral,
        }
    }
}

impl CommandIcon {
    pub const fn marked(glyph: Glyph, badge: Badge) -> Self {
        Self {
            glyph,
            badge: Some(badge),
            tone: IconTone::Neutral,
        }
    }

    pub const fn with_tone(mut self, tone: IconTone) -> Self {
        self.tone = tone;
        self
    }

    pub fn paint(self, ui: &Ui, rect: Rect, color: egui::Color32) {
        let Some(badge) = self.badge else {
            self.glyph.paint(ui, rect, color);
            return;
        };
        self.glyph
            .paint(ui, Rect::from_min_size(rect.min, rect.size() * 0.72), color);
        let badge_rect = Rect::from_min_max(rect.min + rect.size() * 0.52, rect.max);
        let painter = ui.painter();
        painter.rect_filled(badge_rect.expand(0.5), 1, Palette::of_ui(ui).surface);
        let point = |x: f32, y: f32| badge_rect.min + badge_rect.size() * vec2(x, y) / 10.0;
        let stroke = Stroke::new((rect.width() / 20.0).clamp(1.0, 1.6), color);
        let line = |points: &[(f32, f32)]| {
            painter.add(egui::Shape::line(
                points.iter().map(|&(x, y)| point(x, y)).collect(),
                stroke,
            ));
        };
        match badge {
            Badge::Plus | Badge::Minus => {
                line(&[(1., 5.), (9., 5.)]);
                if badge == Badge::Plus {
                    line(&[(5., 1.), (5., 9.)]);
                }
            }
            Badge::Pencil => line(&[(1., 9.), (2., 6.), (7., 1.), (9., 3.), (4., 8.), (1., 9.)]),
            Badge::Stack => {
                line(&[(1., 2.), (7., 2.), (7., 8.), (1., 8.), (1., 2.)]);
                line(&[(4., 0.), (10., 0.), (10., 6.)]);
            }
            Badge::Left | Badge::Right | Badge::Up | Badge::Down => {
                let transform = |x, y| match badge {
                    Badge::Left => (10. - x, y),
                    Badge::Up => (y, 10. - x),
                    Badge::Down => (y, x),
                    _ => (x, y),
                };
                let points: Vec<_> = [(1., 5.), (9., 5.), (5., 1.), (9., 5.), (5., 9.)]
                    .into_iter()
                    .map(|(x, y)| transform(x, y))
                    .collect();
                line(&points);
            }
            Badge::Reset => line(&[
                (2., 4.),
                (4., 1.),
                (8., 2.),
                (9., 6.),
                (6., 9.),
                (2., 8.),
                (2., 4.),
                (0., 5.),
                (2., 4.),
                (4., 5.),
            ]),
            Badge::Check => line(&[(1., 5.), (4., 8.), (9., 1.)]),
            Badge::Root => {
                painter.circle_stroke(point(5., 3.), badge_rect.width() * 0.22, stroke);
                line(&[(5., 5.), (5., 9.), (1., 9.), (9., 9.)]);
            }
            Badge::Branch => line(&[(1., 1.), (1., 8.), (8., 8.), (1., 8.), (1., 4.), (8., 4.)]),
            Badge::Before => line(&[
                (1., 1.),
                (9., 1.),
                (5., 2.),
                (5., 9.),
                (2., 6.),
                (5., 9.),
                (8., 6.),
            ]),
            Badge::Parent => line(&[
                (1., 9.),
                (1., 6.),
                (9., 6.),
                (9., 9.),
                (5., 6.),
                (5., 1.),
                (2., 4.),
                (5., 1.),
                (8., 4.),
            ]),
            Badge::Floating => {
                painter.circle_stroke(point(5., 5.), badge_rect.width() * 0.25, stroke);
                for (a, b) in [
                    ((0., 0.), (2., 2.)),
                    ((8., 8.), (10., 10.)),
                    ((0., 10.), (2., 8.)),
                    ((8., 2.), (10., 0.)),
                ] {
                    line(&[a, b]);
                }
            }
            Badge::SelectAll | Badge::SelectNone | Badge::Invert => {
                line(&[(1., 1.), (9., 1.), (9., 9.), (1., 9.), (1., 1.)]);
                match badge {
                    Badge::SelectAll => line(&[(2., 5.), (4., 7.), (8., 3.)]),
                    Badge::SelectNone => line(&[(2., 8.), (8., 2.)]),
                    _ => {
                        painter.rect_filled(
                            Rect::from_min_max(point(1., 1.), point(5., 9.)),
                            0,
                            color,
                        );
                    }
                }
            }
            Badge::AlignLeft | Badge::AlignCenter | Badge::AlignRight => {
                let x = match badge {
                    Badge::AlignLeft => 1.,
                    Badge::AlignCenter => 5.,
                    _ => 9.,
                };
                line(&[(x, 0.), (x, 10.)]);
                line(&[(2., 3.), (8., 3.)]);
                let start = match badge {
                    Badge::AlignLeft => 2.,
                    Badge::AlignCenter => 4.,
                    _ => 6.,
                };
                line(&[(start, 7.), (start + 2., 7.)]);
            }
            Badge::AlignTop | Badge::AlignMiddle | Badge::AlignBottom => {
                let y = match badge {
                    Badge::AlignTop => 1.,
                    Badge::AlignMiddle => 5.,
                    _ => 9.,
                };
                line(&[(0., y), (10., y)]);
                line(&[(3., 2.), (3., 8.)]);
                let start = match badge {
                    Badge::AlignTop => 2.,
                    Badge::AlignMiddle => 4.,
                    _ => 6.,
                };
                line(&[(7., start), (7., start + 2.)]);
            }
            Badge::DistributeHorizontal => {
                for x in [1., 5., 9.] {
                    line(&[(x, 1.), (x, 9.)]);
                }
                line(&[(1., 5.), (9., 5.)]);
            }
            Badge::DistributeVertical => {
                for y in [1., 5., 9.] {
                    line(&[(1., y), (9., y)]);
                }
                line(&[(5., 1.), (5., 9.)]);
            }
            Badge::Expand | Badge::Collapse => {
                let offset = if badge == Badge::Expand { 0. } else { 4. };
                line(&[(1., 3. + offset), (5., 7. - offset), (9., 3. + offset)]);
            }
            Badge::Eye => {
                line(&[
                    (0., 5.),
                    (3., 2.),
                    (7., 2.),
                    (10., 5.),
                    (7., 8.),
                    (3., 8.),
                    (0., 5.),
                ]);
                painter.circle_filled(point(5., 5.), badge_rect.width() * 0.12, color);
            }
            Badge::Spark => line(&[
                (5., 0.),
                (6.5, 3.5),
                (10., 5.),
                (6.5, 6.5),
                (5., 10.),
                (3.5, 6.5),
                (0., 5.),
                (3.5, 3.5),
                (5., 0.),
            ]),
            Badge::Save => {
                line(&[(1., 1.), (8., 1.), (9., 2.), (9., 9.), (1., 9.), (1., 1.)]);
                line(&[(3., 1.), (3., 4.), (7., 4.), (7., 1.)]);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detail {
    Large,
    Labeled,
    Icon,
    CompactLarge,
    CompactLabeled,
    CompactIcon,
}

impl Detail {
    pub fn compact(self) -> Self {
        match self {
            Self::Large | Self::CompactLarge => Self::CompactLarge,
            Self::Labeled | Self::CompactLabeled => Self::CompactLabeled,
            Self::Icon | Self::CompactIcon => Self::CompactIcon,
        }
    }

    fn is_compact(self) -> bool {
        matches!(
            self,
            Self::CompactLarge | Self::CompactLabeled | Self::CompactIcon
        )
    }

    fn icon_size(self) -> f32 {
        match self {
            Self::Large => 32.0,
            Self::Labeled => 24.0,
            Self::Icon => 16.0,
            Self::CompactLarge => 24.0,
            Self::CompactLabeled => 20.0,
            Self::CompactIcon => 16.0,
        }
    }
}

impl Glyph {
    pub fn paint(self, ui: &Ui, rect: Rect, color: egui::Color32) {
        let painter = ui.painter();
        let point = |x: f32, y: f32| {
            pos2(
                rect.left() + rect.width() * x / 24.0,
                rect.top() + rect.height() * y / 24.0,
            )
        };
        let stroke = Stroke::new((rect.width() / 16.0).clamp(1.2, 1.8), color);
        let line = |points: &[(f32, f32)]| {
            painter.add(egui::Shape::line(
                points.iter().map(|&(x, y)| point(x, y)).collect(),
                stroke,
            ));
        };
        let box_at = |x: f32, y: f32, w: f32, h: f32| {
            painter.rect_stroke(
                Rect::from_min_max(point(x, y), point(x + w, y + h)),
                2,
                stroke,
                StrokeKind::Inside,
            );
        };
        match self {
            Self::File | Self::Sheet => {
                line(&[
                    (5., 2.),
                    (15., 2.),
                    (20., 7.),
                    (20., 22.),
                    (5., 22.),
                    (5., 2.),
                ]);
                line(&[(15., 2.), (15., 8.), (20., 8.)]);
                line(&[(8., 13.), (16., 13.)]);
                line(&[(8., 17.), (14., 17.)]);
            }
            Self::Window => {
                box_at(2., 3., 20., 18.);
                line(&[(2., 8.), (22., 8.)]);
                line(&[(6., 5.5), (6.1, 5.5)]);
                line(&[(9., 5.5), (9.1, 5.5)]);
            }
            Self::Folder => {
                line(&[
                    (3., 20.),
                    (3., 5.),
                    (10., 5.),
                    (12., 8.),
                    (21., 8.),
                    (21., 20.),
                    (3., 20.),
                ]);
                line(&[(3., 11.), (21., 11.)]);
            }
            Self::Save => {
                box_at(3., 3., 18., 18.);
                box_at(7., 3., 9., 6.);
                box_at(7., 14., 10., 7.);
            }
            Self::Copy => {
                box_at(3., 3., 13., 15.);
                box_at(8., 7., 13., 15.);
            }
            Self::Clipboard => {
                box_at(5., 4., 15., 18.);
                box_at(8., 2., 9., 5.);
                line(&[(9., 12.), (16., 12.)]);
                line(&[(9., 16.), (15., 16.)]);
            }
            Self::Cut => {
                painter.circle_stroke(point(6., 18.), rect.width() * 0.13, stroke);
                painter.circle_stroke(point(18., 18.), rect.width() * 0.13, stroke);
                line(&[(8., 15.), (19., 2.)]);
                line(&[(16., 15.), (5., 2.)]);
            }
            Self::Undo | Self::Redo => {
                let mirror = |x| if self == Self::Redo { 24. - x } else { x };
                let points: Vec<_> = [
                    (8., 4.),
                    (3., 9.),
                    (8., 14.),
                    (3., 9.),
                    (14., 9.),
                    (20., 12.),
                    (20., 18.),
                ]
                .into_iter()
                .map(|(x, y)| (mirror(x), y))
                .collect();
                line(&points);
            }
            Self::Layout => {
                for (x, y) in [(2., 2.), (14., 2.), (2., 14.), (14., 14.)] {
                    box_at(x, y, 8., 8.);
                }
            }
            Self::Topic | Self::Child => {
                box_at(2., 8., 8., 8.);
                box_at(16., 2., 7., 6.);
                box_at(16., 16., 7., 6.);
                line(&[(10., 12.), (13., 12.), (13., 5.), (16., 5.)]);
                line(&[(13., 12.), (13., 19.), (16., 19.)]);
                if self == Self::Child {
                    line(&[(17., 11.), (23., 11.)]);
                    line(&[(20., 8.), (20., 14.)]);
                }
            }
            Self::Edit => {
                line(&[
                    (4., 20.),
                    (6., 14.),
                    (17., 3.),
                    (21., 7.),
                    (10., 18.),
                    (4., 20.),
                ]);
                line(&[(14., 6.), (18., 10.)]);
            }
            Self::Up | Self::Down => {
                let y = |y| if self == Self::Down { 24. - y } else { y };
                line(&[(5., y(10.)), (12., y(3.)), (19., y(10.))]);
                line(&[(12., y(3.)), (12., y(21.))]);
            }
            Self::Indent | Self::Outdent => {
                line(&[(3., 4.), (21., 4.)]);
                line(&[(11., 9.), (21., 9.)]);
                line(&[(11., 14.), (21., 14.)]);
                line(&[(3., 20.), (21., 20.)]);
                if self == Self::Indent {
                    line(&[(3., 8.), (7., 12.), (3., 16.)]);
                } else {
                    line(&[(7., 8.), (3., 12.), (7., 16.)]);
                }
            }
            Self::Delete => {
                line(&[(3., 6.), (21., 6.)]);
                box_at(6., 6., 12., 15.);
                line(&[(9., 3.), (15., 3.)]);
                line(&[(10., 10.), (10., 17.)]);
                line(&[(14., 10.), (14., 17.)]);
            }
            Self::Link => {
                box_at(2., 5., 13., 8.);
                box_at(9., 11., 13., 8.);
                line(&[(8., 9.), (16., 15.)]);
            }
            Self::Format => {
                line(&[(4., 20.), (12., 3.), (20., 20.)]);
                line(&[(7., 14.), (17., 14.)]);
                line(&[(3., 23.), (21., 23.)]);
            }
            Self::Search | Self::Zoom | Self::ZoomOut | Self::ZoomReset => {
                painter.circle_stroke(point(10., 10.), rect.width() * 0.31, stroke);
                line(&[(15., 15.), (22., 22.)]);
                if matches!(self, Self::Zoom | Self::ZoomOut) {
                    line(&[(6., 10.), (14., 10.)]);
                }
                if self == Self::Zoom {
                    line(&[(10., 6.), (10., 14.)]);
                }
                if self == Self::ZoomReset {
                    line(&[(8., 8.), (10., 6.), (10., 14.)]);
                    line(&[(7., 14.), (13., 14.)]);
                }
            }
            Self::Fit => {
                for points in [
                    [(2., 8.), (2., 2.), (8., 2.)],
                    [(16., 2.), (22., 2.), (22., 8.)],
                    [(2., 16.), (2., 22.), (8., 22.)],
                    [(16., 22.), (22., 22.), (22., 16.)],
                ] {
                    line(&points);
                }
                box_at(7., 7., 10., 10.);
            }
            Self::Boundary => {
                for points in [
                    [(2., 8.), (2., 2.), (8., 2.)],
                    [(16., 2.), (22., 2.), (22., 8.)],
                    [(2., 16.), (2., 22.), (8., 22.)],
                    [(16., 22.), (22., 22.), (22., 16.)],
                ] {
                    line(&points);
                }
                box_at(6., 9., 5., 6.);
                box_at(14., 9., 5., 6.);
            }
            Self::Summary => {
                line(&[
                    (3., 3.),
                    (9., 3.),
                    (9., 10.),
                    (13., 12.),
                    (9., 14.),
                    (9., 21.),
                    (3., 21.),
                ]);
                line(&[(13., 12.), (22., 12.)]);
            }
            Self::Todo => {
                box_at(3., 3., 18., 18.);
                line(&[(6., 12.), (10., 16.), (18., 7.)]);
            }
            Self::Tag => {
                line(&[
                    (2., 3.),
                    (12., 3.),
                    (22., 13.),
                    (12., 23.),
                    (2., 13.),
                    (2., 3.),
                ]);
                painter.circle_stroke(point(7., 8.), rect.width() * 0.07, stroke);
            }
            Self::Notes => {
                box_at(4., 2., 16., 20.);
                for y in [7., 12., 17.] {
                    line(&[(8., y), (16., y)]);
                }
            }
            Self::Outline => {
                for (y, x) in [(4., 3.), (9., 8.), (14., 8.), (20., 3.)] {
                    line(&[(x, y), (21., y)]);
                }
            }
            Self::StatusBar => {
                box_at(2., 3., 20., 18.);
                line(&[(2., 16.), (22., 16.)]);
                line(&[(5., 19.), (11., 19.)]);
            }
            Self::Symbols => {
                painter.circle_stroke(point(7., 7.), rect.width() * 0.18, stroke);
                box_at(14., 3., 7., 8.);
                line(&[(3., 21.), (7., 14.), (11., 21.), (3., 21.)]);
                line(&[(15., 18.), (22., 18.)]);
                line(&[(18.5, 14.5), (18.5, 21.5)]);
            }
            Self::Keyboard => {
                box_at(1., 5., 22., 14.);
                for y in [9., 13.] {
                    for x in [5., 9., 13., 17.] {
                        line(&[(x, y), (x + 1., y)]);
                    }
                }
                line(&[(7., 17.), (17., 17.)]);
            }
            Self::Book => {
                line(&[
                    (12., 5.),
                    (8., 3.),
                    (2., 3.),
                    (2., 20.),
                    (8., 20.),
                    (12., 22.),
                    (16., 20.),
                    (22., 20.),
                    (22., 3.),
                    (16., 3.),
                    (12., 5.),
                    (12., 22.),
                ]);
            }
            Self::Info => {
                painter.circle_stroke(point(12., 12.), rect.width() * 0.42, stroke);
                line(&[(12., 10.), (12., 18.)]);
                painter.circle_filled(point(12., 6.), rect.width() * 0.05, color);
            }
            Self::Message => {
                line(&[
                    (2., 3.),
                    (22., 3.),
                    (22., 17.),
                    (9., 17.),
                    (4., 22.),
                    (4., 17.),
                    (2., 17.),
                    (2., 3.),
                ]);
                line(&[(6., 8.), (18., 8.)]);
                line(&[(6., 12.), (15., 12.)]);
            }
            Self::News => {
                box_at(2., 3., 20., 18.);
                box_at(5., 6., 6., 6.);
                line(&[(14., 6.), (19., 6.)]);
                line(&[(14., 11.), (19., 11.)]);
                line(&[(5., 16.), (19., 16.)]);
            }
            Self::Merge => {
                line(&[
                    (2., 3.),
                    (8., 3.),
                    (14., 12.),
                    (22., 12.),
                    (18., 8.),
                    (22., 12.),
                    (18., 16.),
                ]);
                line(&[(2., 21.), (8., 21.), (14., 12.)]);
            }
            Self::Mask => {
                line(&[
                    (2., 6.),
                    (12., 9.),
                    (22., 6.),
                    (21., 16.),
                    (16., 21.),
                    (8., 21.),
                    (3., 16.),
                    (2., 6.),
                ]);
                line(&[(6., 13.), (9., 14.)]);
                line(&[(15., 14.), (18., 13.)]);
            }
            Self::Relationship => {
                box_at(1., 3., 6., 6.);
                box_at(17., 15., 6., 6.);
                line(&[
                    (7., 6.),
                    (13., 6.),
                    (13., 18.),
                    (17., 18.),
                    (14., 15.),
                    (17., 18.),
                    (14., 21.),
                ]);
            }
            Self::Template => {
                box_at(3., 2., 18., 20.);
                line(&[(3., 8.), (21., 8.)]);
                line(&[(10., 8.), (10., 22.)]);
                line(&[(10., 15.), (21., 15.)]);
            }
            Self::History => {
                painter.circle_stroke(point(12., 12.), rect.width() * 0.4, stroke);
                line(&[(12., 5.), (12., 12.), (17., 15.)]);
            }
            Self::Print => {
                box_at(3., 8., 18., 10.);
                box_at(7., 2., 10., 7.);
                box_at(7., 14., 10., 8.);
            }
            Self::Export => {
                line(&[(3., 4.), (3., 21.), (20., 21.), (20., 16.)]);
                line(&[(8., 15.), (21., 3.), (21., 11.), (21., 3.), (13., 3.)]);
            }
            Self::Settings => {
                painter.circle_stroke(point(12., 12.), rect.width() * 0.31, stroke);
                painter.circle_stroke(point(12., 12.), rect.width() * 0.12, stroke);
                for (x, y, xx, yy) in [
                    (12., 1., 12., 4.),
                    (12., 20., 12., 23.),
                    (1., 12., 4., 12.),
                    (20., 12., 23., 12.),
                    (4., 4., 6., 6.),
                    (18., 18., 20., 20.),
                    (4., 20., 6., 18.),
                    (18., 6., 20., 4.),
                ] {
                    line(&[(x, y), (xx, yy)]);
                }
            }
            Self::Help => {
                painter.circle_stroke(point(12., 12.), rect.width() * 0.43, stroke);
                line(&[
                    (8., 8.),
                    (10., 5.),
                    (15., 5.),
                    (17., 8.),
                    (12., 13.),
                    (12., 15.),
                ]);
                painter.circle_filled(point(12., 19.), 1., color);
            }
            Self::Spark => {
                line(&[
                    (12., 2.),
                    (15., 9.),
                    (22., 12.),
                    (15., 15.),
                    (12., 22.),
                    (9., 15.),
                    (2., 12.),
                    (9., 9.),
                    (12., 2.),
                ]);
            }
            Self::Play => {
                line(&[(7., 3.), (21., 12.), (7., 21.), (7., 3.)]);
            }
            Self::Pin => {
                line(&[
                    (8., 3.),
                    (18., 3.),
                    (16., 10.),
                    (20., 14.),
                    (5., 14.),
                    (9., 10.),
                    (8., 3.),
                ]);
                line(&[(12., 14.), (12., 22.)]);
            }
            Self::Chevron => {
                line(&[(5., 8.), (12., 16.), (19., 8.)]);
            }
            Self::Camera => {
                line(&[
                    (2., 7.),
                    (7., 7.),
                    (9., 4.),
                    (15., 4.),
                    (17., 7.),
                    (22., 7.),
                    (22., 21.),
                    (2., 21.),
                    (2., 7.),
                ]);
                painter.circle_stroke(point(12., 14.), rect.width() * 0.2, stroke);
            }
            Self::Close => {
                line(&[(5., 5.), (19., 19.)]);
                line(&[(19., 5.), (5., 19.)]);
            }
            Self::Power => {
                line(&[(12., 2.), (12., 13.)]);
                line(&[
                    (6., 5.),
                    (3., 10.),
                    (3., 16.),
                    (7., 21.),
                    (17., 21.),
                    (21., 16.),
                    (21., 10.),
                    (18., 5.),
                ]);
            }
            Self::Home => {
                line(&[
                    (2., 11.),
                    (12., 2.),
                    (22., 11.),
                    (19., 11.),
                    (19., 22.),
                    (5., 22.),
                    (5., 11.),
                    (2., 11.),
                ]);
                box_at(10., 15., 5., 7.);
            }
        }
    }
}

pub fn command_button(
    ui: &mut Ui,
    icon: impl Into<CommandIcon>,
    label: &str,
    detail: Detail,
    selected: bool,
    tooltip: Option<&str>,
) -> (Response, bool) {
    button(ui, icon.into(), label, detail, selected, tooltip, false)
}

pub fn menu_button(
    ui: &mut Ui,
    icon: impl Into<CommandIcon>,
    label: &str,
    detail: Detail,
    tooltip: Option<&str>,
) -> (Response, bool) {
    button(ui, icon.into(), label, detail, false, tooltip, true)
}

fn button(
    ui: &mut Ui,
    icon: CommandIcon,
    label: &str,
    detail: Detail,
    selected: bool,
    tooltip: Option<&str>,
    dropdown: bool,
) -> (Response, bool) {
    let touch_allowed = widgets::touch_activation_allowed(ui.ctx());
    let palette = Palette::of_ui(ui);
    let size = match detail {
        Detail::Large => vec2(112.0, 96.0),
        Detail::Labeled => vec2(ui.available_width().clamp(112.0, 260.0), 44.0),
        Detail::Icon => vec2(44.0, 44.0),
        Detail::CompactLarge => vec2(104.0, 72.0),
        Detail::CompactLabeled => vec2(ui.available_width().clamp(104.0, 240.0), 34.0),
        Detail::CompactIcon => vec2(34.0, 34.0),
    };
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), selected, label)
    });
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact_selectable(&response, selected);
        let fill = if selected {
            palette.accent_soft
        } else if response.hovered() || response.is_pointer_button_down_on() {
            visuals.bg_fill
        } else {
            egui::Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, 4, fill);
        if response.has_focus() && ui.is_enabled() {
            ui.painter().rect_stroke(
                rect.shrink(1.0),
                4,
                Stroke::new(2.0, palette.text),
                StrokeKind::Inside,
            );
        }
        let color = if ui.is_enabled() {
            palette.text
        } else {
            ui.visuals().weak_text_color()
        };
        let icon_size = detail.icon_size();
        let center = match detail {
            Detail::Large => pos2(rect.center().x, rect.top() + 26.0),
            Detail::Labeled => pos2(rect.left() + 20.0, rect.center().y),
            Detail::Icon => rect.center(),
            Detail::CompactLarge => pos2(rect.center().x, rect.top() + 18.0),
            Detail::CompactLabeled => pos2(rect.left() + 16.0, rect.center().y),
            Detail::CompactIcon => rect.center(),
        };
        let icon_color = if ui.is_enabled() {
            icon.tone.color(
                palette.dark,
                matches!(detail, Detail::Large | Detail::CompactLarge),
            )
        } else {
            ui.visuals().weak_text_color()
        };
        icon.paint(
            ui,
            Rect::from_center_size(center, Vec2::splat(icon_size)),
            icon_color,
        );
        if !matches!(detail, Detail::Icon | Detail::CompactIcon) {
            let text_rect = match detail {
                Detail::Large => Rect::from_min_max(
                    pos2(rect.left() + 8., rect.top() + 48.),
                    pos2(
                        rect.right() - if dropdown { 24. } else { 8. },
                        rect.bottom() - 8.,
                    ),
                ),
                Detail::CompactLarge => Rect::from_min_max(
                    pos2(rect.left() + 6., rect.top() + 34.),
                    pos2(
                        rect.right() - if dropdown { 24. } else { 6. },
                        rect.bottom() - 4.,
                    ),
                ),
                _ => Rect::from_min_max(
                    pos2(
                        rect.left() + if detail.is_compact() { 32. } else { 40. },
                        rect.top() + if detail.is_compact() { 4. } else { 8. },
                    ),
                    pos2(
                        rect.right() - if dropdown { 28. } else { 8. },
                        rect.bottom() - if detail.is_compact() { 4. } else { 8. },
                    ),
                ),
            };
            let mut job = egui::text::LayoutJob::simple(
                label.to_owned(),
                FontId::proportional(12.0),
                color,
                text_rect.width(),
            );
            job.wrap.max_rows = if matches!(detail, Detail::Large | Detail::CompactLarge) {
                2
            } else {
                1
            };
            let galley = ui.painter().layout_job(job);
            let anchor = if matches!(detail, Detail::Large | Detail::CompactLarge) {
                Align2::CENTER_TOP
            } else {
                Align2::LEFT_CENTER
            };
            let origin: Pos2 = anchor.align_size_within_rect(galley.size(), text_rect).min;
            if dropdown {
                Glyph::Chevron.paint(
                    ui,
                    Rect::from_center_size(
                        pos2(rect.right() - 14., origin.y + galley.size().y * 0.5),
                        Vec2::splat(13.0),
                    ),
                    color,
                );
            }
            ui.painter()
                .with_clip_rect(text_rect.intersect(ui.clip_rect()))
                .galley(origin, galley, color);
        } else if dropdown {
            Glyph::Chevron.paint(
                ui,
                Rect::from_center_size(pos2(rect.right() - 7., rect.center().y), Vec2::splat(13.0)),
                color,
            );
        }
    }
    let response = response.on_hover_text(tooltip.unwrap_or(label));
    let keyboard = ui.is_enabled()
        && response.has_focus()
        && ui.input_mut(|input| {
            input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                || input.consume_key(egui::Modifiers::NONE, egui::Key::Space)
        });
    let activated = ui.is_enabled() && ((response.clicked() && touch_allowed) || keyboard);
    (response, activated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::accesskit::Role;
    use egui_kittest::{
        Harness,
        kittest::{NodeT, Queryable},
    };

    #[test]
    fn semantic_colors_differ_by_priority_and_contrast_in_both_themes() {
        fn luminance(color: egui::Color32) -> f32 {
            let channel = |value: u8| {
                let value = value as f32 / 255.0;
                if value <= 0.04045 {
                    value / 12.92
                } else {
                    ((value + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
        }
        for dark in [false, true] {
            let palette = if dark {
                Palette::dark(crate::tokens::DEFAULT_ACCENT)
            } else {
                Palette::light(crate::tokens::DEFAULT_ACCENT)
            };
            for tone in [
                IconTone::Neutral,
                IconTone::File,
                IconTone::Edit,
                IconTone::Create,
                IconTone::Format,
                IconTone::Ai,
                IconTone::View,
                IconTone::Help,
                IconTone::Danger,
            ] {
                assert_ne!(tone.color(dark, true), tone.color(dark, false));
                for primary in [false, true] {
                    for background in [palette.surface, palette.surface_alt, palette.accent_soft] {
                        let foreground = luminance(tone.color(dark, primary));
                        let alpha = background.a() as f32 / 255.0;
                        let composite = |front: u8, back: u8| {
                            (front as f32 + back as f32 * (1.0 - alpha))
                                .round()
                                .min(255.0) as u8
                        };
                        let background = luminance(egui::Color32::from_rgb(
                            composite(background.r(), palette.surface.r()),
                            composite(background.g(), palette.surface.g()),
                            composite(background.b(), palette.surface.b()),
                        ));
                        let contrast = (foreground.max(background) + 0.05)
                            / (foreground.min(background) + 0.05);
                        assert!(
                            contrast >= 3.0,
                            "{tone:?}, dark={dark}, primary={primary}: {contrast}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn dropdown_chevrons_align_with_their_caption() {
        for detail in [
            Detail::Large,
            Detail::Labeled,
            Detail::CompactLarge,
            Detail::CompactLabeled,
        ] {
            let mut h = Harness::builder()
                .with_size(vec2(300., 180.))
                .build_ui(move |ui| {
                    menu_button(ui, Glyph::Folder, "Open Recent", detail, None);
                });
            h.run_steps(3);
            let rect = h.get_by_role_and_label(Role::Button, "Open Recent").rect();
            let text = h
                .output()
                .shapes
                .iter()
                .find_map(|shape| {
                    if let egui::Shape::Text(text) = &shape.shape {
                        Some(text)
                    } else {
                        None
                    }
                })
                .unwrap();
            let arrow = h
                .output()
                .shapes
                .iter()
                .find_map(|shape| {
                    if let egui::Shape::Path(path) = &shape.shape
                        && path.points.len() == 3
                        && path.points.iter().all(|point| point.x > rect.right() - 22.)
                    {
                        return Some(Rect::from_points(&path.points));
                    }
                    None
                })
                .expect("dropdown has one caption-adjacent chevron");
            assert!((arrow.center().y - (text.pos.y + text.galley.size().y * 0.5)).abs() < 0.1);
            assert!(arrow.left() >= text.pos.x + text.galley.size().x);
        }
    }

    #[test]
    fn command_icons_use_consistent_priority_tiers_and_touch_targets() {
        let mut sizes = Vec::new();
        for detail in [
            Detail::Large,
            Detail::Labeled,
            Detail::Icon,
            Detail::CompactLarge,
            Detail::CompactLabeled,
            Detail::CompactIcon,
        ] {
            let mut h = Harness::builder()
                .with_size(vec2(300., 160.))
                .build_ui(move |ui| {
                    command_button(ui, Glyph::Folder, "Open", detail, false, None);
                });
            h.run_steps(3);
            let rect = h.get_by_role_and_label(Role::Button, "Open").rect();
            let minimum = if detail.is_compact() { 34.0 } else { 44.0 };
            assert!(rect.width() >= minimum && rect.height() >= minimum);
            let bounds = h
                .output()
                .shapes
                .iter()
                .filter(|shape| matches!(shape.shape, egui::Shape::Path(_)))
                .fold(Rect::NOTHING, |mut bounds, shape| {
                    if let egui::Shape::Path(path) = &shape.shape {
                        for point in &path.points {
                            bounds.extend_with(*point);
                        }
                    }
                    bounds
                });
            assert!(bounds.is_positive());
            sizes.push(bounds.size() / detail.icon_size());
        }
        for size in &sizes[1..] {
            assert!((*size - sizes[0]).length() < 0.01, "{sizes:?}");
        }
    }

    #[test]
    fn every_detail_preserves_names_keyboard_activation_and_disabled_state() {
        for detail in [
            Detail::Large,
            Detail::Labeled,
            Detail::Icon,
            Detail::CompactLarge,
            Detail::CompactLabeled,
            Detail::CompactIcon,
        ] {
            let mut h = Harness::builder()
                .with_size(vec2(300., 160.))
                .build_ui_state(
                    move |ui, state: &mut (bool, usize)| {
                        let (_, activated) = ui
                            .add_enabled_ui(state.0, |ui| {
                                command_button(
                                    ui,
                                    Glyph::Down,
                                    "Move selected topics down",
                                    detail,
                                    false,
                                    None,
                                )
                            })
                            .inner;
                        if activated {
                            state.1 += 1;
                        }
                    },
                    (true, 0),
                );
            h.run_steps(3);
            h.get_by_role_and_label(Role::Button, "Move selected topics down")
                .click();
            h.run_steps(3);
            assert_eq!(h.state().1, 1);
            h.get_by_role_and_label(Role::Button, "Move selected topics down")
                .focus();
            h.run_steps(2);
            h.key_press(egui::Key::Space);
            h.run_steps(2);
            h.key_press(egui::Key::Enter);
            h.run_steps(2);
            assert_eq!(h.state().1, 3);
            h.state_mut().0 = false;
            h.run_steps(3);
            assert!(
                h.get_by_role_and_label(Role::Button, "Move selected topics down")
                    .accesskit_node()
                    .is_disabled()
            );
            h.key_press(egui::Key::Enter);
            h.run_steps(3);
            assert_eq!(h.state().1, 3);
        }
    }

    #[test]
    fn canceled_touch_cannot_activate_a_ribbon_command() {
        let mut h = Harness::builder().build_ui_state(
            |ui, count: &mut usize| {
                if command_button(ui, Glyph::Delete, "Delete", Detail::Icon, false, None).1 {
                    *count += 1;
                }
            },
            0,
        );
        h.run_steps(3);
        let pos = h
            .get_by_role_and_label(Role::Button, "Delete")
            .rect()
            .center();
        h.event(egui::Event::PointerMoved(pos));
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        for phase in [egui::TouchPhase::Start, egui::TouchPhase::Cancel] {
            h.event(egui::Event::Touch {
                device_id: egui::TouchDeviceId(1),
                id: egui::TouchId(1),
                phase,
                pos,
                force: None,
            });
            h.run_steps(2);
        }
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(3);
        assert_eq!(*h.state(), 0);
        h.get_by_role_and_label(Role::Button, "Delete").click();
        h.run_steps(3);
        assert_eq!(*h.state(), 1);
    }
}
