//! Minimal geometry and color primitives (kept independent from any GUI toolkit).

use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, Mul, Sub};

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn length(self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x + o.x, self.y + o.y)
    }
}

impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x - o.x, self.y - o.y)
    }
}

impl Mul<f32> for Vec2 {
    type Output = Vec2;
    fn mul(self, s: f32) -> Vec2 {
        Vec2::new(self.x * s, self.y * s)
    }
}

/// Axis aligned rectangle described by its min corner and size.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Rect {
    pub min: Vec2,
    pub size: Vec2,
}

impl Rect {
    pub fn from_min_size(min: Vec2, size: Vec2) -> Self {
        Self { min, size }
    }

    pub fn from_center_size(center: Vec2, size: Vec2) -> Self {
        Self {
            min: Vec2::new(center.x - size.x / 2.0, center.y - size.y / 2.0),
            size,
        }
    }

    pub fn max(&self) -> Vec2 {
        self.min + self.size
    }

    pub fn center(&self) -> Vec2 {
        Vec2::new(
            self.min.x + self.size.x / 2.0,
            self.min.y + self.size.y / 2.0,
        )
    }

    pub fn left_center(&self) -> Vec2 {
        Vec2::new(self.min.x, self.center().y)
    }

    pub fn right_center(&self) -> Vec2 {
        Vec2::new(self.max().x, self.center().y)
    }

    pub fn top_center(&self) -> Vec2 {
        Vec2::new(self.center().x, self.min.y)
    }

    pub fn bottom_center(&self) -> Vec2 {
        Vec2::new(self.center().x, self.max().y)
    }

    pub fn contains(&self, p: Vec2) -> bool {
        let max = self.max();
        p.x >= self.min.x && p.x <= max.x && p.y >= self.min.y && p.y <= max.y
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        let a = self.max();
        let b = other.max();
        self.min.x < b.x && other.min.x < a.x && self.min.y < b.y && other.min.y < a.y
    }

    pub fn translate(&self, d: Vec2) -> Rect {
        Rect::from_min_size(self.min + d, self.size)
    }

    pub fn expand(&self, m: f32) -> Rect {
        Rect::from_min_size(
            Vec2::new(self.min.x - m, self.min.y - m),
            Vec2::new(self.size.x + 2.0 * m, self.size.y + 2.0 * m),
        )
    }

    pub fn union(&self, other: &Rect) -> Rect {
        let a = self.max();
        let b = other.max();
        let min = Vec2::new(self.min.x.min(other.min.x), self.min.y.min(other.min.y));
        let max = Vec2::new(a.x.max(b.x), a.y.max(b.y));
        Rect::from_min_size(min, max - min)
    }
}

/// sRGB color with alpha, serialized as `#RRGGBB` or `#RRGGBBAA`.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const WHITE: Color = Color::rgb(255, 255, 255);
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    pub const TRANSPARENT: Color = Color::rgba(0, 0, 0, 0);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Parses `#RGB`, `#RRGGBB` or `#RRGGBBAA` (leading `#` optional).
    pub fn from_hex(s: &str) -> Option<Self> {
        let s = s.trim().trim_start_matches('#');
        if !s.is_ascii() {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(s.get(i..i + 2)?, 16).ok();
        match s.len() {
            3 => {
                let nib = |i: usize| {
                    u8::from_str_radix(s.get(i..i + 1)?, 16)
                        .ok()
                        .map(|v| v * 17)
                };
                Some(Self::rgb(nib(0)?, nib(1)?, nib(2)?))
            }
            6 => Some(Self::rgb(byte(0)?, byte(2)?, byte(4)?)),
            8 => Some(Self::rgba(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
            _ => None,
        }
    }

    pub fn to_hex(self) -> String {
        if self.a == 255 {
            format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
        } else {
            format!("#{:02X}{:02X}{:02X}{:02X}", self.r, self.g, self.b, self.a)
        }
    }

    /// Relative luminance (0..1) used to pick readable text colors.
    pub fn luminance(self) -> f32 {
        fn ch(c: u8) -> f32 {
            let c = c as f32 / 255.0;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * ch(self.r) + 0.7152 * ch(self.g) + 0.0722 * ch(self.b)
    }

    /// Dark or white text, whichever contrasts better with `self`.
    pub fn contrasting_text(self) -> Color {
        if self.luminance() > 0.45 {
            Color::rgb(0x1F, 0x23, 0x28)
        } else {
            Color::WHITE
        }
    }

    /// Linear blend towards `other` by `t` (0..1).
    pub fn lerp(self, other: Color, t: f32) -> Color {
        let t = t.clamp(0.0, 1.0);
        let m = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Color::rgba(
            m(self.r, other.r),
            m(self.g, other.g),
            m(self.b, other.b),
            m(self.a, other.a),
        )
    }
}

impl fmt::Debug for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl Serialize for Color {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Color::from_hex(&s).ok_or_else(|| serde::de::Error::custom(format!("invalid color {s:?}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_hex_roundtrip() {
        let c = Color::from_hex("#1a2B3c").unwrap();
        assert_eq!(c, Color::rgb(0x1a, 0x2b, 0x3c));
        assert_eq!(c.to_hex(), "#1A2B3C");
        assert_eq!(Color::from_hex("fff").unwrap(), Color::WHITE);
        assert_eq!(Color::from_hex("#00000080").unwrap().a, 0x80);
        assert!(Color::from_hex("#12345").is_none());
        assert!(Color::from_hex("#zzzzzz").is_none());
        assert!(Color::from_hex("#ääää").is_none());
    }

    #[test]
    fn contrasting_text_is_readable() {
        assert_eq!(
            Color::WHITE.contrasting_text(),
            Color::rgb(0x1F, 0x23, 0x28)
        );
        assert_eq!(Color::BLACK.contrasting_text(), Color::WHITE);
    }

    #[test]
    fn rect_ops() {
        let r = Rect::from_center_size(Vec2::ZERO, Vec2::new(10.0, 4.0));
        assert_eq!(r.min, Vec2::new(-5.0, -2.0));
        assert!(r.contains(Vec2::new(4.0, 1.0)));
        assert!(!r.contains(Vec2::new(6.0, 0.0)));
        let u = r.union(&Rect::from_min_size(
            Vec2::new(10.0, 10.0),
            Vec2::new(1.0, 1.0),
        ));
        assert_eq!(u.max(), Vec2::new(11.0, 11.0));
        assert!(r.intersects(&Rect::from_min_size(
            Vec2::new(4.0, 1.0),
            Vec2::new(5.0, 5.0)
        )));
    }
}
