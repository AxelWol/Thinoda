//! System font fallbacks so symbol glyphs used by [`crate::icons`] render on every platform.

use std::sync::Arc;

use egui::{Context, FontData, FontDefinitions, FontFamily};

/// Installed font names, with face data loaded only for families used by a document.
#[derive(Default)]
pub struct SystemFonts {
    database: Option<fontdb::Database>,
    names: Vec<String>,
    attempted: std::collections::BTreeSet<String>,
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

    /// Register selected system faces without replacing the application's fallback fonts.
    /// Unavailable families are reported once; the document's requested name is unchanged.
    pub fn ensure(
        &mut self,
        ctx: &Context,
        names: impl IntoIterator<Item = String>,
    ) -> Vec<String> {
        let mut defs = ctx.fonts(|fonts| fonts.definitions().clone());
        let mut changed = false;
        let mut errors = Vec::new();
        for name in names {
            if matches!(name.as_str(), "" | "sans-serif" | "monospace")
                || defs
                    .families
                    .contains_key(&FontFamily::Name(name.clone().into()))
                || !self.attempted.insert(name.clone())
            {
                continue;
            }
            let db = self.database();
            let family = match name.as_str() {
                "serif" => fontdb::Family::Serif,
                "cursive" => fontdb::Family::Cursive,
                "fantasy" => fontdb::Family::Fantasy,
                _ => fontdb::Family::Name(&name),
            };
            let face = db.query(&fontdb::Query {
                families: &[family],
                ..Default::default()
            });
            let data = face.and_then(|id| {
                db.with_face_data(id, |bytes, index| {
                    let mut data = FontData::from_owned(bytes.to_vec());
                    data.index = index;
                    data
                })
            });
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
            ctx.set_fonts(defs);
            ctx.request_repaint();
        }
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
        assert!(fonts.ensure(&ctx, [name.clone()]).is_empty());
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
        assert!(fonts.ensure(&ctx, [name]).is_empty());
        assert_eq!(
            fonts.ensure(&ctx, ["Thinoda Missing Family".into()]),
            ["Thinoda Missing Family"]
        );
        assert!(
            fonts
                .ensure(&ctx, ["Thinoda Missing Family".into()])
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
