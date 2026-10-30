//! Font database shared by raster export and printing.

use resvg::usvg::fontdb::Database;
use std::sync::{Arc, OnceLock};

/// System fonts plus embedded fallbacks, loaded once per process.
///
/// Browsers do not expose system font files, so the web build renders text with
/// the fonts that egui embeds. Native exports also retain these text fallbacks:
/// the first system face may be a bitmap emoji or icon font.
pub fn database() -> Arc<Database> {
    static DB: OnceLock<Arc<Database>> = OnceLock::new();
    DB.get_or_init(|| Arc::new(build(!cfg!(target_arch = "wasm32"))))
        .clone()
}

pub(crate) fn build(system: bool) -> Database {
    let mut db = Database::new();
    if system {
        db.load_system_fonts();
    }
    db.load_font_data(epaint_default_fonts::UBUNTU_LIGHT.to_vec());
    db.load_font_data(epaint_default_fonts::HACK_REGULAR.to_vec());
    configure_generic_families(&mut db);
    db
}

fn configure_generic_families(db: &mut Database) {
    use resvg::usvg::fontdb::{Family, Query};

    let has_family = |family| {
        let query = Query {
            families: &[family],
            ..Query::default()
        };
        db.query(&query).is_some()
    };
    let has_sans = has_family(Family::SansSerif);
    let has_serif = has_family(Family::Serif);
    let has_cursive = has_family(Family::Cursive);
    let has_fantasy = has_family(Family::Fantasy);
    let has_monospace = has_family(Family::Monospace);

    if !has_sans {
        db.set_sans_serif_family("Ubuntu");
    }
    if !has_serif {
        db.set_serif_family("Ubuntu");
    }
    if !has_cursive {
        db.set_cursive_family("Ubuntu");
    }
    if !has_fantasy {
        db.set_fantasy_family("Ubuntu");
    }
    if !has_monospace {
        db.set_monospace_family("Hack");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_fallback_resolves_generic_families() {
        use resvg::usvg::fontdb::{Family, Query};
        let db = build(false);
        assert!(db.len() >= 2);
        for family in [Family::SansSerif, Family::Serif, Family::Monospace] {
            let query = Query {
                families: &[family],
                ..Query::default()
            };
            assert!(db.query(&query).is_some(), "{family:?}");
        }
    }

    #[test]
    fn partial_system_fonts_resolve_unavailable_generic_families() {
        use resvg::usvg::fontdb::{Family, Query};

        let mut db = Database::new();
        db.load_font_data(epaint_default_fonts::UBUNTU_LIGHT.to_vec());
        db.load_font_data(epaint_default_fonts::HACK_REGULAR.to_vec());
        db.set_serif_family("Unavailable serif");
        db.set_sans_serif_family("Unavailable sans");
        db.set_cursive_family("Unavailable cursive");
        db.set_fantasy_family("Unavailable fantasy");
        db.set_monospace_family("Unavailable monospace");

        configure_generic_families(&mut db);

        for family in [
            Family::SansSerif,
            Family::Serif,
            Family::Cursive,
            Family::Fantasy,
            Family::Monospace,
        ] {
            let query = Query {
                families: &[family],
                ..Query::default()
            };
            assert!(db.query(&query).is_some(), "{family:?}");
        }

        let options = resvg::usvg::Options {
            fontdb: Arc::new(db),
            ..Default::default()
        };
        let tree = resvg::usvg::Tree::from_str(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><text id="line" font-family="Segoe UI, Helvetica, Arial, sans-serif" font-size="16">Central</text></svg>"#,
            &options,
        )
        .unwrap();
        assert!(tree.node_by_id("line").is_some());
    }

    #[test]
    fn missing_generic_families_prefer_text_fonts_over_first_loaded_symbols() {
        use resvg::usvg::fontdb::{Family, Query};

        let mut db = Database::new();
        db.load_font_data(
            include_bytes!("../../thinoda-ui/assets/icons/ThinodaMaterialSymbols.ttf").to_vec(),
        );
        db.load_font_data(epaint_default_fonts::UBUNTU_LIGHT.to_vec());
        db.load_font_data(epaint_default_fonts::HACK_REGULAR.to_vec());
        db.set_sans_serif_family("Unavailable sans");
        db.set_monospace_family("Unavailable monospace");

        configure_generic_families(&mut db);

        for (family, expected) in [(Family::SansSerif, "Ubuntu"), (Family::Monospace, "Hack")] {
            let id = db
                .query(&Query {
                    families: &[family],
                    ..Query::default()
                })
                .unwrap();
            assert!(
                db.face(id)
                    .unwrap()
                    .families
                    .iter()
                    .any(|(name, _)| name == expected),
                "{family:?} must use a text font, not the first loaded symbol face"
            );
        }

        let tree = resvg::usvg::Tree::from_str(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="30"><text id="digits" y="20" font-family="sans-serif" font-size="18">x2+3</text></svg>"#,
            &resvg::usvg::Options {
                fontdb: Arc::new(db),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(tree.node_by_id("digits").is_some());
        let outlined = tree.to_string(&resvg::usvg::WriteOptions::default());
        assert!(!outlined.contains("<image"));
        let tree = svg2pdf::usvg::Tree::from_str(&outlined, &Default::default()).unwrap();
        let pdf = svg2pdf::to_pdf(&tree, Default::default(), Default::default()).unwrap();
        assert!(!String::from_utf8_lossy(&pdf).contains("/Subtype /Image"));
    }

    #[test]
    fn configured_system_text_families_are_preserved() {
        let mut db = build(false);
        db.set_sans_serif_family("Hack");
        db.set_monospace_family("Ubuntu");
        configure_generic_families(&mut db);
        assert_eq!(
            db.family_name(&resvg::usvg::fontdb::Family::SansSerif),
            "Hack"
        );
        assert_eq!(
            db.family_name(&resvg::usvg::fontdb::Family::Monospace),
            "Ubuntu"
        );
    }
}
