//! System font fallbacks so symbol glyphs used by [`crate::icons`] render on every platform.

use std::sync::Arc;

use egui::{Context, FontData, FontDefinitions, FontFamily};

/// Installed font names, with face data loaded only for families used by a document.
#[derive(Default)]
pub struct SystemFonts {
    database: Option<fontdb::Database>,
    names: Vec<String>,
    attempted: std::collections::BTreeSet<String>,
    preferred_fallback: Option<String>,
    pending: Option<(u64, FontDefinitions)>,
}

impl SystemFonts {
    fn database(&mut self) -> &fontdb::Database {
        self.database.get_or_insert_with(|| {
            let mut db = fontdb::Database::new();
            db.load_system_fonts();
            self.names = db
                .faces()
                .filter_map(|face| face.families.first().map(|(name, _)| name.clone()))
                .collect();
            self.names.sort_by_key(|name| name.to_lowercase());
            self.names.dedup();
            db
        })
    }

    pub fn names(&mut self) -> &[String] {
        self.database();
        &self.names
    }

    pub fn preferred_fallback(&self) -> Option<&str> {
        self.preferred_fallback.as_deref()
    }

    fn face_data(&mut self, name: &str) -> Option<FontData> {
        let db = self.database();
        let family = match name {
            "serif" => fontdb::Family::Serif,
            "cursive" => fontdb::Family::Cursive,
            "fantasy" => fontdb::Family::Fantasy,
            _ => fontdb::Family::Name(name),
        };
        let id = db.query(&fontdb::Query {
            families: &[family],
            ..Default::default()
        })?;
        db.with_face_data(id, |bytes, index| {
            let mut data = FontData::from_owned(bytes.to_vec());
            data.index = index;
            data
        })
    }

    /// Register selected system faces without replacing the application's fallback fonts.
    /// The preferred fallback follows every configured face, including newly registered
    /// named families. Changes and removals update all families together.
    /// Unavailable families are reported once; the document's requested name is unchanged.
    pub fn ensure(
        &mut self,
        ctx: &Context,
        names: impl IntoIterator<Item = String>,
        preferred_fallback: Option<&str>,
    ) -> Vec<String> {
        // set_fonts is deferred: a theme preview can register more faces in this pass.
        let pass = ctx.cumulative_pass_nr();
        let mut defs = self
            .pending
            .take()
            .filter(|(pending_pass, _)| *pending_pass == pass)
            .map(|(_, defs)| defs)
            .unwrap_or_else(|| ctx.fonts(|fonts| fonts.definitions().clone()));
        let mut changed = false;
        let mut errors = Vec::new();
        if self.preferred_fallback() != preferred_fallback {
            const KEY: &str = "thinoda-preferred-cjk";
            changed |= defs.font_data.remove(KEY).is_some();
            for chain in defs.families.values_mut() {
                chain.retain(|key| key != KEY);
            }
            self.preferred_fallback = preferred_fallback.map(str::to_owned);
            if let Some(name) = preferred_fallback {
                if let Some(data) = self.face_data(name) {
                    defs.font_data.insert(KEY.into(), Arc::new(data));
                    for chain in defs.families.values_mut() {
                        chain.push(KEY.into());
                    }
                    changed = true;
                } else {
                    errors.push(name.to_owned());
                }
            }
        }
        for name in names {
            if matches!(name.as_str(), "" | "sans-serif" | "monospace")
                || defs
                    .families
                    .contains_key(&FontFamily::Name(name.clone().into()))
                || !self.attempted.insert(name.clone())
            {
                continue;
            }
            let data = self.face_data(&name);
            if let Some(data) = data {
                let mut fallback = defs.families[&FontFamily::Proportional].clone();
                let key = format!("thinoda-system:{name}");
                defs.font_data.insert(key.clone(), Arc::new(data));
                fallback.insert(0, key);
                defs.families
                    .insert(FontFamily::Name(name.into()), fallback);
                changed = true;
            } else {
                errors.push(name);
            }
        }
        if changed {
            ctx.set_fonts(defs.clone());
            ctx.request_repaint();
        }
        self.pending = Some((pass, defs));
        errors
    }
}

/// Symbol/emoji fonts shipped with the operating systems (first existing file per entry wins).
const CANDIDATES: &[&[&str]] = &[
    // Windows
    &["C:\\Windows\\Fonts\\seguisym.ttf"],
    &["C:\\Windows\\Fonts\\segoeui.ttf"],
    // macOS
    &[
        "/System/Library/Fonts/Apple Symbols.ttf",
        "/System/Library/Fonts/Supplemental/Apple Symbols.ttf",
    ],
    // Linux (Ubuntu)
    &[
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
    ],
    &[
        "/usr/share/fonts/truetype/noto/NotoSansSymbols2-Regular.ttf",
        "/usr/share/fonts/noto/NotoSansSymbols2-Regular.ttf",
    ],
];

/// Font files available on this system, in fallback order.
pub fn system_fallbacks() -> Vec<std::path::PathBuf> {
    CANDIDATES
        .iter()
        .filter_map(|alts| {
            alts.iter()
                .map(std::path::PathBuf::from)
                .find(|p| p.is_file())
        })
        .collect()
}

/// Appends the available system symbol fonts as fallbacks to egui's default fonts.
/// Returns the number of fonts installed.
pub fn install_fallbacks(ctx: &Context) -> usize {
    let mut defs = FontDefinitions::default();
    let mut n = 0;
    for path in system_fallbacks() {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let name = format!("system-fallback-{n}");
        defs.font_data
            .insert(name.clone(), Arc::new(FontData::from_owned(bytes)));
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            defs.families.entry(family).or_default().push(name.clone());
        }
        n += 1;
    }
    if n > 0 {
        ctx.set_fonts(defs);
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferred_fallback_changes_all_chains_without_replacing_primary_faces() {
        let ctx = Context::default();
        let mut db = fontdb::Database::new();
        let defaults = FontDefinitions::default();
        for name in ["Hack", "Ubuntu-Light"] {
            db.load_font_data(defaults.font_data[name].bytes().to_vec());
        }
        let mut names: Vec<_> = db.faces().map(|face| face.families[0].0.clone()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 2);
        let mut configured = defaults;
        for chain in configured.families.values_mut() {
            *chain = vec!["Hack".into()];
        }
        ctx.set_fonts(configured);
        let mut fonts = SystemFonts {
            database: Some(db),
            ..Default::default()
        };
        ctx.begin_pass(Default::default());
        let original = ctx.fonts(|f| f.definitions().clone());
        assert!(!ctx.fonts_mut(|f| f.has_glyph(&egui::FontId::default(), '\u{018f}')));
        assert!(fonts.ensure(&ctx, [names[0].clone()], None).is_empty());
        ctx.end_pass().textures_delta.clear();
        ctx.begin_pass(Default::default());
        let named = FontFamily::Name(names[0].clone().into());
        let primary = ctx.fonts(|f| f.definitions().families[&named][0].clone());
        assert!(fonts.ensure(&ctx, [], Some(&names[1])).is_empty());
        assert!(
            fonts
                .ensure(&ctx, [names[1].clone()], Some(&names[1]))
                .is_empty()
        );
        ctx.end_pass().textures_delta.clear();
        ctx.begin_pass(Default::default());
        let installed = ctx.fonts(|f| f.definitions().clone());
        assert!(
            installed
                .families
                .contains_key(&FontFamily::Name(names[1].clone().into()))
        );
        assert!(ctx.fonts_mut(|f| f.has_glyph(&egui::FontId::default(), '\u{018f}')));
        for (family, chain) in &installed.families {
            assert_eq!(chain.last().unwrap(), "thinoda-preferred-cjk", "{family:?}");
            assert_eq!(
                chain
                    .iter()
                    .filter(|s| *s == "thinoda-preferred-cjk")
                    .count(),
                1
            );
        }
        assert_eq!(installed.families[&named][0], primary);
        assert_eq!(
            &installed.families[&FontFamily::Proportional]
                [..original.families[&FontFamily::Proportional].len()],
            &original.families[&FontFamily::Proportional]
        );
        assert!(fonts.ensure(&ctx, [], Some(&names[1])).is_empty());
        assert_eq!(fonts.preferred_fallback(), Some(names[1].as_str()));
        assert!(fonts.ensure(&ctx, [], Some(&names[0])).is_empty());
        ctx.end_pass().textures_delta.clear();
        ctx.begin_pass(Default::default());
        assert!(!ctx.fonts_mut(|f| f.has_glyph(&egui::FontId::default(), '\u{018f}')));
        assert_eq!(
            ctx.fonts(|f| f.definitions().families[&named][0].clone()),
            primary
        );
        assert_eq!(
            fonts.ensure(&ctx, [], Some("Thinoda Missing CJK Family")),
            ["Thinoda Missing CJK Family"]
        );
        ctx.end_pass().textures_delta.clear();
        ctx.begin_pass(Default::default());
        assert!(ctx.fonts(|f| {
            !f.definitions()
                .font_data
                .contains_key("thinoda-preferred-cjk")
        }));
        assert!(
            fonts
                .ensure(&ctx, [], Some("Thinoda Missing CJK Family"))
                .is_empty()
        );
        assert!(fonts.ensure(&ctx, [], None).is_empty());
        assert!(fonts.preferred_fallback().is_none());
        assert_eq!(
            ctx.fonts(|f| f.definitions().families[&FontFamily::Proportional].clone()),
            original.families[&FontFamily::Proportional]
        );
        ctx.end_pass().textures_delta.clear();
    }

    #[test]
    fn named_faces_register_once_with_existing_fallbacks_and_report_missing_names() {
        let ctx = Context::default();
        let mut db = fontdb::Database::new();
        db.load_font_data(
            FontDefinitions::default().font_data["Hack"]
                .bytes()
                .to_vec(),
        );
        let name = db.faces().next().unwrap().families[0].0.clone();
        let mut fonts = SystemFonts {
            database: Some(db),
            ..Default::default()
        };
        ctx.begin_pass(Default::default());
        let original = ctx.fonts(|f| f.definitions().clone());
        assert!(fonts.ensure(&ctx, [name.clone()], None).is_empty());
        ctx.end_pass().textures_delta.clear();
        ctx.begin_pass(Default::default());
        let installed = ctx.fonts(|f| f.definitions().clone());
        assert_eq!(
            installed.families[&FontFamily::Proportional],
            original.families[&FontFamily::Proportional]
        );
        let family = FontFamily::Name(name.clone().into());
        let primary = &installed.families[&family][0];
        assert_eq!(
            installed.font_data[primary].bytes(),
            FontDefinitions::default().font_data["Hack"].bytes()
        );
        assert_eq!(
            &installed.families[&family][1..],
            &original.families[&FontFamily::Proportional]
        );
        assert!(fonts.ensure(&ctx, [name], None).is_empty());
        assert_eq!(
            fonts.ensure(&ctx, ["Thinoda Missing Family".into()], None),
            ["Thinoda Missing Family"]
        );
        assert!(
            fonts
                .ensure(&ctx, ["Thinoda Missing Family".into()], None)
                .is_empty()
        );
        assert_eq!(
            ctx.fonts(|f| f.definitions().font_data.len()),
            installed.font_data.len()
        );
        ctx.end_pass().textures_delta.clear();
    }

    #[test]
    fn fallbacks_are_existing_files() {
        for p in super::system_fallbacks() {
            assert!(p.is_file());
        }
    }
}
