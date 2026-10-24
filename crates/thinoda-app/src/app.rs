//! Application state, command dispatch and lifecycle (recovery, autosave, close handling).

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use web_time::Instant;

use egui::{Color32, Context, Key, KeyboardShortcut, Modifiers, ViewportCommand};
use thinoda_canvas::{Action, FloatingArrangement, apply_action};
use thinoda_core::{Workbook, search::SearchOptions, templates};
use thinoda_io::{ExportOptions, Format};
use thinoda_platform::{
    Paths, Settings,
    cloud::{CloudFolder, detect_cloud_folders},
    recovery::{OrphanSession, RecoveryManager},
    settings::ThemeMode as SettingsTheme,
    single_instance::SingleInstance,
    system,
};
use thinoda_ui::{CommandPalette, ThemeMode, Toasts};

use crate::{
    doc::{OpenDoc, is_native},
    files,
    i18n::{self, t},
};

/// Save dialog filters for map documents.
const MAP_FILTERS: &[files::Filter<'static>] = &[
    ("Thinoda", &["thinoda"]),
    ("XMind", &["xmind"]),
    ("MindManager", &["mmap"]),
];

/// Every user command of the app (menus, toolbar, shortcuts and command palette).
#[derive(Debug, Clone, PartialEq)]
pub enum Cmd {
    Canvas(Action),
    New,
    NewFromTemplate(&'static str),
    Home,
    Open,
    OpenPath(PathBuf),
    Save,
    SaveAs,
    SaveAll,
    CloseTab(usize),
    NextDoc,
    Import,
    MergeFiles,
    MaskedCopy,
    MapShot,
    Print,
    Labels,
    Export(Format),
    Settings,
    About,
    Shortcuts,
    UserGuide,
    QuickStart,
    Feedback,
    ProjectNews,
    Quit,
    ToggleOutliner,
    ToggleZen,
    ToggleFormatPanel,
    ToggleSymbolsPanel,
    ToggleStatusBar,
    Find,
    FindReplace,
    TopicStyle,
    Palette,
    NewSheet,
    DuplicateSheet,
    DeleteSheet,
    RenameSheet,
    PreviousSheet,
    NextSheet,
    CopyStyle,
    PasteStyle,
    ResetStyle,
    SaveQuickStyle,
    ApplyQuickStyle(thinoda_core::TopicStyle),
    EditCustomTheme,
    ApplyCustomTheme(Box<thinoda_core::custom_theme::CustomTheme>),
    CopySheet,
    PasteSheet,
    SaveSheetAs,
    NewSheetFromTopic,
    OpenLink,
    ShowInFolder,
    MenuPaste,
    SetTheme(SettingsTheme),
    /// Focus a topic property field in the Format panel (Ctrl+K / Ctrl+Shift+N).
    EditField(FocusField),
    Ai(crate::ai::AiCommand),
}

/// Format panel fields that commands can focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusField {
    Hyperlink,
    Notes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CenterMode {
    Map,
    Outliner,
}

pub use thinoda_platform::settings::{FormatTab, SidebarFamily};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    #[default]
    General,
    Editor,
    Ai,
    Cloud,
}

/// What to do after the "save changes?" dialog.
#[derive(Debug, Clone, PartialEq)]
pub enum CloseIntent {
    Tab(String),
    Quit,
}

#[derive(Default)]
pub struct SearchState {
    pub open: bool,
    pub focus: bool,
    pub query: String,
    pub replace: String,
    pub show_replace: bool,
    pub opts: SearchOptions,
    pub hits: Vec<thinoda_core::search::SearchHit>,
    pub all_sheets: bool,
    pub current: usize,
    pub dim: bool,
    pub last_key: (String, usize, String, u64, bool, bool, bool, bool, bool),
}

/// Optional OS integrations (absent in tests and when the profile is unavailable).
#[derive(Default)]
pub struct Services {
    pub paths: Option<Paths>,
    pub recovery: Option<Arc<RecoveryManager>>,
    pub instance: Option<SingleInstance>,
    pub cloud: Vec<CloudFolder>,
}

#[derive(Default)]
pub struct RenameSheetDraft {
    pub(crate) document_key: String,
    pub(crate) revision: u64,
    pub(crate) index: usize,
    pub(crate) name: String,
    pub(crate) ime_composing: bool,
    pub(crate) focus_requested: bool,
}

#[derive(Default)]
pub struct Dialogs {
    pub mindmanager_export: bool,
    pub mindmanager_symbol_color_loss: bool,
    pub mindmanager_save: Option<(String, PathBuf, bool)>,
    pub(crate) save_close_tab: Option<String>,
    pub imports: std::collections::VecDeque<(PathBuf, OpenDoc, Vec<thinoda_io::ImportWarning>)>,
    pub symbol_export: Option<(usize, Format)>,
    pub(crate) export_owner: Option<(String, usize, u64)>,
    pub guide: crate::help::GuideState,
    pub settings: bool,
    pub settings_tab: SettingsTab,
    pub(crate) settings_gesture: Option<crate::ui::SettingsGesture>,
    pub about: bool,
    pub legal: crate::legal::LegalState,
    pub shortcuts: bool,
    pub shortcut_browser: crate::shortcut_browser::ShortcutBrowser,
    pub masked_copy: bool,
    pub(crate) masked_copy_owner: Option<(String, usize, u64)>,
    pub close: Option<CloseIntent>,
    pub conflict: Option<String>,
    pub rename_sheet: Option<RenameSheetDraft>,
    pub recovery: Vec<OrphanSession>,
}

pub struct ThinodaApp {
    pub docs: Vec<OpenDoc>,
    pub active: usize,
    pub settings: Settings,
    pub services: Services,
    pub mode: CenterMode,
    pub zen: bool,
    pub format_tab: FormatTab,
    pub sidebar_family: SidebarFamily,
    pub template_choice: Option<&'static str>,
    pub template_category: Option<templates::TemplateCategory>,
    pub home_return_to: Option<String>,
    /// Explicit topic-style overrides, shared by documents for this app session.
    pub style_clipboard: Option<thinoda_core::TopicStyle>,
    pub symbol_color: Option<thinoda_core::Color>,
    pub sheet_paste_request: Option<(Instant, String)>,
    pub discard_late_sheet_paste: bool,
    pub toasts: Toasts,
    pub palette: CommandPalette,
    pub search: SearchState,
    pub dialogs: Dialogs,
    pub accent: Color32,
    pub(crate) applied: Option<(SettingsTheme, Color32, u32)>,
    pub(crate) quitting: bool,
    pub(crate) save_all: Option<crate::save_all::SaveAll>,
    last_poll: Instant,
    last_frame: Instant,
    /// Ask the OS clipboard for text on the next frame (menu "Paste").
    pub(crate) paste_requested: bool,
    /// Format panel field to focus on the next frame.
    pub focus_field: Option<FocusField>,
    pub ai: crate::ai::AiState,
    pub(crate) map_shot: crate::map_shot::MapShot,
    pub(crate) printing: crate::printing::PrintState,
    pub(crate) label_editor: Option<crate::labels::LabelEditor>,
    pub(crate) fonts: thinoda_ui::fonts::SystemFonts,
    pub(crate) font_source: Option<(String, u64)>,
    pub(crate) quick_style_dialog: Option<crate::quick_styles::StyleDialog>,
    pub(crate) theme_dialog: Option<crate::theme_editor::ThemeDialog>,
    last_title: String,
    /// Startup work still pending behind the splash; `None` once ready.
    pub(crate) launch: Option<crate::startup::Launch>,
}

pub fn ui_theme(t: SettingsTheme) -> ThemeMode {
    match t {
        SettingsTheme::System => ThemeMode::System,
        SettingsTheme::Light => ThemeMode::Light,
        SettingsTheme::Dark => ThemeMode::Dark,
    }
}

/// Export formats in the order of XMind's Export menu (all free in Thinoda).
pub const EXPORT_MENU: &[Format] = &[
    Format::MindManager,
    Format::Png,
    Format::Jpeg,
    Format::Svg,
    Format::Pdf,
    Format::Markdown,
    Format::Word,
    Format::Excel,
    Format::Opml,
    Format::TextBundle,
    Format::FreeMind,
    Format::XMind,
    Format::Csv,
    Format::PlainText,
];

impl ThinodaApp {
    /// Creates the app with the given settings and services. Opens `files` (or nothing → home).
    pub fn new(settings: Settings, services: Services, files: &[PathBuf]) -> Self {
        i18n::set_lang(i18n::resolve(settings.language));
        let mut app = Self {
            docs: Vec::new(),
            active: 0,
            format_tab: settings.format_tab,
            sidebar_family: settings.sidebar_family,
            settings,
            services,
            mode: CenterMode::Map,
            zen: false,
            template_choice: None,
            template_category: None,
            home_return_to: None,
            style_clipboard: None,
            symbol_color: None,
            sheet_paste_request: None,
            discard_late_sheet_paste: false,
            toasts: Toasts::default(),
            palette: CommandPalette::default(),
            search: SearchState::default(),
            dialogs: Dialogs::default(),
            accent: thinoda_ui::tokens::DEFAULT_ACCENT,
            applied: None,
            quitting: false,
            save_all: None,
            last_poll: Instant::now(),
            last_frame: Instant::now(),
            paste_requested: false,
            focus_field: None,
            ai: Default::default(),
            map_shot: Default::default(),
            printing: Default::default(),
            label_editor: None,
            fonts: Default::default(),
            font_source: None,
            quick_style_dialog: None,
            theme_dialog: None,
            last_title: String::new(),
            launch: None,
        };
        for f in files {
            app.open_path(f);
        }
        app
    }

    /// App for tests: no OS services, English UI, one new document.
    pub fn headless() -> Self {
        let settings = Settings {
            language: thinoda_platform::settings::Language::English,
            ..Settings::default()
        };
        let mut app = Self::new(settings, Services::default(), &[]);
        app.new_doc(templates::instantiate("mind-map"));
        app
    }

    pub fn doc(&self) -> Option<&OpenDoc> {
        self.docs.get(self.active)
    }

    pub fn doc_mut(&mut self) -> Option<&mut OpenDoc> {
        self.docs.get_mut(self.active)
    }

    pub fn new_doc(&mut self, wb: Workbook) {
        let mut d = OpenDoc::new(wb);
        d.view.titles = new_topic_titles();
        d.view.set_initial_zoom(self.settings.initial_canvas_zoom);
        self.docs.push(d);
        self.active = self.docs.len() - 1;
        self.mode = CenterMode::Map;
    }

    pub fn open_path(&mut self, path: &Path) {
        if let Some(i) = self
            .docs
            .iter()
            .position(|d| d.path.as_deref() == Some(path))
        {
            self.active = i;
            return;
        }
        if self.dialogs.imports.iter().any(|(p, ..)| p == path) {
            return;
        }
        self.add_opened(path, OpenDoc::open_with_report(path));
    }

    /// Adds the result of opening `path` as a tab, or reports the error as a toast.
    pub(crate) fn add_opened(
        &mut self,
        path: &Path,
        opened: Result<(OpenDoc, Vec<thinoda_io::ImportWarning>), String>,
    ) {
        if let Some(i) = self
            .docs
            .iter()
            .position(|d| d.path.as_deref() == Some(path))
        {
            self.active = i;
            return;
        }
        match opened {
            Ok((d, warnings)) if !warnings.is_empty() => {
                if !self.dialogs.imports.iter().any(|(p, ..)| p == path) {
                    self.dialogs
                        .imports
                        .push_back((path.to_path_buf(), d, warnings));
                }
            }
            Ok((d, _)) => self.install_opened_doc(path, d),
            Err(e) => {
                self.toasts
                    .error(format!("{} {}: {e}", t("Could not open"), path.display()));
            }
        }
    }

    pub fn resolve_import(&mut self, accept: bool) {
        if let Some((path, d, _)) = self.dialogs.imports.pop_front()
            && accept
        {
            self.install_opened_doc(&path, d);
        }
    }

    fn install_opened_doc(&mut self, path: &Path, mut d: OpenDoc) {
        let native = is_native(d.format) && d.import_source.is_none();
        d.view.titles = new_topic_titles();
        d.view.set_initial_zoom(self.settings.initial_canvas_zoom);
        // Replace a pristine, untouched "Untitled" tab like most editors do.
        if self.docs.len() == 1
            && self.docs[0].path.is_none()
            && !self.docs[0].is_dirty()
            && !self.docs[0].doc.can_undo()
        {
            self.docs.clear();
        }
        self.docs.push(d);
        self.active = self.docs.len() - 1;
        if native {
            self.settings.add_recent(path);
            self.save_settings();
        }
    }

    pub fn save_settings(&mut self) {
        self.settings.format_tab = self.format_tab;
        self.settings.sidebar_family = self.sidebar_family;
        if let Some(p) = self.services.paths.clone()
            && let Err(e) = self
                .settings
                .save_with_credentials(&p, &thinoda_platform::credentials::NativeCredentialStore)
        {
            log::warn!("could not save settings: {e}");
            self.toasts
                .error(format!("{}: {e}", t("Could not save settings securely")));
        }
    }

    // ----- commands --------------------------------------------------------------------------

    pub fn exec(&mut self, ctx: &Context, cmd: Cmd) {
        for doc in &mut self.docs {
            if let Err(error) = doc.flush_inspector_edit(ctx) {
                log::warn!("inspector commit before {cmd:?}: {error}");
                self.toasts.error(error);
                return;
            }
        }
        if matches!(
            cmd,
            Cmd::Save | Cmd::SaveAs | Cmd::SaveAll | Cmd::Export(_) | Cmd::Quit
        ) && !self.command_available(&cmd)
        {
            self.toasts.warning(t("Not possible here"));
            return;
        }
        match cmd {
            Cmd::Canvas(action) => self.canvas_action(ctx, action),
            Cmd::New => {
                self.remember_map_defaults();
                let mut workbook = templates::instantiate("mind-map");
                let sheet = workbook.active_mut();
                if self.settings.reuse_last_structure
                    && let Some(structure) =
                        thinoda_core::Structure::from_xmind_class(&self.settings.default_structure)
                {
                    sheet.structure = structure;
                }
                if self.settings.reuse_last_theme
                    && thinoda_core::theme::Theme::IDS
                        .contains(&self.settings.default_theme.as_str())
                {
                    sheet.theme.theme_id = self.settings.default_theme.clone();
                    sheet.theme.custom_theme = self.settings.default_custom_theme.clone();
                }
                self.new_doc(workbook);
            }
            Cmd::NewFromTemplate(id) => {
                if !templates::TEMPLATES
                    .iter()
                    .any(|template| template.id == id)
                {
                    self.toasts.error(t("Unknown template"));
                    return;
                }
                self.new_doc(templates::instantiate(id));
                self.settings.add_recent_template(id);
                self.save_settings();
                self.template_choice = None;
            }
            Cmd::Home => {
                if self.doc().is_some() {
                    self.home_return_to = self.doc().map(|doc| doc.key.clone());
                }
                self.template_choice = None;
                // The home screen is shown when no document is open; offer it as a "new" tab.
                self.active = self.docs.len();
            }
            Cmd::Open => {
                let exts: Vec<&str> = Format::IMPORTABLE
                    .iter()
                    .flat_map(|f| f.extensions().iter().copied())
                    .collect();
                if let Some(files) = files::pick_files(
                    files::Purpose::Open,
                    None,
                    &[(t("Mind maps"), &exts)],
                    self.settings.last_open_dir.as_deref(),
                    true,
                ) {
                    for f in files {
                        if let Some(dir) = f.parent() {
                            self.settings.last_open_dir = Some(dir.to_path_buf());
                        }
                        self.open_path(&f);
                    }
                }
            }
            Cmd::OpenPath(p) => self.open_path(&p),
            Cmd::Save => {
                self.save_active(false);
            }
            Cmd::SaveAs => {
                self.save_active(true);
            }
            Cmd::SaveAll => {
                self.begin_save_all(ctx, false);
            }
            Cmd::CloseTab(i) => self.request_close_tab(i),
            Cmd::NextDoc => {
                if !self.docs.is_empty() {
                    self.active = (self.active + 1) % self.docs.len();
                }
            }
            Cmd::Import => {
                let exts: Vec<&str> = Format::IMPORTABLE
                    .iter()
                    .flat_map(|f| f.extensions().iter().copied())
                    .collect();
                for f in files::pick_files(
                    files::Purpose::Import,
                    None,
                    &[(t("Import"), &exts)],
                    None,
                    false,
                )
                .unwrap_or_default()
                {
                    self.open_path(&f);
                }
            }
            Cmd::MergeFiles => {
                if self.doc().is_none() {
                    self.toasts.warning(t("Not possible here"));
                    return;
                }
                if let Some(paths) = files::pick_files(
                    files::Purpose::Merge,
                    Some(t("Merge Map Files…")),
                    &[("Thinoda / XMind", &["thinoda", "xmind"])],
                    None,
                    true,
                ) {
                    self.merge_files(&paths);
                }
            }
            Cmd::MaskedCopy => {
                if let Some(d) = self.doc_mut() {
                    d.view.commit_edit(&mut d.doc);
                    self.dialogs.masked_copy_owner =
                        Some((d.key.clone(), d.doc.active_index(), d.doc.state_id()));
                    self.dialogs.masked_copy = true;
                } else {
                    self.toasts.warning(t("Not possible here"));
                }
            }
            Cmd::MapShot => {
                if !self.command_available(&Cmd::MapShot) {
                    self.toasts.warning(t("Not possible here"));
                    return;
                }
                if let Some(d) = self.doc_mut() {
                    d.view.commit_edit(&mut d.doc);
                }
                self.map_shot.requested = true;
                ctx.request_repaint();
            }
            Cmd::Labels => self.begin_labels(ctx),
            Cmd::Print => self.begin_print(ctx),
            Cmd::Export(format) => {
                if let Some(doc) = self.doc_mut() {
                    doc.view.commit_edit(&mut doc.doc);
                }
                if self.doc().is_some_and(|d| {
                    thinoda_io::symbol_color_loss(d.doc.workbook(), format)
                        || (format == Format::XMind
                            && d.doc
                                .workbook()
                                .sheets
                                .iter()
                                .any(|s| s.topics.values().any(|t| !t.marker_colors.is_empty())))
                }) {
                    self.dialogs.symbol_export = Some((self.active, format));
                    self.dialogs.export_owner = self
                        .doc()
                        .map(|doc| (doc.key.clone(), doc.doc.active_index(), doc.doc.state_id()));
                } else {
                    self.export(format, false);
                }
            }
            Cmd::Settings => self.dialogs.settings = true,
            Cmd::About => self.dialogs.about = true,
            Cmd::Shortcuts => {
                if let Some(doc) = self.doc_mut() {
                    doc.view.commit_edit(&mut doc.doc);
                }
                self.dialogs.shortcut_browser = Default::default();
                self.dialogs.shortcuts = true;
            }
            Cmd::UserGuide => self.open_guide(false),
            Cmd::QuickStart => self.open_guide(true),
            Cmd::Feedback => ctx.open_url(egui::OpenUrl::new_tab(
                "https://github.com/AxelWol/Thinoda/issues/new/choose",
            )),
            Cmd::ProjectNews => ctx.open_url(egui::OpenUrl::new_tab(
                "https://github.com/AxelWol/Thinoda/commits",
            )),
            Cmd::Quit if files::is_web() => {
                self.toasts.info(t("Close the browser tab to quit"));
            }
            Cmd::Quit => ctx.send_viewport_cmd(ViewportCommand::Close),
            Cmd::ToggleOutliner => {
                self.mode = if self.mode == CenterMode::Map {
                    CenterMode::Outliner
                } else {
                    CenterMode::Map
                };
            }
            Cmd::ToggleZen => {
                self.zen = !self.zen;
                if self.zen {
                    self.toasts.info(t("Zen Mode: press Escape to exit"));
                }
            }
            Cmd::ToggleFormatPanel => {
                if self.doc().is_none() {
                    self.toasts
                        .info(t("Open or select a map to show the Format Panel"));
                } else {
                    self.settings.show_format_panel = !self.settings.show_format_panel
                        || self.sidebar_family != SidebarFamily::Format;
                    self.sidebar_family = SidebarFamily::Format;
                    self.save_settings();
                }
            }
            Cmd::ToggleSymbolsPanel => {
                if self.doc().is_none() {
                    self.toasts.info(t("Open or select a map to show Symbols"));
                } else {
                    self.settings.show_format_panel = !self.settings.show_format_panel
                        || self.sidebar_family != SidebarFamily::Symbols;
                    self.sidebar_family = SidebarFamily::Symbols;
                    self.save_settings();
                }
            }
            Cmd::ToggleStatusBar => {
                self.settings.show_status_bar = !self.settings.show_status_bar;
                self.save_settings();
            }
            Cmd::Find => {
                self.search.open = true;
                self.search.focus = true;
            }
            Cmd::FindReplace => {
                self.search.open = true;
                self.search.show_replace = true;
                self.search.focus = true;
            }
            Cmd::Palette => {
                if self.command_available(&Cmd::Palette) {
                    self.palette.toggle();
                } else {
                    self.toasts.warning(t("Not possible here"));
                }
            }
            Cmd::NewSheet => {
                if let Some(d) = self.doc_mut() {
                    d.view.commit_edit(&mut d.doc);
                    let n = d.doc.workbook().sheets.len() + 1;
                    let mut wb = templates::instantiate("mind-map");
                    let mut sheet = wb.sheets.remove(0);
                    sheet.title = format!("{} {n}", t("Sheet"));
                    let i = d.doc.add_sheet(sheet);
                    d.doc.set_active(i);
                    d.view.reset();
                }
            }
            Cmd::DuplicateSheet => {
                if let Some(d) = self.doc_mut() {
                    d.view.commit_edit(&mut d.doc);
                    let a = d.doc.active_index();
                    match d.doc.duplicate_sheet(a) {
                        Ok(i) => {
                            d.doc.set_active(i);
                            d.view.reset();
                        }
                        Err(e) => {
                            self.toasts
                                .error(format!("{}: {e}", t("Not possible here")));
                        }
                    }
                }
            }
            Cmd::DeleteSheet => {
                if let Some(d) = self.doc_mut() {
                    d.view.commit_edit(&mut d.doc);
                    let a = d.doc.active_index();
                    if d.doc.remove_sheet(a).is_ok() {
                        d.view.reset();
                    } else {
                        self.toasts.warning(t("Not possible here"));
                    }
                }
            }
            Cmd::RenameSheet => {
                if let Some(d) = self.doc_mut() {
                    d.view.commit_edit(&mut d.doc);
                    self.dialogs.rename_sheet = Some(RenameSheetDraft {
                        document_key: d.key.clone(),
                        revision: d.doc.state_id(),
                        index: d.doc.active_index(),
                        name: d.doc.sheet().title.clone(),
                        ..Default::default()
                    });
                }
            }
            Cmd::PreviousSheet | Cmd::NextSheet => {
                if let Some(d) = self.doc() {
                    let n = d.doc.workbook().sheets.len();
                    let current = d.doc.active_index();
                    let index = if cmd == Cmd::NextSheet {
                        (current + 1) % n
                    } else {
                        (current + n - 1) % n
                    };
                    self.switch_sheet(index);
                }
            }
            Cmd::CopyStyle | Cmd::PasteStyle | Cmd::ResetStyle => self.edit_style(cmd),
            Cmd::SaveQuickStyle => self.begin_quick_style(),
            Cmd::ApplyQuickStyle(style) => self.apply_quick_style(style),
            Cmd::EditCustomTheme => self.begin_theme_editor(),
            Cmd::ApplyCustomTheme(theme) => {
                self.apply_custom_theme(*theme);
            }
            Cmd::CopySheet => {
                if let Some(d) = self.doc_mut() {
                    d.view.commit_edit(&mut d.doc);
                    match thinoda_core::clipboard::encode_sheet(d.doc.sheet()) {
                        Ok(text) => {
                            ctx.copy_text(text);
                            self.toasts.info(t("Copied sheet to clipboard"));
                        }
                        Err(e) => {
                            self.toasts
                                .error(format!("{}: {e}", t("Could not copy sheet")));
                        }
                    }
                }
            }
            Cmd::PasteSheet => {
                if let Some(d) = self.doc_mut() {
                    d.view.commit_edit(&mut d.doc);
                    let key = d.key.clone();
                    self.paste_requested = false;
                    self.discard_late_sheet_paste = false;
                    self.sheet_paste_request = Some((Instant::now(), key));
                    ctx.send_viewport_cmd(ViewportCommand::RequestPaste);
                }
            }
            Cmd::SaveSheetAs => self.save_sheet_as(),
            Cmd::NewSheetFromTopic => {
                if !self.command_available(&cmd) {
                    self.toasts.warning(t("Not possible here"));
                    return;
                }
                let d = self.doc_mut().unwrap();
                d.view.commit_edit(&mut d.doc);
                let topic = d.view.primary().unwrap();
                match d.doc.sheet().copy_branch_as_sheet(topic) {
                    Ok(sheet) => {
                        let external = d
                            .doc
                            .sheet()
                            .relationships
                            .iter()
                            .filter(|r| sheet.contains(r.from) != sheet.contains(r.to))
                            .count();
                        d.doc.add_sheet(sheet);
                        d.view.reset();
                        if external > 0 {
                            self.toasts.info(t(
                                "External relationships remain only in the original sheet",
                            ));
                        }
                    }
                    Err(e) => {
                        self.toasts
                            .error(format!("{}: {e}", t("Not possible here")));
                    }
                }
            }
            Cmd::OpenLink => {
                let link = self.doc().and_then(|d| {
                    let id = d.view.primary()?;
                    d.doc.sheet().topic(id)?.hyperlink.clone()
                });
                if let Some(l) = link {
                    self.open_link(&l);
                }
            }
            Cmd::ShowInFolder => {
                if let Some(p) = self.doc().and_then(|d| d.path.clone())
                    && let Err(e) = system::reveal_in_file_manager(&p)
                    && files::is_web()
                {
                    self.toasts
                        .warning(format!("{}: {e}", t("Not possible here")));
                }
            }
            Cmd::MenuPaste => {
                self.discard_late_sheet_paste = false;
                self.paste_requested = true;
                ctx.send_viewport_cmd(ViewportCommand::RequestPaste);
            }
            Cmd::Ai(a) => self.start_ai(a),
            Cmd::EditField(f) => {
                if !self.command_available(&Cmd::EditField(f)) {
                    self.toasts.warning(t("Not possible here"));
                    return;
                }
                let d = self.doc_mut().unwrap();
                d.view.commit_edit(&mut d.doc);
                self.mode = CenterMode::Map;
                self.zen = false;
                self.settings.show_format_panel = true;
                self.sidebar_family = SidebarFamily::Format;
                self.format_tab = FormatTab::Style;
                self.focus_field = Some(f);
                self.save_settings();
            }
            Cmd::TopicStyle => {
                if self.doc().is_some_and(|d| !d.view.selection.is_empty()) {
                    self.zen = false;
                    self.settings.show_format_panel = true;
                    self.sidebar_family = SidebarFamily::Format;
                    self.format_tab = FormatTab::Style;
                    self.save_settings();
                }
            }
            Cmd::SetTheme(th) => {
                self.settings.theme = th;
                self.save_settings();
            }
        }
    }

    fn canvas_action(&mut self, ctx: &Context, action: Action) {
        let Some(d) = self.docs.get_mut(self.active) else {
            return;
        };
        d.view.commit_edit(&mut d.doc);
        if !action.available(&d.doc, &d.view) {
            self.toasts.warning(t("Not possible here"));
            return;
        }
        let starting_relationship = action == Action::StartRelationship;
        match apply_action(&mut d.doc, &mut d.view, action) {
            Ok(_) => {
                if starting_relationship && d.view.relationship_from.is_some() {
                    self.toasts.info(t("Pick a target topic, or click blank space to create a connected floating topic"));
                }
            }
            Err(e) => {
                log::debug!("action failed: {e}");
                self.toasts.warning(t("Not possible here"));
            }
        }
        if let Some(text) = d.view.clipboard_out.take() {
            ctx.copy_text(text);
        }
    }

    pub fn switch_sheet(&mut self, index: usize) {
        let Some(d) = self.doc_mut() else { return };
        if index >= d.doc.workbook().sheets.len() {
            self.toasts.warning(t("Not possible here"));
            return;
        }
        if index == d.doc.active_index() {
            return;
        }
        d.view.commit_edit(&mut d.doc);
        d.doc.set_active(index);
        d.view.reset();
    }

    pub fn command_available(&self, cmd: &Cmd) -> bool {
        match cmd {
            Cmd::Palette => {
                self.palette.open
                    || (!self.any_modal_open() && !self.dialogs.settings && !self.dialogs.about)
            }
            Cmd::Save | Cmd::SaveAs | Cmd::Export(_) => {
                self.save_all.is_none() && self.doc().is_some()
            }
            Cmd::SaveAll => self.save_all.is_none() && !self.docs.is_empty(),
            Cmd::Quit => self.save_all.is_none(),
            Cmd::CloseTab(index) => self.save_all.is_none() && *index < self.docs.len(),
            Cmd::Print => {
                self.doc().is_some()
                    && !self.any_modal_open()
                    && !self.dialogs.settings
                    && !self.dialogs.about
            }
            Cmd::Labels | Cmd::EditField(_) => {
                !self.any_modal_open()
                    && !self.dialogs.settings
                    && !self.dialogs.about
                    && !self.dialogs.shortcuts
                    && self.doc().is_some_and(|d| {
                        d.view.selection.len() == 1
                            && d.view.selected_object.is_none()
                            && d.view
                                .primary()
                                .is_some_and(|id| d.doc.sheet().contains(id))
                    })
            }
            Cmd::MapShot => {
                self.doc().is_some()
                    && self.mode == CenterMode::Map
                    && !self.map_shot.active()
                    && !self.any_modal_open()
                    && !self.dialogs.settings
                    && !self.dialogs.about
                    && !self.dialogs.shortcuts
            }
            Cmd::EditCustomTheme | Cmd::ApplyCustomTheme(_) => {
                !self.any_modal_open() && self.doc().is_some()
            }
            Cmd::Canvas(action) => self
                .doc()
                .is_some_and(|d| action.available(&d.doc, &d.view)),
            Cmd::SaveQuickStyle => {
                !self.any_modal_open()
                    && self.doc().is_some_and(|d| {
                        d.view.selected_object.is_none()
                            && d.view.selection.len() == 1
                            && d.view
                                .primary()
                                .is_some_and(|id| d.doc.sheet().contains(id))
                    })
            }
            Cmd::ApplyQuickStyle(_) => {
                !self.any_modal_open()
                    && self.doc().is_some_and(|d| {
                        d.view.selected_object.is_none()
                            && !d.view.selection.is_empty()
                            && d.view
                                .selection
                                .iter()
                                .all(|id| d.doc.sheet().contains(*id))
                    })
            }
            Cmd::CopyStyle | Cmd::NewSheetFromTopic => self.doc().is_some_and(|d| {
                d.view.selection.len() == 1
                    && d.view
                        .primary()
                        .is_some_and(|id| d.doc.sheet().contains(id))
            }),
            Cmd::PasteStyle | Cmd::ResetStyle => self.doc().is_some_and(|d| {
                !d.view.selection.is_empty()
                    && d.view
                        .selection
                        .iter()
                        .all(|id| d.doc.sheet().contains(*id))
                    && (*cmd != Cmd::PasteStyle || self.style_clipboard.is_some())
            }),
            Cmd::NextSheet | Cmd::PreviousSheet | Cmd::DeleteSheet => self
                .doc()
                .is_some_and(|d| d.doc.workbook().sheets.len() > 1),
            Cmd::NewSheet
            | Cmd::RenameSheet
            | Cmd::DuplicateSheet
            | Cmd::CopySheet
            | Cmd::PasteSheet
            | Cmd::SaveSheetAs
            | Cmd::MergeFiles
            | Cmd::MaskedCopy => self.doc().is_some(),
            _ => true,
        }
    }

    fn edit_style(&mut self, cmd: Cmd) {
        if !self.command_available(&cmd) {
            self.toasts.warning(t("Not possible here"));
            return;
        }
        if cmd == Cmd::CopyStyle {
            let d = self.doc().unwrap();
            self.style_clipboard = d
                .view
                .primary()
                .and_then(|id| d.doc.sheet().topic(id))
                .map(|topic| topic.style.clone());
            self.toasts.info(t("Copied topic style overrides"));
            return;
        }
        let style = if cmd == Cmd::PasteStyle {
            self.style_clipboard.clone().unwrap()
        } else {
            thinoda_core::TopicStyle::default()
        };
        let label = if cmd == Cmd::PasteStyle {
            "Paste Style"
        } else {
            "Reset Style"
        };
        let d = self.doc_mut().unwrap();
        d.view.commit_edit(&mut d.doc);
        let selection = d.view.selection.clone();
        if let Err(e) = d.doc.edit_sheet(label, |sheet| {
            sheet.update_style(&selection, |target| *target = style.clone())
        }) {
            self.toasts
                .error(format!("{}: {e}", t("Not possible here")));
        }
    }
    pub fn open_link(&mut self, link: &str) {
        if let Err(e) = system::open_url(link) {
            self.toasts.error(format!("{link}: {e}"));
        }
    }

    /// Remove the `OpenUrl` commands emitted this frame (hyperlinks, Help links) and
    /// return their targets. eframe is built without its `links` feature, so the
    /// native shell hands these to [`Self::open_link`] instead.
    pub fn take_open_urls(ctx: &egui::Context) -> Vec<String> {
        ctx.output_mut(|o| {
            let mut urls = Vec::new();
            o.commands.retain(|c| match c {
                egui::OutputCommand::OpenUrl(open) => {
                    urls.push(open.url.clone());
                    false
                }
                _ => true,
            });
            urls
        })
    }

    pub fn merge_files(&mut self, paths: &[PathBuf]) -> bool {
        if self.doc().is_none() || paths.is_empty() {
            self.toasts
                .warning(t("Select map files and open a destination document first"));
            return false;
        }
        let mut sheets = Vec::new();
        for path in paths {
            if !Format::from_path(path).is_some_and(is_native) {
                self.toasts.error(format!(
                    "{}: {}",
                    t("Choose .thinoda or .xmind files"),
                    path.display()
                ));
                return false;
            }
            match files::import_workbook(path) {
                Ok(workbook) => sheets.extend(workbook.sheets),
                Err(error) => {
                    self.toasts.error(format!(
                        "{}: {}: {error}",
                        t("Could not merge map files"),
                        path.display()
                    ));
                    return false;
                }
            }
        }
        let d = self.doc_mut().unwrap();
        d.view.commit_edit(&mut d.doc);
        match d.doc.merge_sheets(sheets) {
            Ok(_) => {
                d.view.reset();
                self.toasts
                    .success(t("Merged sheets; original files are unchanged"));
                true
            }
            Err(error) => {
                self.toasts
                    .error(format!("{}: {error}", t("Could not merge map files")));
                false
            }
        }
    }

    pub fn create_masked_copy(&mut self) {
        let Some(d) = self.doc_mut() else {
            self.toasts.warning(t("Not possible here"));
            return;
        };
        d.view.commit_edit(&mut d.doc);
        let workbook = d.doc.workbook().masked_copy();
        self.new_doc(workbook);
        self.doc_mut().unwrap().doc.mark_dirty();
        self.dialogs.masked_copy = false;
        self.dialogs.masked_copy_owner = None;
        self.toasts.info(t(
            "Masked copy created; inspect it before saving or sharing",
        ));
    }

    pub fn selection_commands() -> Vec<(Action, &'static str)> {
        vec![
            (Action::SelectAll, "Select All"),
            (Action::SelectNone, "Select None"),
            (Action::InvertSelection, "Invert Selection"),
            (Action::SelectSiblings, "Select Siblings"),
            (Action::SelectDescendants, "Select Descendants"),
            (Action::SelectRoot, "Central Topic"),
        ]
    }

    pub fn arrangement_commands() -> Vec<(Action, &'static str)> {
        vec![
            (Action::MoveUp, "Move Up"),
            (Action::MoveDown, "Move Down"),
            (Action::SortAscending, "Sort Children Ascending"),
            (Action::SortDescending, "Sort Children Descending"),
            (
                Action::AlignFloating(FloatingArrangement::Left),
                "Align Left",
            ),
            (
                Action::AlignFloating(FloatingArrangement::HorizontalCenter),
                "Align Horizontal Centers",
            ),
            (
                Action::AlignFloating(FloatingArrangement::Right),
                "Align Right",
            ),
            (Action::AlignFloating(FloatingArrangement::Top), "Align Top"),
            (
                Action::AlignFloating(FloatingArrangement::VerticalCenter),
                "Align Vertical Centers",
            ),
            (
                Action::AlignFloating(FloatingArrangement::Bottom),
                "Align Bottom",
            ),
            (
                Action::AlignFloating(FloatingArrangement::DistributeHorizontal),
                "Distribute Horizontal Centers",
            ),
            (
                Action::AlignFloating(FloatingArrangement::DistributeVertical),
                "Distribute Vertical Centers",
            ),
        ]
    }

    pub fn handle_sheet_paste(&mut self, ctx: &Context) {
        if self.discard_late_sheet_paste {
            let discarded = ctx.input_mut(|input| {
                if let Some(index) = input
                    .events
                    .iter()
                    .position(|event| matches!(event, egui::Event::Paste(_)))
                {
                    input.events.remove(index);
                    true
                } else {
                    false
                }
            });
            if discarded {
                self.discard_late_sheet_paste = false;
                self.toasts
                    .warning(t("Ignored a late sheet paste; paste again to retry"));
            }
            return;
        }
        if self.sheet_paste_request.is_none()
            && !ctx.text_edit_focused()
            && self.doc().is_some_and(|d| d.view.editing_topic().is_none())
            && !self.any_modal_open()
            && ctx.input(|input| {
                input.events.iter().any(|event| {
                matches!(event, egui::Event::Paste(text) if text.starts_with("THINODA-SHEET-"))
            })
            })
            && let Some(d) = self.doc()
        {
            self.sheet_paste_request = Some((Instant::now(), d.key.clone()));
        }
        let Some((requested, key)) = self.sheet_paste_request.take() else {
            return;
        };
        let text = ctx.input_mut(|input| {
            let index = input
                .events
                .iter()
                .position(|event| matches!(event, egui::Event::Paste(_)))?;
            match input.events.remove(index) {
                egui::Event::Paste(text) => Some(text),
                _ => unreachable!(),
            }
        });
        if self.doc().is_none_or(|d| d.key != key) || self.any_modal_open() {
            self.discard_late_sheet_paste = text.is_none();
            self.toasts.warning(t(
                "Sheet paste cancelled because the document changed or a dialog opened",
            ));
            return;
        }
        let Some(text) = text else {
            if requested.elapsed() < Duration::from_secs(2) {
                self.sheet_paste_request = Some((requested, key));
                ctx.request_repaint_after(Duration::from_millis(100));
            } else {
                self.discard_late_sheet_paste = true;
                self.toasts
                    .warning(t("Clipboard did not provide sheet data"));
            }
            return;
        };
        self.paste_requested = false;
        match thinoda_core::clipboard::decode_sheet(&text) {
            Ok(sheet) => {
                let d = self.doc_mut().unwrap();
                d.doc.add_sheet(sheet);
                d.view.reset();
            }
            Err(e) => {
                self.toasts
                    .error(format!("{}: {e}", t("Could not paste sheet")));
            }
        }
    }

    fn save_sheet_as(&mut self) {
        let Some(d) = self.doc_mut() else { return };
        d.view.commit_edit(&mut d.doc);
        let dir = d.path.as_ref().and_then(|path| path.parent());
        if let Some(path) = files::save_target(
            None,
            MAP_FILTERS,
            &format!("{}.thinoda", sanitize(&d.doc.sheet().title)),
            dir,
            files::Target::Document,
        ) {
            self.save_sheet_to(&path);
        }
    }

    pub fn save_sheet_to(&mut self, path: &Path) -> bool {
        if Format::from_path(path) == Some(Format::MindManager) {
            if let Some(doc) = self.doc() {
                self.dialogs.mindmanager_save = Some((doc.key.clone(), path.to_path_buf(), true));
            }
            return false;
        }
        self.save_sheet_to_confirmed(path)
    }

    pub(crate) fn save_sheet_to_confirmed(&mut self, path: &Path) -> bool {
        let backup = self.settings.backup_on_save;
        let Some(d) = self.doc_mut() else {
            return false;
        };
        d.view.commit_edit(&mut d.doc);
        match d.save_sheet_to(path, backup) {
            Ok(()) => {
                self.toasts
                    .success(format!("{} {}", t("Saved sheet to"), path.display()));
                self.deliver(path);
                true
            }
            Err(e) => {
                self.toasts
                    .error(format!("{}: {e}", t("Could not save sheet")));
                false
            }
        }
    }

    /// Saves the active document. Returns `false` if the user cancelled or saving failed.
    pub fn save_active(&mut self, save_as: bool) -> bool {
        if self.dialogs.mindmanager_save.is_some() {
            return false;
        }
        let backup = self.settings.backup_on_save;
        let default_dir = self
            .settings
            .cloud
            .preferred_folder
            .clone()
            .or(self.settings.last_open_dir.clone());
        let Some(d) = self.docs.get_mut(self.active) else {
            return false;
        };
        d.view.commit_edit(&mut d.doc);
        let (target, chosen) = match (&d.path, save_as) {
            (Some(p), false) if is_native(d.format) || d.format == Format::MindManager => {
                if d.changed_on_disk() && !d.ignore_conflict {
                    self.dialogs.conflict = Some(d.key.clone());
                    return false;
                }
                (p.clone(), false)
            }
            _ => {
                let dir = d
                    .path
                    .as_ref()
                    .and_then(|p| p.parent().map(Path::to_path_buf))
                    .or(default_dir);
                match files::save_target(
                    None,
                    MAP_FILTERS,
                    &format!("{}.thinoda", sanitize(&d.title())),
                    dir.as_deref(),
                    files::Target::Document,
                ) {
                    Some(p) => (p, true),
                    None => return false,
                }
            }
        };
        if Format::from_path(&target) == Some(Format::MindManager) {
            self.dialogs.mindmanager_save = Some((d.key.clone(), target, false));
            return false;
        }
        let saved = self.save_active_to(&target, backup);
        // Browser: a newly chosen file name is also downloaded; later saves stay in browser storage.
        if saved && chosen {
            self.deliver(&target);
        }
        saved
    }

    /// Web: downloads a written file (desktop: no-op).
    pub(crate) fn deliver(&mut self, path: &Path) {
        if let Err(e) = files::deliver(path) {
            self.toasts.error(format!("{}: {e}", t("Download failed")));
        }
    }

    pub(crate) fn save_active_to(&mut self, target: &Path, backup: bool) -> bool {
        let Some(d) = self.docs.get_mut(self.active) else {
            return false;
        };
        let same_file = match d.targets_current_file(target) {
            Ok(same_file) => same_file,
            Err(error) => {
                d.ignore_conflict = false;
                self.toasts
                    .error(format!("{}: {error}", t("Could not save")));
                return false;
            }
        };
        if same_file && d.changed_on_disk() && !d.ignore_conflict {
            self.dialogs.conflict = Some(d.key.clone());
            return false;
        }
        match d.save_to(target, backup) {
            Ok(()) => {
                if let Some(r) = &self.services.recovery {
                    let _ = r.remove(&d.key);
                }
                self.settings.add_recent(target);
                self.save_settings();
                self.toasts
                    .success(format!("{} {}", t("Saved to"), target.display()));
                true
            }
            Err(e) => {
                d.ignore_conflict = false;
                self.toasts.error(format!("{}: {e}", t("Could not save")));
                false
            }
        }
    }

    pub(crate) fn export(&mut self, format: Format, allow_symbol_color_loss: bool) {
        if format == Format::MindManager {
            self.dialogs.mindmanager_export = true;
            self.dialogs.mindmanager_symbol_color_loss = allow_symbol_color_loss;
            self.dialogs.export_owner = self
                .doc()
                .map(|doc| (doc.key.clone(), doc.doc.active_index(), doc.doc.state_id()));
            return;
        }
        self.export_confirmed(format, allow_symbol_color_loss);
    }

    pub(crate) fn export_confirmed(&mut self, format: Format, allow_symbol_color_loss: bool) {
        let dark = self
            .applied
            .is_some_and(|(th, ..)| th == SettingsTheme::Dark);
        let Some(d) = self.docs.get_mut(self.active) else {
            return;
        };
        d.view.commit_edit(&mut d.doc);
        let ext = format.extensions()[0];
        let Some(path) = files::save_target(
            None,
            &[(t(format.label()), format.extensions())],
            &format!("{}.{ext}", sanitize(&d.title())),
            d.path.as_ref().and_then(|p| p.parent()),
            files::Target::Download,
        ) else {
            return;
        };
        let opts = ExportOptions {
            allow_symbol_color_loss,
            dark: dark
                && matches!(
                    format,
                    Format::Png | Format::Jpeg | Format::Svg | Format::Pdf
                ),
            scale: 2.0,
            ..Default::default()
        };
        let res = thinoda_io::export_bytes_with_consent(
            d.doc.workbook(),
            format,
            &opts,
            format == Format::MindManager,
        )
        .map_err(crate::doc::export_error)
        .and_then(|b| {
            d.ensure_export_destination(&path)?;
            thinoda_platform::fs::atomic_write(&path, &b, false).map_err(|e| e.to_string())
        });
        match res {
            Ok(()) => {
                self.settings.last_export_format = Some(format.label().to_string());
                if files::is_web() {
                    self.deliver(&path);
                } else {
                    self.toasts
                        .success(format!("{} {}", t("Exported to"), path.display()));
                }
            }
            Err(e) => {
                self.toasts.error(format!("{}: {e}", t("Export failed")));
            }
        }
    }

    pub fn request_close_tab(&mut self, i: usize) {
        if self.save_all.is_some() {
            self.toasts.warning(t("Not possible here"));
            return;
        }
        if i >= self.docs.len() {
            return;
        }
        let d = &mut self.docs[i];
        d.view.commit_edit(&mut d.doc);
        if self.docs[i].is_dirty() {
            self.active = i;
            self.dialogs.close = Some(CloseIntent::Tab(self.docs[i].key.clone()));
        } else {
            self.close_tab_now(i);
        }
    }

    pub fn close_tab_now(&mut self, i: usize) {
        if i >= self.docs.len() {
            return;
        }
        let home = self.active >= self.docs.len();
        let d = self.docs.remove(i);
        if let Some(r) = &self.services.recovery {
            let _ = r.remove(&d.key);
        }
        if home {
            self.active = self.docs.len();
        } else if i < self.active {
            self.active -= 1;
        } else if self.active >= self.docs.len() {
            self.active = self.docs.len().saturating_sub(1);
        }
    }

    pub(crate) fn resolve_save_close_tab(&mut self, key: &str, saved: bool) {
        if self.dialogs.save_close_tab.as_deref() != Some(key) {
            return;
        }
        if !saved
            && (self.dialogs.conflict.as_deref() == Some(key)
                || self
                    .dialogs
                    .mindmanager_save
                    .as_ref()
                    .is_some_and(|(pending, _, sheet_only)| pending == key && !sheet_only))
        {
            return;
        }
        self.dialogs.save_close_tab = None;
        if saved
            && let Some(index) = self.docs.iter().position(|doc| doc.key == key)
            && !self.docs[index].is_dirty()
        {
            self.close_tab_now(index);
        }
    }

    pub fn any_dirty(&self) -> bool {
        self.docs.iter().any(|d| d.is_dirty())
    }

    // ----- per-frame housekeeping -------------------------------------------------------------

    /// Applies theme/accent/scale settings when they changed.
    pub fn apply_appearance(&mut self, ctx: &Context) {
        let accent = self.accent;
        let scale = (self.settings.ui_scale.clamp(0.5, 3.0) * 100.0) as u32;
        let key = (self.settings.theme, accent, scale);
        if self.applied != Some(key) {
            thinoda_ui::install(ctx, ui_theme(self.settings.theme), accent);
            if (ctx.zoom_factor() * 100.0) as u32 != scale && self.settings.ui_scale > 0.0 {
                ctx.set_zoom_factor(scale as f32 / 100.0);
            }
            self.applied = Some(key);
        }
        let dark = ctx.theme() == egui::Theme::Dark;
        for d in &mut self.docs {
            d.view.dark = dark;
            d.view.accent = accent;
        }
    }

    pub fn housekeeping(&mut self, ctx: &Context) {
        let dt = self.last_frame.elapsed().as_secs_f32();
        self.last_frame = Instant::now();
        self.toasts.tick(dt);
        files::set_repaint_context(ctx);
        for picked in files::take_picked() {
            for error in picked.errors {
                self.toasts
                    .error(format!("{}: {error}", t("Could not open")));
            }
            match picked.purpose {
                files::Purpose::Open | files::Purpose::Import => {
                    for path in &picked.paths {
                        self.open_path(path);
                    }
                }
                files::Purpose::Merge => {
                    self.merge_files(&picked.paths);
                }
            }
        }
        self.poll_ai(ctx);
        self.remember_map_defaults();

        for d in &mut self.docs {
            d.tick();
        }

        // Crash recovery snapshots.
        if let Some(r) = self.services.recovery.clone() {
            let interval = self.settings.recovery_interval_secs.max(1) as f32;
            for d in &mut self.docs {
                if d.idle_secs() >= interval.min(2.0) || d.idle_secs() >= interval {
                    d.snapshot(&r);
                }
            }
        }
        // Autosave of documents that already have a native file.
        if self.settings.autosave && self.save_all.is_none() {
            let interval = self.settings.autosave_interval_secs.max(5) as f32;
            let backup = self.settings.backup_on_save;
            for d in &mut self.docs {
                if d.is_dirty()
                    && d.path.is_some()
                    && is_native(d.format)
                    && d.view.editing_topic().is_none()
                    && d.idle_secs() >= interval
                    && !d.changed_on_disk()
                {
                    let p = d.path.clone().unwrap();
                    if let Err(e) = d.save_to(&p, backup) {
                        self.toasts.error(format!("{}: {e}", t("Could not save")));
                    } else if let Some(r) = &self.services.recovery {
                        let _ = r.remove(&d.key);
                    }
                }
            }
        }
        // Files forwarded from secondary instances (e.g. double-click in Explorer/Finder).
        if self.last_poll.elapsed() > Duration::from_millis(500) {
            self.last_poll = Instant::now();
            // The browser has no background flusher thread; this also refreshes the
            // recovery lock heartbeat of this tab.
            #[cfg(target_arch = "wasm32")]
            if let Some(r) = &self.services.recovery
                && let Err(e) = r.flush()
            {
                log::debug!("recovery flush: {e}");
            }
            let paths = self
                .services
                .instance
                .as_ref()
                .and_then(|i| i.poll().ok())
                .unwrap_or_default();
            if !paths.is_empty() {
                for p in paths {
                    self.open_path(&p);
                }
                ctx.send_viewport_cmd(ViewportCommand::Focus);
            }
        }

        // Window title.
        let title = match self.doc() {
            Some(d) => format!(
                "{}{} — Thinoda",
                if d.is_dirty() { "• " } else { "" },
                d.title()
            ),
            None => "Thinoda".to_string(),
        };
        if title != self.last_title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
        ctx.request_repaint_after(Duration::from_millis(1000));
    }

    fn remember_map_defaults(&mut self) {
        let Some(doc) = self.doc() else { return };
        let sheet = doc.doc.sheet();
        let structure = sheet.structure.xmind_class().to_owned();
        let theme = sheet.theme.theme_id.clone();
        let custom = sheet.theme.custom_theme.clone();
        let mut changed = false;
        if self.settings.reuse_last_structure && self.settings.default_structure != structure {
            self.settings.default_structure = structure;
            changed = true;
        }
        if self.settings.reuse_last_theme && self.settings.default_theme != theme {
            self.settings.default_theme = theme;
            changed = true;
        }
        if self.settings.reuse_last_theme && self.settings.default_custom_theme != custom {
            self.settings.default_custom_theme = custom;
            changed = true;
        }
        if changed {
            self.save_settings();
        }
    }

    /// Handles a window close request (asks to save dirty documents).
    pub fn handle_close_request(&mut self, ctx: &Context) {
        if ctx.input(|i| i.viewport().close_requested()) {
            for doc in &mut self.docs {
                if let Err(error) = doc.flush_inspector_edit(ctx) {
                    log::warn!("inspector commit before window close: {error}");
                    self.toasts.error(error);
                    ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                    return;
                }
            }
            if self.save_all.is_some() {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                return;
            }
            for d in &mut self.docs {
                d.view.commit_edit(&mut d.doc);
            }
            if self.quitting || !self.any_dirty() {
                self.shutdown();
            } else {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                self.dialogs.close = Some(CloseIntent::Quit);
            }
        }
    }

    /// Clean shutdown: removes recovery data of saved documents.
    pub fn shutdown(&mut self) {
        self.save_settings();
        if let Some(r) = self.services.recovery.take() {
            for d in &self.docs {
                if !d.is_dirty() {
                    let _ = r.remove(&d.key);
                }
            }
            if !self.any_dirty() {
                let _ = r.finish();
            } else {
                let _ = r.flush();
            }
        }
    }

    pub fn restore_orphans(&mut self, index: Option<usize>) {
        let sessions = std::mem::take(&mut self.dialogs.recovery);
        let mut keep = Vec::new();
        for (i, s) in sessions.into_iter().enumerate() {
            if index.is_some_and(|x| x != i) {
                keep.push(s);
                continue;
            }
            let workbooks = s
                .docs
                .iter()
                .map(|doc| {
                    doc.load().map_err(|error| {
                        format!("{} {}: {error}", t("Could not open"), doc.meta.title)
                    })
                })
                .collect::<Result<Vec<_>, _>>();
            match workbooks {
                Ok(workbooks) => {
                    let mut restored = Vec::new();
                    for (doc, wb) in s.docs.iter().zip(workbooks) {
                        let mut d = OpenDoc::new(wb);
                        d.view.titles = new_topic_titles();
                        d.path = doc
                            .meta
                            .original_path
                            .clone()
                            .filter(|p| thinoda_platform::fs::exists(p));
                        d.import_source = doc.meta.import_source.clone();
                        if let Some(p) = &d.path {
                            d.format = Format::from_path(p).unwrap_or(Format::Thinoda);
                            d.fingerprint = thinoda_platform::fs::fingerprint(p).ok();
                        }
                        d.doc.mark_dirty();
                        restored.push(d);
                    }
                    if let Some(recovery) = &self.services.recovery {
                        let persisted =
                            restored.iter().zip(&s.docs).try_for_each(|(doc, source)| {
                                let mut meta = source.meta.clone();
                                meta.original_path = doc.path.clone();
                                meta.dirty = true;
                                recovery.persist_snapshot(&doc.key, meta, doc.doc.workbook())
                            });
                        if let Err(error) = persisted {
                            log::warn!("Recovery handoff failed: {error}");
                            self.toasts
                                .error(format!("{}: {error}", t("Recover Unsaved Work")));
                            for doc in &restored {
                                if let Err(error) = recovery.remove(&doc.key) {
                                    log::warn!("Recovery handoff rollback failed: {error}");
                                }
                            }
                            keep.push(s);
                            continue;
                        }
                    } else {
                        log::warn!("Recovery handoff unavailable; retaining original snapshots");
                        self.toasts.warning(format!(
                            "{}: {}",
                            t("Could not save"),
                            t("Recover Unsaved Work")
                        ));
                    }
                    for (doc, source) in restored.into_iter().zip(&s.docs) {
                        self.docs.push(doc);
                        self.active = self.docs.len() - 1;
                        self.toasts
                            .success(format!("{}: {}", t("Restored"), source.meta.title));
                    }
                }
                Err(message) => {
                    log::warn!("Recovery restore failed: {message}");
                    self.toasts.error(message);
                    keep.push(s);
                    continue;
                }
            }
            // Never delete the last durable copy before the replacement session owns one.
            if self.services.recovery.is_some()
                && let Err(error) = s.discard()
            {
                log::warn!("Restored recovery cleanup failed: {error}");
                self.toasts
                    .error(format!("{}: {error}", t("Recover Unsaved Work")));
            }
        }
        self.dialogs.recovery = keep;
    }

    pub fn discard_orphans(&mut self) {
        for mut session in std::mem::take(&mut self.dialogs.recovery) {
            session.docs.retain(|doc| match doc.discard() {
                Ok(()) => false,
                Err(error) => {
                    log::warn!("Recovery discard failed for {}: {error}", doc.doc_key);
                    self.toasts.error(format!("{}: {error}", t("Discard All")));
                    true
                }
            });
            if !session.docs.is_empty() {
                self.dialogs.recovery.push(session);
            }
        }
    }

    /// Global keyboard shortcuts that work regardless of canvas focus.
    pub fn global_shortcuts(&mut self, ctx: &Context) {
        for (sc, c) in Self::global_shortcut_bindings(self.active) {
            if matches!(c, Cmd::CopyStyle | Cmd::PasteStyle) && ctx.text_edit_focused() {
                continue;
            }
            if ctx.input_mut(|i| i.consume_shortcut(&sc)) {
                self.exec(ctx, c);
                return;
            }
        }
        if self.zen
            && !self.search.open
            && !self.dialogs.settings
            && !ctx.text_edit_focused()
            && !ctx.input(|input| input.pointer.any_down())
            && self.doc().is_none_or(|doc| {
                doc.view.editing_topic().is_none() && doc.view.relationship_from.is_none()
            })
            && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
        {
            self.zen = false;
        }
        // Drag & drop of files onto the window.
        for p in files::dropped_files(ctx) {
            self.open_path(&p);
        }
    }

    /// Ordered dispatch bindings, also used by command-palette and shortcut hints.
    pub fn global_shortcut_bindings(active: usize) -> Vec<(KeyboardShortcut, Cmd)> {
        let cmd = Modifiers::COMMAND;
        let cs = Modifiers::COMMAND.plus(Modifiers::SHIFT);
        vec![
            (
                KeyboardShortcut::new(Modifiers::NONE, Key::F7),
                Cmd::MapShot,
            ),
            (KeyboardShortcut::new(cs, Key::S), Cmd::SaveAs),
            (KeyboardShortcut::new(cmd, Key::S), Cmd::Save),
            (KeyboardShortcut::new(cs, Key::P), Cmd::Palette),
            (KeyboardShortcut::new(cmd, Key::P), Cmd::Print),
            (
                KeyboardShortcut::new(cs, Key::I),
                Cmd::Ai(crate::ai::AiCommand::Brainstorm),
            ),
            (
                KeyboardShortcut::new(cs, Key::N),
                Cmd::EditField(FocusField::Notes),
            ),
            (
                KeyboardShortcut::new(cmd, Key::K),
                Cmd::EditField(FocusField::Hyperlink),
            ),
            (KeyboardShortcut::new(cmd, Key::N), Cmd::New),
            (KeyboardShortcut::new(cs, Key::O), Cmd::ToggleOutliner),
            (KeyboardShortcut::new(cmd, Key::O), Cmd::Open),
            (KeyboardShortcut::new(cmd, Key::W), Cmd::CloseTab(active)),
            (KeyboardShortcut::new(cmd, Key::Q), Cmd::Quit),
            (KeyboardShortcut::new(cmd, Key::F), Cmd::Find),
            (KeyboardShortcut::new(cmd, Key::H), Cmd::FindReplace),
            (KeyboardShortcut::new(cmd, Key::Comma), Cmd::Settings),
            (KeyboardShortcut::new(cmd, Key::Tab), Cmd::NextDoc),
            (KeyboardShortcut::new(cmd, Key::PageUp), Cmd::PreviousSheet),
            (KeyboardShortcut::new(cmd, Key::PageDown), Cmd::NextSheet),
            (
                KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::ALT), Key::C),
                Cmd::CopyStyle,
            ),
            (
                KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::ALT), Key::V),
                Cmd::PasteStyle,
            ),
            (
                KeyboardShortcut::new(Modifiers::NONE, Key::F11),
                Cmd::ToggleZen,
            ),
            (KeyboardShortcut::new(cs, Key::R), Cmd::ToggleFormatPanel),
            (
                KeyboardShortcut::new(Modifiers::NONE, Key::F1),
                Cmd::Shortcuts,
            ),
        ]
    }

    /// Recomputes search hits when the query or document changed.
    pub fn update_search(&mut self) {
        let Some(d) = self.docs.get_mut(self.active) else {
            self.search.hits.clear();
            self.search.current = 0;
            return;
        };
        let o = &self.search.opts;
        let key = (
            d.key.clone(),
            if self.search.all_sheets {
                usize::MAX
            } else {
                d.doc.active_index()
            },
            self.search.query.clone(),
            d.doc.state_id(),
            o.case_sensitive,
            o.whole_word,
            o.include_notes,
            o.include_labels,
            self.search.all_sheets,
        );
        if key == self.search.last_key {
            return;
        }
        self.search.last_key = key;
        self.search.hits = if self.search.query.is_empty() {
            Vec::new()
        } else if self.search.all_sheets {
            thinoda_core::search::search_workbook(
                d.doc.workbook(),
                &self.search.query,
                &self.search.opts,
            )
        } else {
            thinoda_core::search::search_sheet(
                d.doc.sheet(),
                d.doc.active_index(),
                &self.search.query,
                &self.search.opts,
            )
        };
        self.search.current = 0;
        self.apply_search_filter();
    }

    pub fn apply_search_filter(&mut self) {
        for d in &mut self.docs {
            d.view.filter = None;
        }
        if let Some(d) = self.docs.get_mut(self.active) {
            d.view.filter = (self.search.open && self.search.dim && !self.search.query.is_empty())
                .then(|| {
                    self.search
                        .hits
                        .iter()
                        .filter(|h| h.sheet == d.doc.active_index())
                        .map(|h| h.topic)
                        .collect()
                });
        }
    }

    /// Selects the next (`+1`) or previous (`-1`) search hit, expanding collapsed ancestors.
    pub fn goto_hit(&mut self, delta: isize) {
        if let Some(d) = self.doc_mut() {
            d.view.commit_edit(&mut d.doc);
        }
        self.update_search();
        let n = self.search.hits.len();
        if n == 0 {
            return;
        }
        self.search.current =
            ((self.search.current as isize + delta).rem_euclid(n as isize)) as usize;
        let hit = &self.search.hits[self.search.current];
        let (sheet, id) = (hit.sheet, hit.topic);
        let Some(d) = self.docs.get_mut(self.active) else {
            return;
        };
        if d.doc.active_index() != sheet {
            d.doc.set_active(sheet);
            d.view.reset();
        }
        let mut hidden = Vec::new();
        let mut p = d.doc.sheet().parent(id);
        while let Some(x) = p {
            if d.doc.sheet().topic(x).is_some_and(|t| t.collapsed) {
                hidden.push(x);
            }
            p = d.doc.sheet().parent(x);
        }
        if !hidden.is_empty()
            && let Err(e) = d.doc.edit_sheet("Expand", |s| {
                for h in &hidden {
                    s.set_collapsed(*h, false)?;
                }
                Ok(())
            })
        {
            self.toasts
                .error(format!("{}: {e}", t("Not possible here")));
            return;
        }
        d.view.select_only(id);
        d.view.reveal(id);
        // Expanding a result changes document state, but must not reset result navigation.
        self.search.last_key.3 = d.doc.state_id();
        self.apply_search_filter();
    }

    pub fn replace_all(&mut self) {
        if let Some(d) = self.doc_mut() {
            d.view.commit_edit(&mut d.doc);
        }
        self.update_search();
        let Some(d) = self.docs.get_mut(self.active) else {
            return;
        };
        let hits = self.search.hits.clone();
        let (q, r, o) = (
            self.search.query.clone(),
            self.search.replace.clone(),
            self.search.opts.clone(),
        );
        let res = d.doc.edit_book("Replace", |wb| {
            let mut total = 0;
            for (index, sheet) in wb.sheets.iter_mut().enumerate() {
                let topics: Vec<_> = hits
                    .iter()
                    .filter(|h| h.sheet == index)
                    .map(|h| h.topic)
                    .collect();
                total += thinoda_core::search::replace_in_titles(sheet, &topics, &q, &r, &o);
            }
            Ok(total)
        });
        match res {
            Ok(n) => self.toasts.info(format!("{} × {n}", t("Replace"))),
            Err(e) => self
                .toasts
                .error(format!("{}: {e}", t("Not possible here"))),
        };
        self.update_search();
    }

    /// The live catalog resolves active-document commands and platform modifier names.
    pub fn command_catalog(&self, ctx: &Context) -> Vec<(Cmd, thinoda_ui::Command)> {
        Self::palette_commands()
            .into_iter()
            .map(|(mut command, mut metadata)| {
                if matches!(command, Cmd::CloseTab(_)) {
                    command = Cmd::CloseTab(self.active);
                }
                metadata.shortcut = Self::shortcut_hint_for_mode(ctx, &command, self.mode);
                (command, metadata)
            })
            .collect()
    }

    /// Clipboard metadata describes OS events; it does not add key dispatch.
    pub fn shortcut_binding(command: &Cmd) -> Option<KeyboardShortcut> {
        Self::shortcut_binding_for_mode(command, CenterMode::Map)
    }

    pub fn shortcut_binding_for_mode(command: &Cmd, mode: CenterMode) -> Option<KeyboardShortcut> {
        Self::shortcut_binding_for_platform(command, mode, egui::os::OperatingSystem::Windows)
    }

    pub fn shortcut_binding_for_platform(
        command: &Cmd,
        mode: CenterMode,
        os: egui::os::OperatingSystem,
    ) -> Option<KeyboardShortcut> {
        static MAP: std::sync::OnceLock<Vec<(KeyboardShortcut, Cmd)>> = std::sync::OnceLock::new();
        static OUTLINE: std::sync::OnceLock<Vec<(KeyboardShortcut, Cmd)>> =
            std::sync::OnceLock::new();
        let defaults = match mode {
            CenterMode::Map => &MAP,
            CenterMode::Outliner => &OUTLINE,
        };
        let bindings = defaults.get_or_init(|| {
            let mut bindings = Self::global_shortcut_bindings(0);
            bindings.extend(
                Self::contextual_shortcut_bindings_for_mode(mode)
                    .into_iter()
                    .map(|(shortcut, action)| (shortcut, Cmd::Canvas(action))),
            );
            if mode == CenterMode::Map {
                for (key, command) in [
                    (Key::X, Cmd::Canvas(Action::Cut)),
                    (Key::C, Cmd::Canvas(Action::Copy)),
                    (Key::V, Cmd::MenuPaste),
                ] {
                    bindings.push((KeyboardShortcut::new(Modifiers::COMMAND, key), command));
                }
            }
            bindings
        });
        let normalized = match command {
            Cmd::CloseTab(_) => &Cmd::CloseTab(0),
            Cmd::Canvas(Action::Paste(_)) => &Cmd::MenuPaste,
            _ => command,
        };
        bindings
            .iter()
            .find(|(shortcut, bound)| {
                bound == normalized
                    && !Self::default_shortcut_command(mode, os, *shortcut)
                        .is_some_and(|dispatched| dispatched != *normalized)
            })
            .map(|(binding, _)| *binding)
    }

    pub fn shortcut_hint(ctx: &Context, command: &Cmd) -> Option<String> {
        Self::shortcut_hint_for_mode(ctx, command, CenterMode::Map)
    }

    pub fn shortcut_hint_for_mode(
        ctx: &Context,
        command: &Cmd,
        mode: CenterMode,
    ) -> Option<String> {
        Self::shortcut_binding_for_platform(command, mode, ctx.os())
            .map(|binding| ctx.format_shortcut(&binding))
    }

    /// Platform-neutral catalog with default non-macOS shortcut names.
    pub fn palette_commands() -> Vec<(Cmd, thinoda_ui::Command)> {
        use thinoda_ui::Command as C;
        let mut v: Vec<(Cmd, &str, &str, &str)> = vec![
            (Cmd::New, "New", "", "File"),
            (Cmd::Open, "Open…", "", "File"),
            (Cmd::Quit, "Quit", "", "File"),
            (Cmd::Save, "Save", "", "File"),
            (Cmd::SaveAs, "Save As…", "", "File"),
            (Cmd::SaveAll, "Save All", "", "File"),
            (Cmd::Import, "Import", "", "File"),
            (Cmd::Settings, "Settings…", "", "Edit"),
            (Cmd::Canvas(Action::InsertChild), "Subtopic", "", "Insert"),
            (Cmd::Canvas(Action::InsertSibling), "Topic", "", "Insert"),
            (
                Cmd::Canvas(Action::InsertSiblingBefore),
                "Topic Before",
                "",
                "Insert",
            ),
            (
                Cmd::Canvas(Action::InsertParent),
                "Parent Topic",
                "",
                "Insert",
            ),
            (
                Cmd::Canvas(Action::InsertFloating(thinoda_core::Vec2::new(0.0, -260.0))),
                "Floating Topic",
                "",
                "Insert",
            ),
            (
                Cmd::Canvas(Action::StartRelationship),
                "Relationship",
                "",
                "Insert",
            ),
            (Cmd::Canvas(Action::AddSummary), "Summary", "", "Insert"),
            (Cmd::Canvas(Action::AddBoundary), "Boundary", "", "Insert"),
            (
                Cmd::EditField(FocusField::Hyperlink),
                "Hyperlink",
                "",
                "Insert",
            ),
            (Cmd::EditField(FocusField::Notes), "Notes", "", "Insert"),
            (Cmd::Canvas(Action::ToggleTodo), "To-Do", "", "Insert"),
            (Cmd::Canvas(Action::Undo), "Undo", "", "Edit"),
            (Cmd::Canvas(Action::Redo), "Redo", "", "Edit"),
            (Cmd::Canvas(Action::Delete), "Delete", "", "Edit"),
            (
                Cmd::Canvas(Action::DeleteKeepChildren),
                "Delete Topic Only",
                "",
                "Edit",
            ),
            (
                Cmd::Canvas(Action::SelectRoot),
                "Go to Central Topic",
                "",
                "Edit",
            ),
            (Cmd::Canvas(Action::Duplicate), "Duplicate", "", "Edit"),
            (Cmd::Canvas(Action::SelectAll), "Select All", "", "Edit"),
            (Cmd::Find, "Find…", "", "Edit"),
            (Cmd::FindReplace, "Find and Replace…", "", "Edit"),
            (Cmd::CopyStyle, "Copy Style", "", "Edit"),
            (Cmd::PasteStyle, "Paste Style", "", "Edit"),
            (Cmd::ResetStyle, "Reset Style", "", "Edit"),
            (Cmd::SaveQuickStyle, "Save Quick Style…", "", "Style"),
            (Cmd::EditCustomTheme, "Edit Custom Theme…", "", "Style"),
            (Cmd::Canvas(Action::Indent), "Indent", "", "Edit"),
            (Cmd::Canvas(Action::Outdent), "Outdent", "", "Edit"),
            (Cmd::PreviousSheet, "Previous Sheet", "", "View"),
            (Cmd::NextSheet, "Next Sheet", "", "View"),
            (Cmd::CopySheet, "Copy Sheet", "", "File"),
            (Cmd::PasteSheet, "Paste Sheet", "", "File"),
            (Cmd::SaveSheetAs, "Save Sheet As…", "", "File"),
            (Cmd::MergeFiles, "Merge Map Files…", "", "File"),
            (Cmd::MaskedCopy, "Create Masked Copy…", "", "File"),
            (Cmd::MapShot, "Map Shot", "", "Tools"),
            (Cmd::Print, "Print", "", "File"),
            (Cmd::Labels, "Labels", "", "Insert"),
            (Cmd::NewSheetFromTopic, "New Sheet from Topic", "", "Edit"),
            (
                Cmd::Ai(crate::ai::AiCommand::Brainstorm),
                "AI: Brainstorm Subtopics",
                "",
                "AI",
            ),
            (
                Cmd::Ai(crate::ai::AiCommand::Breakdown),
                "AI: Break Down into To-Dos",
                "",
                "AI",
            ),
            (
                Cmd::Ai(crate::ai::AiCommand::Explain),
                "AI: Explain Topic",
                "",
                "AI",
            ),
            (
                Cmd::Ai(crate::ai::AiCommand::Summarize),
                "AI: Summarize Branch",
                "",
                "AI",
            ),
            (
                Cmd::Ai(crate::ai::AiCommand::GenerateMap),
                "AI: Generate Map…",
                "",
                "AI",
            ),
            (Cmd::ToggleOutliner, "Outliner", "", "View"),
            (Cmd::ToggleZen, "Zen Mode", "", "View"),
            (Cmd::ToggleFormatPanel, "Format Panel", "", "View"),
            (Cmd::ToggleStatusBar, "Status Bar", "", "View"),
            (Cmd::ToggleSymbolsPanel, "Symbols", "", "View"),
            (Cmd::Canvas(Action::ZoomIn), "Zoom In", "", "View"),
            (Cmd::Canvas(Action::ZoomOut), "Zoom Out", "", "View"),
            (Cmd::Canvas(Action::ZoomReset), "Actual Size", "", "View"),
            (Cmd::Canvas(Action::Fit), "Fit Map", "", "View"),
            (Cmd::Canvas(Action::ExpandAll), "Expand All", "", "View"),
            (Cmd::Canvas(Action::CollapseAll), "Collapse All", "", "View"),
            (
                Cmd::Canvas(Action::ToggleCollapse),
                "Collapse / Expand",
                "",
                "View",
            ),
            (
                Cmd::Canvas(Action::ToggleCollapseAll),
                "Collapse / Expand All",
                "",
                "View",
            ),
            (Cmd::NewSheet, "New Sheet", "", "Sheet"),
            (Cmd::DuplicateSheet, "Duplicate Sheet", "", "Sheet"),
            (Cmd::RenameSheet, "Rename Sheet", "", "Sheet"),
            (Cmd::DeleteSheet, "Delete Sheet", "", "Sheet"),
            (
                Cmd::SetTheme(SettingsTheme::System),
                "System",
                "",
                "Appearance",
            ),
            (
                Cmd::SetTheme(SettingsTheme::Light),
                "Light",
                "",
                "Appearance",
            ),
            (Cmd::SetTheme(SettingsTheme::Dark), "Dark", "", "Appearance"),
            (Cmd::Shortcuts, "Keyboard Shortcuts", "", "Help"),
            (Cmd::UserGuide, "User Guide", "", "Help"),
            (Cmd::QuickStart, "Quick Start", "", "Help"),
            (Cmd::Feedback, "Send Feedback…", "", "Help"),
            (Cmd::ProjectNews, "Project News…", "", "Help"),
            (Cmd::About, "About Thinoda", "", "Help"),
        ];
        for f in EXPORT_MENU {
            v.push((Cmd::Export(*f), f.label(), "", "Export"));
        }
        for (action, label) in Self::selection_commands()
            .into_iter()
            .chain(Self::arrangement_commands())
        {
            if !v
                .iter()
                .any(|(command, ..)| *command == Cmd::Canvas(action.clone()))
            {
                v.push((Cmd::Canvas(action), label, "", "Edit"));
            }
        }
        v.extend([
            (Cmd::CloseTab(0), "Close Tab", "", "File"),
            (Cmd::NextDoc, "Next Document", "", "View"),
            (Cmd::Palette, "Command Palette…", "", "Edit"),
        ]);
        v.into_iter()
            .enumerate()
            .map(|(i, (cmd, title, sc, cat))| {
                let title = if cat == "Export" {
                    format!("{} → {title}", t("Export"))
                } else {
                    t(title).to_string()
                };
                let mut c = C::new(i.to_string(), title).category(t(cat));
                if let Some(binding) = Self::shortcut_binding(&cmd) {
                    c = c.shortcut(binding.format(&egui::ModifierNames::NAMES, false));
                } else if !sc.is_empty() {
                    c = c.shortcut(sc);
                }
                (cmd, c)
            })
            .collect()
    }

    /// Additional canvas shortcuts not handled by the canvas widget itself.
    pub fn insert_shortcuts(&mut self, ctx: &Context) {
        if self.doc().is_none_or(|d| d.view.editing_topic().is_some()) || ctx.text_edit_focused() {
            return;
        }
        for (shortcut, action) in Self::insertion_shortcut_bindings() {
            if ctx.input_mut(|i| i.consume_shortcut(&shortcut)) {
                self.canvas_action(ctx, action);
                return;
            }
        }
    }

    pub fn insertion_shortcut_bindings() -> Vec<(KeyboardShortcut, Action)> {
        let cs = Modifiers::COMMAND.plus(Modifiers::SHIFT);
        [
            (cs, Key::L, Action::StartRelationship),
            (cs, Key::B, Action::AddBoundary),
            (Modifiers::COMMAND, Key::B, Action::AddBoundary),
            (
                Modifiers::COMMAND.plus(Modifiers::ALT),
                Key::Slash,
                Action::ToggleCollapseAll,
            ),
            (Modifiers::COMMAND, Key::CloseBracket, Action::AddSummary),
            (
                Modifiers::COMMAND,
                Key::Num1,
                Action::ToggleMarker("priority-1".into()),
            ),
            (
                Modifiers::COMMAND,
                Key::Num2,
                Action::ToggleMarker("priority-2".into()),
            ),
            (
                Modifiers::COMMAND,
                Key::Num3,
                Action::ToggleMarker("priority-3".into()),
            ),
        ]
        .into_iter()
        .map(|(modifiers, key, action)| (KeyboardShortcut::new(modifiers, key), action))
        .collect()
    }

    /// Display inventory only: each dispatcher keeps its existing focus and modal guards.
    pub fn contextual_shortcut_bindings() -> Vec<(KeyboardShortcut, Action)> {
        Self::contextual_shortcut_bindings_for_mode(CenterMode::Map)
    }

    pub fn outline_shortcut_bindings() -> Vec<(KeyboardShortcut, Action)> {
        [
            (Modifiers::COMMAND, Key::Y, Action::Redo),
            (Modifiers::COMMAND | Modifiers::SHIFT, Key::Z, Action::Redo),
            (Modifiers::COMMAND, Key::Z, Action::Undo),
            (Modifiers::NONE, Key::Enter, Action::InsertSibling),
            (Modifiers::NONE, Key::Tab, Action::InsertChild),
            (Modifiers::NONE, Key::Delete, Action::Delete),
            (Modifiers::NONE, Key::F2, Action::StartEdit),
            (Modifiers::ALT, Key::ArrowUp, Action::MoveUp),
            (Modifiers::ALT, Key::ArrowDown, Action::MoveDown),
        ]
        .into_iter()
        .map(|(modifiers, key, action)| (KeyboardShortcut::new(modifiers, key), action))
        .collect()
    }

    /// These keys traverse visible Outline rows, not canvas geometry.
    pub fn outline_navigation_bindings() -> [KeyboardShortcut; 2] {
        [Key::ArrowUp, Key::ArrowDown].map(|key| KeyboardShortcut::new(Modifiers::NONE, key))
    }

    pub fn contextual_shortcut_bindings_for_mode(
        mode: CenterMode,
    ) -> Vec<(KeyboardShortcut, Action)> {
        let mut bindings = match mode {
            CenterMode::Map => thinoda_canvas::shortcut_bindings(),
            CenterMode::Outliner => {
                let mut bindings = Self::outline_shortcut_bindings();
                bindings.extend(
                    Self::outline_navigation_bindings()
                        .into_iter()
                        .zip([
                            thinoda_core::navigation::Direction::Up,
                            thinoda_core::navigation::Direction::Down,
                        ])
                        .map(|(shortcut, direction)| (shortcut, Action::Navigate(direction))),
                );
                bindings
            }
        };
        for binding in Self::insertion_shortcut_bindings() {
            if !bindings.contains(&binding) {
                bindings.push(binding);
            }
        }
        bindings
    }
}

/// Localized default titles for new topics.
pub fn new_topic_titles() -> thinoda_canvas::NewTopicTitles {
    thinoda_canvas::NewTopicTitles {
        main: t("Main Topic").into(),
        sub: t("Subtopic").into(),
        floating: t("Floating Topic").into(),
        summary: t("Summary").into(),
    }
}

/// Makes a string safe to use as a file name.
pub fn sanitize(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| {
            if c.is_control() || r#"<>:"/\|?*"#.contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_end_matches('.').to_string();
    if cleaned.is_empty() {
        "Untitled".into()
    } else {
        cleaned.chars().take(120).collect()
    }
}

/// Services of the real application plus startup information.
pub struct Startup {
    pub settings: Settings,
    pub services: Services,
    pub warning: Option<String>,
    pub orphans: Vec<OrphanSession>,
}

/// Creates the services used by the real application (settings dir, recovery, single instance).
pub fn start_services(paths: Paths, instance: Option<SingleInstance>) -> Startup {
    let _ = paths.ensure();
    let (settings, warning) = match Settings::load_with_warning(&paths) {
        Ok(l) => (l.settings, l.warning),
        Err(e) => (Settings::default(), Some(e.to_string())),
    };
    let orphans = RecoveryManager::find_orphans(&paths);
    let recovery = match RecoveryManager::start(&paths) {
        Ok(r) => Some(Arc::new(r)),
        Err(e) => {
            log::error!("crash recovery unavailable: {e}");
            None
        }
    };
    thinoda_platform::crash::install_panic_hook(paths.clone(), recovery.clone());
    // Browser: no threads; `housekeeping` flushes snapshots instead.
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(r) = recovery.clone() {
        // Flush queued snapshots off the UI thread.
        let weak = Arc::downgrade(&r);
        drop(r);
        std::thread::Builder::new()
            .name("thinoda-recovery".into())
            .spawn(move || {
                while let Some(r) = weak.upgrade() {
                    if let Err(e) = r.flush() {
                        log::debug!("recovery flush: {e}");
                    }
                    drop(r);
                    std::thread::sleep(Duration::from_millis(750));
                }
            })
            .ok();
    }
    let services = Services {
        instance: instance.filter(SingleInstance::is_primary),
        cloud: detect_cloud_folders(),
        recovery,
        paths: Some(paths),
    };
    Startup {
        settings,
        services,
        warning,
        orphans,
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_alias_cannot_bypass_external_change_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("map.thinoda");
        let nested = dir.path().join("nested");
        std::fs::create_dir(&nested).unwrap();
        let alias = nested.join("..").join("map.thinoda");
        let mut app = ThinodaApp::headless();
        app.doc_mut().unwrap().save_to(&path, false).unwrap();
        app.doc_mut()
            .unwrap()
            .doc
            .edit_sheet("Edit", |sheet| sheet.set_title(sheet.root, "My changes"))
            .unwrap();
        let external = b"newer external content";
        std::fs::write(&path, external).unwrap();
        let key = app.doc().unwrap().key.clone();
        let workbook = app.doc().unwrap().doc.workbook().clone();
        let revision = app.doc().unwrap().doc.state_id();
        assert!(!app.save_active_to(&alias, false));
        assert_eq!(app.dialogs.conflict.as_deref(), Some(key.as_str()));
        assert_eq!(std::fs::read(&path).unwrap(), external);
        assert!(app.doc().unwrap().is_dirty());
        assert_eq!(app.doc().unwrap().path.as_deref(), Some(path.as_path()));
        assert_eq!(app.doc().unwrap().doc.workbook(), &workbook);
        assert_eq!(app.doc().unwrap().doc.state_id(), revision);
        #[cfg(unix)]
        {
            let link = dir.path().join("linked-map.thinoda");
            let folder = dir.path().join("linked-folder");
            std::os::unix::fs::symlink(&path, &link).unwrap();
            std::os::unix::fs::symlink(dir.path(), &folder).unwrap();
            for alias in [link, folder.join("map.thinoda")] {
                app.dialogs.conflict = None;
                assert!(!app.save_active_to(&alias, false));
                assert_eq!(app.dialogs.conflict.as_deref(), Some(key.as_str()));
                assert_eq!(std::fs::read(&path).unwrap(), external);
                assert_eq!(app.doc().unwrap().doc.state_id(), revision);
            }
        }
        #[cfg(windows)]
        {
            app.dialogs.conflict = None;
            assert!(!app.save_active_to(&dir.path().join("MAP.THINODA"), false));
            assert_eq!(app.dialogs.conflict.as_deref(), Some(key.as_str()));
            assert_eq!(std::fs::read(&path).unwrap(), external);
            assert_eq!(app.doc().unwrap().doc.state_id(), revision);
        }
        app.dialogs.conflict = None;
        let copy = dir.path().join("copy.thinoda");
        assert!(app.save_active_to(&copy, false));
        assert_eq!(std::fs::read(&path).unwrap(), external);
        assert_eq!(OpenDoc::open(&copy).unwrap().doc.workbook(), &workbook);
    }

    #[test]
    fn sanitize_names() {
        assert_eq!(sanitize("a/b:c"), "a_b_c");
        assert_eq!(sanitize("  "), "Untitled");
        assert_eq!(sanitize("Plan."), "Plan");
    }

    #[test]
    fn palette_has_unique_ids() {
        let cmds = ThinodaApp::palette_commands();
        let mut ids: Vec<_> = cmds.iter().map(|(_, c)| c.id.clone()).collect();
        ids.dedup();
        assert_eq!(ids.len(), cmds.len());
        assert!(cmds.iter().any(|(c, _)| *c == Cmd::Export(Format::Pdf)));
    }

    #[test]
    fn search_finds_and_reveals() {
        let mut app = ThinodaApp::headless();
        let d = app.doc_mut().unwrap();
        let root = d.doc.sheet().root;
        let first = d.doc.sheet().children(root)[0];
        let id = d
            .doc
            .edit_sheet("x", |s| s.insert_child(first, "Needle here"))
            .unwrap();
        d.doc
            .edit_sheet("c", |s| s.set_collapsed(first, true))
            .unwrap();
        app.search.open = true;
        app.search.query = "needle".into();
        app.update_search();
        assert_eq!(app.search.hits.len(), 1);
        assert_eq!(app.search.hits[0].topic, id);
        app.goto_hit(0);
        let d = app.doc().unwrap();
        assert_eq!(d.view.selection, vec![id]);
        assert!(!d.doc.sheet().topic(first).unwrap().collapsed);
    }

    #[test]
    fn closing_other_tabs_preserves_active_document_and_home_chooser() {
        let ctx = egui::Context::default();
        let mut app = ThinodaApp::headless();
        app.new_doc(templates::instantiate("meeting"));
        let expected = app.doc().unwrap().key.clone();
        app.new_doc(templates::instantiate("swot"));
        app.active = 1;
        app.close_tab_now(0);
        assert_eq!(app.doc().unwrap().key, expected);
        app.exec(&ctx, Cmd::Home);
        app.close_tab_now(1);
        assert!(app.doc().is_none());
        assert_eq!(app.active, app.docs.len());
        assert_eq!(app.home_return_to.as_deref(), Some(expected.as_str()));
    }

    #[test]
    fn close_tab_asks_when_dirty() {
        let mut app = ThinodaApp::headless();
        let d = app.doc_mut().unwrap();
        let root = d.doc.sheet().root;
        d.doc.edit_sheet("t", |s| s.set_title(root, "X")).unwrap();
        app.request_close_tab(0);
        assert_eq!(
            app.dialogs.close,
            Some(CloseIntent::Tab(app.docs[0].key.clone()))
        );
        assert_eq!(app.docs.len(), 1);
        app.close_tab_now(0);
        assert!(app.docs.is_empty());
    }
}
