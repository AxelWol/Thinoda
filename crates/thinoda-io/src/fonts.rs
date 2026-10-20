//! Font database shared by raster export and printing.

use resvg::usvg::fontdb::Database;
use std::sync::{Arc, OnceLock};

/// System fonts plus embedded fallbacks, loaded once per process.
///
/// Browsers do not expose system font files, so the web build (and systems
/// without fonts, such as minimal containers) render text with the fonts that
/// egui embeds. Generic families map to a loaded face so `sans-serif` and
/// `monospace` always resolve.
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
    if db.is_empty() {
        db.load_font_data(epaint_default_fonts::UBUNTU_LIGHT.to_vec());
        db.load_font_data(epaint_default_fonts::HACK_REGULAR.to_vec());
    }
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

    let sans = first_family(db);
    let mono = db
        .faces()
        .filter_map(|face| face.families.first())
        .map(|(name, _)| name.clone())
        .find(|name| Some(name) != sans.as_ref());

    if !has_sans && let Some(sans) = &sans {
        db.set_sans_serif_family(sans.clone());
    }
    if !has_serif && let Some(sans) = &sans {
        db.set_serif_family(sans.clone());
    }
    if !has_cursive && let Some(sans) = &sans {
        db.set_cursive_family(sans.clone());
    }
    if !has_fantasy && let Some(sans) = &sans {
        db.set_fantasy_family(sans);
    }
    if !has_monospace && let Some(mono) = mono.or(sans) {
        db.set_monospace_family(mono);
    }
}

fn first_family(db: &Database) -> Option<String> {
    db.faces()
        .find_map(|face| face.families.first())
        .map(|(name, _)| name.clone())
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
}
