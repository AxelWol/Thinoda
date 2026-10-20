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
        let sans = first_family(&db);
        db.load_font_data(epaint_default_fonts::HACK_REGULAR.to_vec());
        let mono = db
            .faces()
            .filter_map(|face| face.families.first())
            .map(|(name, _)| name.clone())
            .find(|name| Some(name) != sans.as_ref());
        if let Some(sans) = sans {
            db.set_sans_serif_family(sans.clone());
            db.set_serif_family(sans.clone());
            db.set_cursive_family(sans.clone());
            db.set_fantasy_family(sans);
        }
        if let Some(mono) = mono {
            db.set_monospace_family(mono);
        }
    }
    db
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
}
