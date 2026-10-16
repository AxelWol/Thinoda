//! System font fallbacks so symbol glyphs used by [`crate::icons`] render on every platform.

use std::sync::Arc;

use egui::{Context, FontData, FontDefinitions, FontFamily};

/// Symbol/emoji fonts shipped with the operating systems (first existing file per entry wins).
const CANDIDATES: &[&[&str]] = &[
    // Windows
    &["C:\\Windows\\Fonts\\seguisym.ttf"],
    &["C:\\Windows\\Fonts\\segoeui.ttf"],
    // macOS
    &["/System/Library/Fonts/Apple Symbols.ttf", "/System/Library/Fonts/Supplemental/Apple Symbols.ttf"],
    // Linux (Ubuntu)
    &["/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", "/usr/share/fonts/TTF/DejaVuSans.ttf"],
    &["/usr/share/fonts/truetype/noto/NotoSansSymbols2-Regular.ttf", "/usr/share/fonts/noto/NotoSansSymbols2-Regular.ttf"],
];

/// Font files available on this system, in fallback order.
pub fn system_fallbacks() -> Vec<std::path::PathBuf> {
    CANDIDATES
        .iter()
        .filter_map(|alts| alts.iter().map(std::path::PathBuf::from).find(|p| p.is_file()))
        .collect()
}

/// Appends the available system symbol fonts as fallbacks to egui's default fonts.
/// Returns the number of fonts installed.
pub fn install_fallbacks(ctx: &Context) -> usize {
    let mut defs = FontDefinitions::default();
    let mut n = 0;
    for path in system_fallbacks() {
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let name = format!("system-fallback-{n}");
        defs.font_data.insert(name.clone(), Arc::new(FontData::from_owned(bytes)));
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
    #[test]
    fn fallbacks_are_existing_files() {
        for p in super::system_fallbacks() {
            assert!(p.is_file());
        }
    }
}
