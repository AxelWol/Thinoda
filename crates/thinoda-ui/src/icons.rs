//! Thinoda's icon vocabulary: one Google Material Symbols (Outlined, Apache-2.0) glyph per
//! action, shared by toolbars, menus, the ribbon and the Outliner so the same action always
//! shows the same icon.
//!
//! Glyphs come from the bundled subset font `assets/icons/ThinodaMaterialSymbols.ttf`
//! (built by `assets/icons/build_icon_font.py`), installed by
//! [`crate::fonts::install_fallbacks`]. Material codepoints are shifted into the
//! Supplementary Private Use Area-A (U+F0000 + codepoint) so they never collide with other
//! fonts. Because icons are text they inherit the current text colour in light and dark mode.
//!
//! Sizing follows the Microsoft ribbon/Fluent tiers: 16 px for menus, the Quick Access
//! Toolbar and compact inline icons, 20 px for toolbar buttons, 24 px for labeled ribbon
//! buttons and 32 px for large ribbon buttons (see [`SIZE_SMALL`] and friends).

/// Menu items, Quick Access Toolbar, status bar and list rows (ribbon/Fluent "small").
pub const SIZE_SMALL: f32 = 16.0;
/// Header toolbar, floating action bar and node toolbar buttons (Fluent default).
pub const SIZE_TOOLBAR: f32 = 20.0;
/// Labeled ribbon buttons.
pub const SIZE_MEDIUM: f32 = 24.0;
/// Large ribbon buttons.
pub const SIZE_LARGE: f32 = 32.0;

pub const MENU: &str = "\u{fe5d2}"; // menu
pub const UNDO: &str = "\u{fe166}"; // undo
pub const REDO: &str = "\u{fe15a}"; // redo
pub const SEARCH: &str = "\u{fef7a}"; // search
pub const FIND_REPLACE: &str = "\u{fe881}"; // find_replace
pub const ADD: &str = "\u{fe145}"; // add
pub const REMOVE: &str = "\u{fe15b}"; // remove
pub const CLOSE: &str = "\u{fe5cd}"; // close
pub const CHECK: &str = "\u{fe668}"; // check
pub const SETTINGS: &str = "\u{fe8b8}"; // settings
pub const NEW_FILE: &str = "\u{fe89c}"; // note_add
pub const OPEN: &str = "\u{fe2c8}"; // folder_open
pub const SHOW_IN_FOLDER: &str = "\u{ff3d5}"; // folder_eye
pub const SAVE: &str = "\u{fe161}"; // save
pub const SAVE_AS: &str = "\u{feb60}"; // save_as
pub const EXPORT: &str = "\u{ff3b2}"; // file_export
pub const IMPORT: &str = "\u{fe890}"; // input
pub const PRINT: &str = "\u{fe8ad}"; // print
pub const TOPIC: &str = "\u{ff422}"; // add_row_below
pub const TOPIC_BEFORE: &str = "\u{ff423}"; // add_row_above
pub const SUBTOPIC: &str = "\u{fe5da}"; // subdirectory_arrow_right
pub const PARENT_TOPIC: &str = "\u{fe5d9}"; // subdirectory_arrow_left
pub const FLOATING_TOPIC: &str = "\u{fef83}"; // bubble
pub const RELATIONSHIP: &str = "\u{ff0c1}"; // conversion_path
pub const BOUNDARY: &str = "\u{ff74d}"; // select
pub const SUMMARY: &str = "\u{fead3}"; // data_object
pub const NOTE: &str = "\u{ff1fc}"; // sticky_note_2
pub const LINK: &str = "\u{fe250}"; // link
pub const OPEN_LINK: &str = "\u{fe89e}"; // open_in_new
pub const LABEL: &str = "\u{fe893}"; // label
pub const IMAGE: &str = "\u{fe3f4}"; // image
pub const MARKER: &str = "\u{fea1e}"; // emoji_symbols
pub const TODO: &str = "\u{fe9de}"; // check_box
pub const TASK_PLANNING: &str = "\u{fe742}"; // edit_calendar
pub const GANTT: &str = "\u{feb85}"; // view_timeline
pub const CALENDAR: &str = "\u{febcc}"; // calendar_month
pub const DATE: &str = "\u{fe878}"; // event
pub const TODAY: &str = "\u{fe8df}"; // today
pub const ZONES: &str = "\u{fe871}"; // dashboard
pub const OUTLINER: &str = "\u{fe241}"; // format_list_bulleted
pub const NUMBERING: &str = "\u{fe242}"; // format_list_numbered
pub const MAP: &str = "\u{fe97a}"; // account_tree
pub const ZEN: &str = "\u{fea78}"; // self_improvement
pub const PITCH: &str = "\u{fe41b}"; // slideshow
pub const PRESENTER: &str = "\u{feaf0}"; // co_present
pub const EDIT_PRESENTATION: &str = "\u{fe8eb}"; // view_carousel
pub const FORMAT: &str = "\u{fe40a}"; // palette
pub const TOPIC_STYLE: &str = "\u{fe3ae}"; // brush
pub const COPY_STYLE: &str = "\u{fe243}"; // format_paint
pub const RESET_STYLE: &str = "\u{fe239}"; // format_clear
pub const QUICK_STYLE: &str = "\u{fe41d}"; // style
pub const THEME: &str = "\u{feb37}"; // contrast
pub const DARK_MODE: &str = "\u{fe51c}"; // dark_mode
pub const LIGHT_MODE: &str = "\u{fe518}"; // light_mode
pub const ZOOM_IN: &str = "\u{fe8ff}"; // zoom_in
pub const ZOOM_OUT: &str = "\u{fe900}"; // zoom_out
pub const ZOOM_RESET: &str = "\u{fefcd}"; // 1x_mobiledata
pub const FIT: &str = "\u{fea10}"; // fit_screen
pub const SHARE: &str = "\u{fe80d}"; // share
pub const CLOUD: &str = "\u{ff15c}"; // cloud
pub const INFO: &str = "\u{fe88e}"; // info
pub const WARNING: &str = "\u{ff083}"; // warning
pub const ERROR: &str = "\u{ff8b6}"; // error
pub const CHEVRON_RIGHT: &str = "\u{fe5cc}"; // chevron_right
pub const CHEVRON_DOWN: &str = "\u{fe5cf}"; // expand_more
pub const COLLAPSE: &str = "\u{fe5d6}"; // unfold_less
pub const EXPAND: &str = "\u{fe5d7}"; // unfold_more
pub const COLLAPSE_ALL: &str = "\u{ff8cf}"; // unfold_less_double
pub const EXPAND_ALL: &str = "\u{ff8d0}"; // unfold_more_double
pub const FILTER: &str = "\u{fe152}"; // filter_list
pub const SHEET: &str = "\u{fe8d8}"; // tab
pub const KEYBOARD: &str = "\u{fe312}"; // keyboard
pub const HELP: &str = "\u{fe8fd}"; // help
pub const RECENT: &str = "\u{fe8b3}"; // history
pub const PIN: &str = "\u{ff10d}"; // push_pin
pub const UNPIN: &str = "\u{fe6f9}"; // keep_off
pub const TRASH: &str = "\u{fe92e}"; // delete
pub const STAR: &str = "\u{ff09a}"; // star
pub const COPY: &str = "\u{fe14d}"; // content_copy
pub const DUPLICATE: &str = "\u{fe03c}"; // library_add
pub const CUT: &str = "\u{fe14e}"; // content_cut
pub const PASTE: &str = "\u{fe14f}"; // content_paste
pub const EDIT: &str = "\u{ff097}"; // edit
pub const MOVE_UP: &str = "\u{feb64}"; // move_up
pub const MOVE_DOWN: &str = "\u{feb61}"; // move_down
pub const INDENT: &str = "\u{fe23e}"; // format_indent_increase
pub const OUTDENT: &str = "\u{fe23d}"; // format_indent_decrease
pub const SORT: &str = "\u{fe164}"; // sort
pub const AI: &str = "\u{fe65f}"; // auto_awesome
pub const AI_CHAT: &str = "\u{fe0c9}"; // chat
pub const BRAINSTORM: &str = "\u{fea4a}"; // psychology
pub const EXPLAIN: &str = "\u{fe90f}"; // lightbulb
pub const SUMMARIZE: &str = "\u{ff071}"; // summarize
pub const TRANSLATE: &str = "\u{fe8e2}"; // translate
pub const TEMPLATE: &str = "\u{fe66b}"; // space_dashboard
pub const CAMERA: &str = "\u{fe412}"; // photo_camera
pub const MERGE: &str = "\u{feb98}"; // merge
pub const MASK: &str = "\u{ff5e4}"; // domino_mask
pub const HOME: &str = "\u{fe9b2}"; // home
pub const QUIT: &str = "\u{ff8c7}"; // power_settings_new
pub const WINDOW: &str = "\u{ff710}"; // new_window
pub const USER_GUIDE: &str = "\u{fea19}"; // menu_book
pub const FEEDBACK: &str = "\u{fe87f}"; // feedback
pub const NEWS: &str = "\u{feb81}"; // newspaper
pub const QUICK_START: &str = "\u{feb9b}"; // rocket_launch
pub const STATUS_BAR: &str = "\u{ff729}"; // bottom_panel_open
pub const SELECT_ALL: &str = "\u{fe162}"; // select_all
pub const SELECT_NONE: &str = "\u{febb6}"; // deselect
pub const INVERT: &str = "\u{fe3e8}"; // flip
pub const ALIGN_LEFT: &str = "\u{fe00d}"; // align_horizontal_left
pub const ALIGN_CENTER: &str = "\u{fe00f}"; // align_horizontal_center
pub const ALIGN_RIGHT: &str = "\u{fe010}"; // align_horizontal_right
pub const TEXT_ALIGN_LEFT: &str = "\u{fe236}"; // format_align_left
pub const TEXT_ALIGN_CENTER: &str = "\u{fe234}"; // format_align_center
pub const TEXT_ALIGN_RIGHT: &str = "\u{fe237}"; // format_align_right
pub const ALIGN_TOP: &str = "\u{fe00c}"; // align_vertical_top
pub const ALIGN_MIDDLE: &str = "\u{fe011}"; // align_vertical_center
pub const ALIGN_BOTTOM: &str = "\u{fe015}"; // align_vertical_bottom
pub const DISTRIBUTE_HORIZONTAL: &str = "\u{fe014}"; // horizontal_distribute
pub const DISTRIBUTE_VERTICAL: &str = "\u{fe076}"; // vertical_distribute
pub const ARROW_LEFT: &str = "\u{fe5c4}"; // arrow_back
pub const ARROW_RIGHT: &str = "\u{fe5c8}"; // arrow_forward
pub const ARROW_UP: &str = "\u{fe5d8}"; // arrow_upward
pub const ARROW_DOWN: &str = "\u{fe5db}"; // arrow_downward
pub const VISIBLE: &str = "\u{fe8f4}"; // visibility
pub const RESET: &str = "\u{ff053}"; // restart_alt
pub const STACK: &str = "\u{ff500}"; // stacks
pub const BRANCH: &str = "\u{ff184}"; // alt_route
pub const ROOT: &str = "\u{fe39e}"; // adjust
pub const MORE: &str = "\u{fe5d3}"; // more_horiz
pub const BOLD: &str = "\u{fe238}"; // format_bold
pub const ITALIC: &str = "\u{fe23f}"; // format_italic
pub const STRIKE: &str = "\u{fe246}"; // format_strikethrough
pub const UNDERLINE: &str = "\u{fe249}"; // format_underlined

/// Every icon with its constant name and Material Symbols name.
pub const ALL: &[(&str, &str, &str)] = &[
    ("MENU", MENU, "menu"),
    ("UNDO", UNDO, "undo"),
    ("REDO", REDO, "redo"),
    ("SEARCH", SEARCH, "search"),
    ("FIND_REPLACE", FIND_REPLACE, "find_replace"),
    ("ADD", ADD, "add"),
    ("REMOVE", REMOVE, "remove"),
    ("CLOSE", CLOSE, "close"),
    ("CHECK", CHECK, "check"),
    ("SETTINGS", SETTINGS, "settings"),
    ("NEW_FILE", NEW_FILE, "note_add"),
    ("OPEN", OPEN, "folder_open"),
    ("SHOW_IN_FOLDER", SHOW_IN_FOLDER, "folder_eye"),
    ("SAVE", SAVE, "save"),
    ("SAVE_AS", SAVE_AS, "save_as"),
    ("EXPORT", EXPORT, "file_export"),
    ("IMPORT", IMPORT, "input"),
    ("PRINT", PRINT, "print"),
    ("TOPIC", TOPIC, "add_row_below"),
    ("TOPIC_BEFORE", TOPIC_BEFORE, "add_row_above"),
    ("SUBTOPIC", SUBTOPIC, "subdirectory_arrow_right"),
    ("PARENT_TOPIC", PARENT_TOPIC, "subdirectory_arrow_left"),
    ("FLOATING_TOPIC", FLOATING_TOPIC, "bubble"),
    ("RELATIONSHIP", RELATIONSHIP, "conversion_path"),
    ("BOUNDARY", BOUNDARY, "select"),
    ("SUMMARY", SUMMARY, "data_object"),
    ("NOTE", NOTE, "sticky_note_2"),
    ("LINK", LINK, "link"),
    ("OPEN_LINK", OPEN_LINK, "open_in_new"),
    ("LABEL", LABEL, "label"),
    ("IMAGE", IMAGE, "image"),
    ("MARKER", MARKER, "emoji_symbols"),
    ("TODO", TODO, "check_box"),
    ("TASK_PLANNING", TASK_PLANNING, "edit_calendar"),
    ("GANTT", GANTT, "view_timeline"),
    ("CALENDAR", CALENDAR, "calendar_month"),
    ("DATE", DATE, "event"),
    ("TODAY", TODAY, "today"),
    ("ZONES", ZONES, "dashboard"),
    ("OUTLINER", OUTLINER, "format_list_bulleted"),
    ("NUMBERING", NUMBERING, "format_list_numbered"),
    ("MAP", MAP, "account_tree"),
    ("ZEN", ZEN, "self_improvement"),
    ("PITCH", PITCH, "slideshow"),
    ("PRESENTER", PRESENTER, "co_present"),
    ("EDIT_PRESENTATION", EDIT_PRESENTATION, "view_carousel"),
    ("FORMAT", FORMAT, "palette"),
    ("TOPIC_STYLE", TOPIC_STYLE, "brush"),
    ("COPY_STYLE", COPY_STYLE, "format_paint"),
    ("RESET_STYLE", RESET_STYLE, "format_clear"),
    ("QUICK_STYLE", QUICK_STYLE, "style"),
    ("THEME", THEME, "contrast"),
    ("DARK_MODE", DARK_MODE, "dark_mode"),
    ("LIGHT_MODE", LIGHT_MODE, "light_mode"),
    ("ZOOM_IN", ZOOM_IN, "zoom_in"),
    ("ZOOM_OUT", ZOOM_OUT, "zoom_out"),
    ("ZOOM_RESET", ZOOM_RESET, "1x_mobiledata"),
    ("FIT", FIT, "fit_screen"),
    ("SHARE", SHARE, "share"),
    ("CLOUD", CLOUD, "cloud"),
    ("INFO", INFO, "info"),
    ("WARNING", WARNING, "warning"),
    ("ERROR", ERROR, "error"),
    ("CHEVRON_RIGHT", CHEVRON_RIGHT, "chevron_right"),
    ("CHEVRON_DOWN", CHEVRON_DOWN, "expand_more"),
    ("COLLAPSE", COLLAPSE, "unfold_less"),
    ("EXPAND", EXPAND, "unfold_more"),
    ("COLLAPSE_ALL", COLLAPSE_ALL, "unfold_less_double"),
    ("EXPAND_ALL", EXPAND_ALL, "unfold_more_double"),
    ("FILTER", FILTER, "filter_list"),
    ("SHEET", SHEET, "tab"),
    ("KEYBOARD", KEYBOARD, "keyboard"),
    ("HELP", HELP, "help"),
    ("RECENT", RECENT, "history"),
    ("PIN", PIN, "push_pin"),
    ("UNPIN", UNPIN, "keep_off"),
    ("TRASH", TRASH, "delete"),
    ("STAR", STAR, "star"),
    ("COPY", COPY, "content_copy"),
    ("DUPLICATE", DUPLICATE, "library_add"),
    ("CUT", CUT, "content_cut"),
    ("PASTE", PASTE, "content_paste"),
    ("EDIT", EDIT, "edit"),
    ("MOVE_UP", MOVE_UP, "move_up"),
    ("MOVE_DOWN", MOVE_DOWN, "move_down"),
    ("INDENT", INDENT, "format_indent_increase"),
    ("OUTDENT", OUTDENT, "format_indent_decrease"),
    ("SORT", SORT, "sort"),
    ("AI", AI, "auto_awesome"),
    ("AI_CHAT", AI_CHAT, "chat"),
    ("BRAINSTORM", BRAINSTORM, "psychology"),
    ("EXPLAIN", EXPLAIN, "lightbulb"),
    ("SUMMARIZE", SUMMARIZE, "summarize"),
    ("TRANSLATE", TRANSLATE, "translate"),
    ("TEMPLATE", TEMPLATE, "space_dashboard"),
    ("CAMERA", CAMERA, "photo_camera"),
    ("MERGE", MERGE, "merge"),
    ("MASK", MASK, "domino_mask"),
    ("HOME", HOME, "home"),
    ("QUIT", QUIT, "power_settings_new"),
    ("WINDOW", WINDOW, "new_window"),
    ("USER_GUIDE", USER_GUIDE, "menu_book"),
    ("FEEDBACK", FEEDBACK, "feedback"),
    ("NEWS", NEWS, "newspaper"),
    ("QUICK_START", QUICK_START, "rocket_launch"),
    ("STATUS_BAR", STATUS_BAR, "bottom_panel_open"),
    ("SELECT_ALL", SELECT_ALL, "select_all"),
    ("SELECT_NONE", SELECT_NONE, "deselect"),
    ("INVERT", INVERT, "flip"),
    ("ALIGN_LEFT", ALIGN_LEFT, "align_horizontal_left"),
    ("ALIGN_CENTER", ALIGN_CENTER, "align_horizontal_center"),
    ("ALIGN_RIGHT", ALIGN_RIGHT, "align_horizontal_right"),
    ("TEXT_ALIGN_LEFT", TEXT_ALIGN_LEFT, "format_align_left"),
    (
        "TEXT_ALIGN_CENTER",
        TEXT_ALIGN_CENTER,
        "format_align_center",
    ),
    ("TEXT_ALIGN_RIGHT", TEXT_ALIGN_RIGHT, "format_align_right"),
    ("ALIGN_TOP", ALIGN_TOP, "align_vertical_top"),
    ("ALIGN_MIDDLE", ALIGN_MIDDLE, "align_vertical_center"),
    ("ALIGN_BOTTOM", ALIGN_BOTTOM, "align_vertical_bottom"),
    (
        "DISTRIBUTE_HORIZONTAL",
        DISTRIBUTE_HORIZONTAL,
        "horizontal_distribute",
    ),
    (
        "DISTRIBUTE_VERTICAL",
        DISTRIBUTE_VERTICAL,
        "vertical_distribute",
    ),
    ("ARROW_LEFT", ARROW_LEFT, "arrow_back"),
    ("ARROW_RIGHT", ARROW_RIGHT, "arrow_forward"),
    ("ARROW_UP", ARROW_UP, "arrow_upward"),
    ("ARROW_DOWN", ARROW_DOWN, "arrow_downward"),
    ("VISIBLE", VISIBLE, "visibility"),
    ("RESET", RESET, "restart_alt"),
    ("STACK", STACK, "stacks"),
    ("BRANCH", BRANCH, "alt_route"),
    ("ROOT", ROOT, "adjust"),
    ("MORE", MORE, "more_horiz"),
    ("BOLD", BOLD, "format_bold"),
    ("ITALIC", ITALIC, "format_italic"),
    ("STRIKE", STRIKE, "format_strikethrough"),
    ("UNDERLINE", UNDERLINE, "format_underlined"),
];

#[cfg(test)]
mod tests {
    use super::ALL;

    #[test]
    fn every_icon_is_a_distinct_glyph_of_the_bundled_font() {
        let manifest: std::collections::HashMap<&str, u32> =
            include_str!("../assets/icons/material-symbols.codepoints")
                .lines()
                .map(|line| {
                    let (name, code) = line.split_once(' ').unwrap();
                    (name, u32::from_str_radix(code, 16).unwrap())
                })
                .collect();
        assert_eq!(manifest.len(), ALL.len(), "manifest and icons.rs differ");
        let mut glyphs = std::collections::HashSet::new();
        for (name, glyph, material) in ALL {
            let mut chars = glyph.chars();
            let code = chars.next().unwrap() as u32;
            assert!(chars.next().is_none(), "{name} is one glyph");
            assert_eq!(manifest.get(material), Some(&code), "{name} = {material}");
            assert!(glyphs.insert(*glyph), "{name} duplicates another icon");
        }
    }

    #[test]
    fn all_glyphs_render_after_font_installation() {
        let mut harness = egui_kittest::Harness::new_ui_state(
            |ui, missing: &mut Vec<&'static str>| {
                let font = egui::FontId::proportional(16.0);
                *missing = ALL
                    .iter()
                    .filter(|(_, g, _)| !ui.fonts_mut(|f| f.has_glyphs(&font, g)))
                    .map(|(n, _, _)| *n)
                    .collect();
            },
            Vec::new(),
        );
        crate::fonts::install_fallbacks(&harness.ctx);
        harness.run_steps(2);
        assert!(
            harness.state().is_empty(),
            "missing glyphs: {:?}",
            harness.state()
        );
    }
}
