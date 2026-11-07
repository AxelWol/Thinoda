//! Application state, command dispatch and lifecycle (recovery, autosave, close handling).

use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use web_time::Instant;

use egui::{Color32, Context, Key, KeyboardShortcut, Modifiers, ViewportCommand};
use thinoda_canvas::{Action, BoardAction, BoardTool, FloatingArrangement, apply_action};
use thinoda_core::{GridKind, SheetRootMode, Workbook, search::SearchOptions, templates};
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
    doc::{OpenDoc, is_native, is_save_target},
    files,
    i18n::{self, t},
    ui::CardOpenOrder,
};

/// Save dialog filters for map documents.
const MAP_FILTERS: &[files::Filter<'static>] = &[
    ("Thinoda", &["thinoda"]),
    ("XMind", &["xmind"]),
    ("MindManager", &["mmap"]),
    ("draw.io", &["drawio"]),
];

/// Editable draw.io SVG and PNG files carry their diagram inside an image file.
const DRAWIO_OPEN_FILTER: files::Filter<'static> = ("draw.io", &["drawio", "dio", "svg", "png"]);

const MAX_PENDING_RECOVERY_CLEANUPS: usize = 256;
pub(crate) const RECOVERY_CLEANUP_WARNING: &str =
    "A recovery copy could not be removed. It may reappear after restart.";

struct PendingRecoveryCleanup {
    key: String,
    failure_logged: bool,
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn set_browser_recovery_announcement(id: &str, message: Option<&str>) {
    let element = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(id));
    if let Some(element) = element {
        if let Err(error) = element.set_attribute("lang", i18n::lang().language_tag()) {
            log::warn!("browser recovery announcement language could not be set: {error:?}");
        }
        element.set_text_content(message);
    } else {
        log::warn!("browser recovery announcement element is unavailable: {id}");
    }
}

/// Every user command of the app (menus, toolbar, shortcuts and command palette).
#[derive(Debug, Clone, PartialEq)]
pub enum Cmd {
    Canvas(Action),
    BoardTool(BoardTool),
    BoardAction(BoardAction),
    BoardRoot(SheetRootMode),
    BoardGrid(GridKind),
    BoardSnap,
    BoardMinimap,
    BoardImage,
    BoardZoomSelection,
    New,
    NewFromTemplate(&'static str),
    Home,
    Open,
    OpenPath(PathBuf),
    Save,
    SaveAs,
    SaveAll,
    CloseTab(usize),
    CloseWindow,
    MoveToNewWindow,
    NextDoc,
    Import,
    ImportDrawioLink,
    MergeFiles,
    MaskedCopy,
    MapShot,
    Print,
    Labels,
    LocalImage,
    Attachment,
    SaveAttachment,
    TaskPlanning,
    EditFimNode,
    AddFimChild,
    InsertFimBefore,
    InsertFimAfter,
    CreateZone,
    AddToZone,
    RemoveFromZone,
    RemoveZone,
    EditZone,
    Zones,
    Pitch,
    PitchPresenter,
    EditPresentation,
    WorkCalendar,
    Gantt,
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
    /// Copy the selection as a draw.io (diagrams.net) clipboard model.
    CopyAsDrawio,
    SetTheme(SettingsTheme),
    /// Focus a topic property field in the Format panel (Ctrl+K / Ctrl+Shift+N).
    EditField(FocusField),
    Ai(crate::ai::AiCommand),
    AiChat,
    AiSettings,
}

/// Format panel fields that commands can focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusField {
    Hyperlink,
    Notes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    Window,
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
    pub canvas_hits: Vec<thinoda_core::search::CanvasSearchHit>,
    pub all_sheets: bool,
    pub current: usize,
    pub dim: bool,
    pub facets: thinoda_core::search::Facets,
    pub last_key: (
        String,
        usize,
        String,
        u64,
        bool,
        bool,
        bool,
        bool,
        bool,
        thinoda_core::search::Facets,
    ),
}

impl SearchState {
    pub fn result_count(&self) -> usize {
        self.hits.len() + self.canvas_hits.len()
    }

    pub fn current_sheet(&self) -> Option<usize> {
        self.hits
            .get(self.current)
            .map(|hit| hit.sheet)
            .or_else(|| {
                self.current
                    .checked_sub(self.hits.len())
                    .and_then(|index| self.canvas_hits.get(index))
                    .map(|hit| hit.sheet)
            })
    }
}

/// Optional OS integrations (absent in tests and when the profile is unavailable).
#[derive(Default)]
pub struct Services {
    pub paths: Option<Paths>,
    pub recovery: Option<Arc<RecoveryManager>>,
    #[cfg(not(target_arch = "wasm32"))]
    pub recovery_flusher: Option<std::thread::JoinHandle<()>>,
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

/// Source path, converted document, warnings and originating viewport for an import.
pub type PendingImport = (
    Option<PathBuf>,
    OpenDoc,
    Vec<thinoda_io::ImportWarning>,
    Option<egui::ViewportId>,
);

#[derive(Default)]
pub struct Dialogs {
    pub mindmanager_export: bool,
    pub powerpoint_export: bool,
    pub powerpoint_aspect_ratio: thinoda_io::PresentationAspectRatio,
    pub(crate) powerpoint_preview: crate::powerpoint_preview::PowerPointPreview,
    pub mindmanager_symbol_color_loss: bool,
    pub mindmanager_save: Option<(String, PathBuf, bool)>,
    pub(crate) mindmanager_save_deliver: bool,
    pub(crate) board_export: Option<crate::export_preflight::BoardPreflight>,
    pub(crate) drawio_link: Option<crate::drawio_link::LinkDraft>,
    pub(crate) save_close_tab: Option<String>,
    pub imports: std::collections::VecDeque<PendingImport>,
    pub symbol_export: Option<(usize, Format)>,
    pub note_export: Option<(Format, bool)>,
    pub(crate) export_owner: Option<(String, usize, u64)>,
    pub guide: crate::help::GuideState,
    pub settings: bool,
    pub settings_tab: SettingsTab,
    pub(crate) settings_gesture: Option<crate::ui::SettingsGesture>,
    pub about: bool,
    pub legal: crate::legal::LegalState,
    pub shortcuts: bool,
    pub shortcut_browser: crate::shortcut_browser::ShortcutBrowser,
    pub(crate) palette_composing: bool,
    pub masked_copy: bool,
    pub(crate) masked_copy_owner: Option<(String, usize, u64)>,
    pub close: Option<CloseIntent>,
    pub conflict: Option<String>,
    pub(crate) conflict_save_target: Option<(String, PathBuf)>,
    pub rename_sheet: Option<RenameSheetDraft>,
    pub recovery: Vec<OrphanSession>,
}

pub struct ThinodaApp {
    pub docs: Vec<OpenDoc>,
    pub active: usize,
    pub(crate) document_windows: HashMap<String, egui::ViewportId>,
    pub(crate) file_ops: crate::file_ops::FileOps,
    pub(crate) cloud_changes: crate::cloud_changes::CloudChanges,
    pub(crate) cloud_program: crate::cloud_program::CloudProgram,
    pub(crate) window_active_documents: HashMap<egui::ViewportId, String>,
    pub(crate) current_viewport: egui::ViewportId,
    pub(crate) drawing_document_window: bool,
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
    pub(crate) clipboard_bridge: crate::board_clipboard::ClipboardBridge,
    pub(crate) card_style_color: Option<Option<thinoda_core::Color>>,
    pub(crate) board_style_clipboard: Option<crate::board_ui::BoardStyleClipboard>,
    pub symbol_color: Option<thinoda_core::Color>,
    pub sheet_paste_request: Option<(Instant, String)>,
    pub discard_late_sheet_paste: bool,
    pub toasts: Toasts,
    pub(crate) recovery_discovery_incomplete: bool,
    recovery_warning: Option<u64>,
    recovery_cleanup_warning: Option<u64>,
    recovery_cleanup_pending: VecDeque<PendingRecoveryCleanup>,
    recovery_cleanup_overflow: bool,
    #[cfg(target_arch = "wasm32")]
    recovery_is_memory_only: bool,
    pub(crate) updates: crate::updates::Updates,
    pub palette: CommandPalette,
    pub search: SearchState,
    pub dialogs: Dialogs,
    pub accent: Color32,
    pub(crate) applied: Option<(SettingsTheme, Color32, u32)>,
    pub(crate) pending_close: Option<CloseIntent>,
    pub(crate) approved_close: Option<CloseIntent>,
    pub(crate) save_all: Option<crate::save_all::SaveAll>,
    last_poll: Instant,
    last_frame: Instant,
    /// Format panel field to focus on the next frame.
    pub focus_field: Option<FocusField>,
    pub ai: crate::ai::AiState,
    pub(crate) map_shot: crate::map_shot::MapShot,
    pub(crate) printing: crate::printing::PrintState,
    pub(crate) planning: crate::planning::PlanningState,
    pub(crate) gantt: crate::gantt::GanttState,
    pub(crate) tree_table: crate::tree_table::TreeTableState,
    pub(crate) zones: crate::zones::ZoneState,
    pub(crate) pitch: crate::pitch::PitchState,
    pub(crate) fim: crate::fim::FimState,
    pub(crate) label_editor: Option<crate::labels::LabelEditor>,
    pub(crate) image_request: Option<crate::images::ImageRequest>,
    pub(crate) attachment_request: Option<crate::images::ImageRequest>,
    pub(crate) fonts: thinoda_ui::fonts::SystemFonts,
    pub(crate) font_source: Option<(String, u64)>,
    pub(crate) quick_style_dialog: Option<crate::quick_styles::StyleDialog>,
    pub(crate) theme_dialog: Option<crate::theme_editor::ThemeDialog>,
    last_titles: HashMap<egui::ViewportId, String>,
    /// Startup work still pending behind the splash; `None` once ready.
    pub(crate) launch: Option<crate::startup::Launch>,
    #[cfg(target_os = "windows")]
    pub(crate) desktop_ribbon: bool,
    #[cfg(target_os = "macos")]
    pub(crate) native_menus: crate::native_menus::NativeMenus,
    #[cfg(any(target_os = "windows", target_os = "macos", test))]
    pub(crate) ribbon: crate::ribbon::RibbonState,
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
    Format::PowerPoint,
    Format::Markdown,
    Format::Word,
    Format::Excel,
    Format::Opml,
    Format::TextBundle,
    Format::DrawIo,
    Format::DrawIoSvg,
    Format::DrawIoPng,
    Format::FreeMind,
    Format::XMind,
    Format::Csv,
    Format::PlainText,
    Format::FimXdf2,
    Format::FimXdf3,
];

impl ThinodaApp {
    /// Creates the app with the given settings and services. Opens `files` (or nothing → home).
    pub fn new(settings: Settings, services: Services, files: &[PathBuf]) -> Self {
        i18n::set_lang(i18n::resolve(settings.language));
        let fim = crate::fim::FimState::new(services.paths.as_ref());
        #[cfg(target_arch = "wasm32")]
        let recovery_is_memory_only =
            services.paths.is_some() && !thinoda_platform::vfs::global_uses_local_storage();
        let mut app = Self {
            docs: Vec::new(),
            document_windows: HashMap::new(),
            file_ops: Default::default(),
            window_active_documents: HashMap::new(),
            current_viewport: egui::ViewportId::ROOT,
            drawing_document_window: false,
            #[cfg(any(target_os = "windows", target_os = "macos", test))]
            ribbon: crate::ribbon::RibbonState::default(),
            #[cfg(target_os = "windows")]
            desktop_ribbon: false,
            #[cfg(target_os = "macos")]
            native_menus: crate::native_menus::NativeMenus::default(),
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
            clipboard_bridge: Default::default(),
            card_style_color: None,
            board_style_clipboard: None,
            symbol_color: None,
            sheet_paste_request: None,
            discard_late_sheet_paste: false,
            toasts: Toasts::default(),
            recovery_discovery_incomplete: false,
            recovery_warning: None,
            recovery_cleanup_warning: None,
            recovery_cleanup_pending: VecDeque::new(),
            recovery_cleanup_overflow: false,
            #[cfg(target_arch = "wasm32")]
            recovery_is_memory_only,
            updates: Default::default(),
            cloud_changes: Default::default(),
            cloud_program: Default::default(),
            palette: CommandPalette::default(),
            search: SearchState::default(),
            dialogs: Dialogs::default(),
            accent: thinoda_ui::tokens::DEFAULT_ACCENT,
            applied: None,
            pending_close: None,
            approved_close: None,
            save_all: None,
            last_poll: Instant::now(),
            last_frame: Instant::now(),
            focus_field: None,
            ai: Default::default(),
            map_shot: Default::default(),
            printing: Default::default(),
            planning: Default::default(),
            gantt: Default::default(),
            tree_table: Default::default(),
            zones: Default::default(),
            pitch: Default::default(),
            fim,
            label_editor: None,
            image_request: None,
            attachment_request: None,
            fonts: Default::default(),
            font_source: None,
            quick_style_dialog: None,
            theme_dialog: None,
            last_titles: HashMap::new(),
            launch: None,
        };
        app.refresh_recovery_warning();
        for f in files {
            app.open_path(f);
        }
        app
    }

    fn update_recovery_warning(&mut self, error: Option<&str>) {
        match error {
            Some(error) if self.recovery_warning.is_none() => {
                log::warn!("crash recovery unavailable: {error}");
                let message = t("Crash recovery is unavailable. Save or download your work.");
                #[cfg(target_arch = "wasm32")]
                set_browser_recovery_announcement("thinoda-recovery-availability", Some(message));
                self.recovery_warning = Some(
                    self.toasts
                        .sticky(thinoda_ui::toast::ToastKind::Warning, message),
                );
            }
            None => {
                if let Some(id) = self.recovery_warning.take() {
                    self.toasts.dismiss(id);
                    #[cfg(target_arch = "wasm32")]
                    set_browser_recovery_announcement("thinoda-recovery-availability", None);
                }
            }
            Some(_) => {}
        }
    }

    fn refresh_recovery_warning(&mut self) {
        #[cfg(target_arch = "wasm32")]
        let memory_only = self.recovery_is_memory_only;
        #[cfg(not(target_arch = "wasm32"))]
        let memory_only = false;
        #[cfg(not(target_arch = "wasm32"))]
        let worker_stopped = self
            .services
            .recovery_flusher
            .as_ref()
            .is_some_and(std::thread::JoinHandle::is_finished);
        #[cfg(target_arch = "wasm32")]
        let worker_stopped = false;

        let error = if self.services.paths.is_some() && self.services.recovery.is_none() {
            Some("recovery service is unavailable")
        } else if memory_only {
            Some("browser recovery storage is memory-only")
        } else if worker_stopped {
            Some("recovery worker stopped")
        } else if self.docs.iter().any(|doc| doc.recovery_snapshot_failed) {
            Some("a document recovery snapshot could not be queued")
        } else if self
            .services
            .recovery
            .as_ref()
            .is_some_and(|recovery| recovery.last_flush_failed())
        {
            Some("recovery flush failed")
        } else if self.recovery_warning.is_some()
            && self
                .services
                .recovery
                .as_ref()
                .is_some_and(|recovery| recovery.has_unflushed_snapshots())
        {
            Some("recovery snapshots are awaiting persistence")
        } else {
            None
        };
        self.update_recovery_warning(error);
    }

    fn defer_recovery_cleanup(&mut self, key: &str, failure_logged: bool) {
        if let Some(pending) = self
            .recovery_cleanup_pending
            .iter_mut()
            .find(|pending| pending.key == key)
        {
            pending.failure_logged |= failure_logged;
        } else if self.recovery_cleanup_pending.len() < MAX_PENDING_RECOVERY_CLEANUPS {
            self.recovery_cleanup_pending
                .push_back(PendingRecoveryCleanup {
                    key: key.to_owned(),
                    failure_logged,
                });
        } else {
            if !self.recovery_cleanup_overflow {
                log::warn!("recovery cleanup retry limit reached; some copies may remain");
            }
            self.recovery_cleanup_overflow = true;
        }
    }

    fn cleanup_closed_recovery(&mut self, key: &str) {
        let Some(recovery) = self.services.recovery.clone() else {
            return;
        };
        if self.docs.iter().any(|doc| doc.key == key) {
            self.defer_recovery_cleanup(key, false);
            return;
        }
        match recovery.remove(key) {
            Ok(()) => self
                .recovery_cleanup_pending
                .retain(|pending| pending.key != key),
            Err(error) => {
                log::warn!("recovery snapshot cleanup failed: {error}");
                self.defer_recovery_cleanup(key, true);
            }
        }
    }

    fn retry_closed_recovery_cleanup(&mut self) {
        let Some(recovery) = self.services.recovery.as_ref() else {
            return;
        };
        let Some(mut pending) = self.recovery_cleanup_pending.pop_front() else {
            return;
        };
        if self.docs.iter().any(|doc| doc.key == pending.key) {
            self.recovery_cleanup_pending.push_back(pending);
        } else if let Err(error) = recovery.remove(&pending.key) {
            if !pending.failure_logged {
                log::warn!("recovery snapshot cleanup failed: {error}");
                pending.failure_logged = true;
            }
            self.recovery_cleanup_pending.push_back(pending);
        }
    }

    fn refresh_recovery_cleanup_warning(&mut self) {
        let failed = self.recovery_cleanup_overflow
            || !self.recovery_cleanup_pending.is_empty()
            || self.docs.iter().any(|doc| doc.recovery_cleanup_failed);
        if failed && self.recovery_cleanup_warning.is_none() {
            let message = t(RECOVERY_CLEANUP_WARNING);
            #[cfg(target_arch = "wasm32")]
            set_browser_recovery_announcement("thinoda-recovery-cleanup", Some(message));
            self.recovery_cleanup_warning = Some(
                self.toasts
                    .sticky(thinoda_ui::toast::ToastKind::Warning, message),
            );
        } else if !failed && let Some(id) = self.recovery_cleanup_warning.take() {
            self.toasts.dismiss(id);
            #[cfg(target_arch = "wasm32")]
            set_browser_recovery_announcement("thinoda-recovery-cleanup", None);
        }
    }

    pub(crate) fn warn_incomplete_recovery_discovery(&mut self) {
        if self.recovery_discovery_incomplete {
            return;
        }
        self.recovery_discovery_incomplete = true;
        let message =
            t("Some recovery data could not be checked. Keep existing storage and save your work.");
        #[cfg(target_arch = "wasm32")]
        set_browser_recovery_announcement("thinoda-recovery-discovery", Some(message));
        self.toasts
            .sticky(thinoda_ui::toast::ToastKind::Warning, message);
    }

    /// App for tests: no OS services, English UI, one new document.
    pub fn headless() -> Self {
        let settings = Settings {
            language: thinoda_platform::settings::Language::English,
            ..Settings::default()
        };
        let mut app = Self::new(settings, Services::default(), &[]);
        #[cfg(not(target_arch = "wasm32"))]
        {
            app.clipboard_bridge = crate::board_clipboard::ClipboardBridge::headless();
        }
        app.new_doc(templates::instantiate("mind-map"));
        app
    }

    pub fn doc(&self) -> Option<&OpenDoc> {
        self.docs.get(self.active)
    }

    pub fn doc_mut(&mut self) -> Option<&mut OpenDoc> {
        self.docs.get_mut(self.active)
    }

    pub(crate) fn visible_document_indices(&self, viewport: egui::ViewportId) -> Vec<usize> {
        self.docs
            .iter()
            .enumerate()
            .filter_map(|(index, doc)| {
                (self
                    .document_windows
                    .get(&doc.key)
                    .copied()
                    .unwrap_or(egui::ViewportId::ROOT)
                    == viewport)
                    .then_some(index)
            })
            .collect()
    }

    pub(crate) fn activate_document_window(&mut self, viewport: egui::ViewportId) {
        let visible = self.visible_document_indices(viewport);
        let preferred = self
            .window_active_documents
            .get(&viewport)
            .and_then(|key| self.docs.iter().position(|doc| &doc.key == key))
            .filter(|index| visible.contains(index));
        let current = visible.contains(&self.active).then_some(self.active);
        let home = (viewport == egui::ViewportId::ROOT && self.active == self.docs.len())
            .then_some(self.docs.len());
        self.active = preferred
            .or(current)
            .or(home)
            .or_else(|| visible.first().copied())
            .unwrap_or(self.docs.len());
    }

    fn assign_document_window(&mut self, key: &str, viewport: egui::ViewportId) {
        let previous = self.document_windows.remove(key);
        if let Some(previous) = previous.filter(|previous| *previous != viewport) {
            self.prune_empty_document_window(previous);
        }
        if viewport != egui::ViewportId::ROOT {
            self.document_windows.insert(key.to_owned(), viewport);
            self.window_active_documents
                .insert(viewport, key.to_owned());
        }
    }

    fn prune_empty_document_window(&mut self, viewport: egui::ViewportId) {
        if !self
            .document_windows
            .values()
            .any(|owner| *owner == viewport)
        {
            self.return_drawio_import_viewport(viewport);
            self.window_active_documents.remove(&viewport);
            self.last_titles.remove(&viewport);
        }
    }

    fn return_document_window_to_root(&mut self, viewport: egui::ViewportId) {
        self.return_drawio_import_viewport(viewport);
        self.document_windows.retain(|_, owner| *owner != viewport);
        self.window_active_documents.remove(&viewport);
        self.last_titles.remove(&viewport);
    }

    fn move_active_to_new_window(&mut self, ctx: &Context) {
        let Some(key) = self.doc().map(|doc| doc.key.clone()) else {
            return;
        };
        if self.document_windows.contains_key(&key) {
            return;
        }
        let viewport = egui::ViewportId::from_hash_of(("thinoda-document-window", &key));
        self.assign_document_window(&key, viewport);
        self.activate_document_window(egui::ViewportId::ROOT);
        ctx.request_repaint();
    }

    pub fn new_doc(&mut self, wb: Workbook) {
        let mut d = OpenDoc::new(wb);
        d.view.titles = new_topic_titles();
        d.view.set_initial_zoom(self.settings.initial_canvas_zoom);
        let key = d.key.clone();
        self.docs.push(d);
        self.active = self.docs.len() - 1;
        self.assign_document_window(&key, self.current_viewport);
        self.mode = CenterMode::Map;
    }

    /// Opens a path synchronously. UI event producers should dispatch `Cmd::OpenPath`
    /// through `exec` so pending card input is ordered before the open.
    pub fn open_path(&mut self, path: &Path) {
        if self.activate_open_path(path) {
            return;
        }
        if self
            .dialogs
            .imports
            .iter()
            .any(|(p, ..)| p.as_deref() == Some(path))
        {
            return;
        }
        self.add_opened(path, OpenDoc::open_with_report(path));
    }

    /// Activates the tab of `path` in the current window if it is already open.
    pub(crate) fn activate_open_path(&mut self, path: &Path) -> bool {
        let Some(i) = self
            .docs
            .iter()
            .position(|d| d.path.as_deref() == Some(path))
        else {
            return false;
        };
        self.active = i;
        let key = self.docs[i].key.clone();
        self.assign_document_window(&key, self.current_viewport);
        true
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
                if !self
                    .dialogs
                    .imports
                    .iter()
                    .any(|(p, ..)| p.as_deref() == Some(path))
                {
                    self.dialogs
                        .imports
                        .push_back((Some(path.to_path_buf()), d, warnings, None));
                }
            }
            Ok((d, _)) => self.install_opened_doc(Some(path), d),
            Err(e) => {
                self.toasts
                    .error(format!("{} {}: {e}", t("Could not open"), path.display()));
            }
        }
    }

    pub fn resolve_import(&mut self, accept: bool) {
        if let Some((path, d, _, owner)) = self.dialogs.imports.pop_front()
            && accept
        {
            let previous = self.current_viewport;
            self.current_viewport = owner.unwrap_or(previous);
            self.install_opened_doc(path.as_deref(), d);
            self.current_viewport = previous;
        }
    }

    pub(crate) fn install_opened_doc(&mut self, path: Option<&Path>, mut d: OpenDoc) {
        let native = is_native(d.format) && d.import_source.is_none();
        if d.doc.sheet().fim.is_some() {
            self.format_tab = FormatTab::Fim;
            self.sidebar_family = SidebarFamily::Format;
            self.settings.show_format_panel = true;
        }
        d.view.titles = new_topic_titles();
        d.view.set_initial_zoom(self.settings.initial_canvas_zoom);
        // Replace a pristine, untouched "Untitled" tab like most editors do.
        if self.docs.len() == 1
            && self.docs[0].path.is_none()
            && !self.docs[0].is_dirty()
            && !self.docs[0].doc.can_undo()
        {
            let key = self.docs[0].key.clone();
            if let Some(owner) = self.document_windows.remove(&key) {
                self.prune_empty_document_window(owner);
            }
            self.docs.clear();
        }
        let key = d.key.clone();
        self.docs.push(d);
        self.active = self.docs.len() - 1;
        self.assign_document_window(&key, self.current_viewport);
        if native && let Some(path) = path {
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
        if self.defer_card_text_command(ctx, &cmd) {
            return;
        }
        if let Cmd::Canvas(action) = &cmd
            && self.block_fim_canvas_during_gesture(ctx, action)
        {
            return;
        }
        if matches!(&cmd, Cmd::SaveAll)
            && let Some(index) = self.docs.iter().position(|doc| doc.has_unresolved_ink())
        {
            self.focus_pending_ink(ctx, index);
            return;
        }
        // Keep document edits and exports from invalidating an unresolved gesture.
        // Navigation and presentation commands remain available so its owner can
        // be reached, and the recovery controls are not commands.
        if !matches!(
            &cmd,
            Cmd::BoardTool(_)
                | Cmd::BoardMinimap
                | Cmd::New
                | Cmd::NewFromTemplate(_)
                | Cmd::Home
                | Cmd::Open
                | Cmd::OpenPath(_)
                | Cmd::ImportDrawioLink
                | Cmd::NextDoc
                | Cmd::CloseTab(_)
                | Cmd::CloseWindow
                | Cmd::Quit
                | Cmd::SaveAll
                | Cmd::Settings
                | Cmd::About
                | Cmd::Shortcuts
                | Cmd::UserGuide
                | Cmd::QuickStart
                | Cmd::Feedback
                | Cmd::ProjectNews
                | Cmd::ToggleOutliner
                | Cmd::ToggleZen
                | Cmd::ToggleFormatPanel
                | Cmd::ToggleSymbolsPanel
                | Cmd::ToggleStatusBar
                | Cmd::Find
                | Cmd::Palette
                | Cmd::SetTheme(_)
        ) && let Some(doc) = self.doc()
            && let Err(error) = doc.ensure_no_pending_ink()
        {
            self.toasts.warning(error);
            return;
        }
        if matches!(
            &cmd,
            Cmd::New
                | Cmd::NewFromTemplate(_)
                | Cmd::Home
                | Cmd::Open
                | Cmd::OpenPath(_)
                | Cmd::ImportDrawioLink
                | Cmd::NextDoc
        ) && let Some(doc) = self.doc_mut()
        {
            doc.suspend_ink(ctx);
        }
        if let Cmd::CloseTab(index) = &cmd
            && let Some(doc) = self.docs.get(*index)
            && self
                .document_windows
                .get(&doc.key)
                .copied()
                .unwrap_or(egui::ViewportId::ROOT)
                == ctx.viewport_id()
        {
            self.docs[*index].suspend_ink(ctx);
        }
        if self.fim_node_has_draft()
            && matches!(
                cmd,
                Cmd::Save
                    | Cmd::SaveAs
                    | Cmd::SaveAll
                    | Cmd::Export(_)
                    | Cmd::Quit
                    | Cmd::CloseWindow
                    | Cmd::CloseTab(_)
            )
        {
            self.toasts
                .warning(t("Apply or cancel the open FIM node draft first."));
            return;
        }
        for doc in &mut self.docs {
            if let Err(error) = doc.flush_inspector_edit(ctx) {
                log::warn!("inspector commit before {cmd:?}: {error}");
                self.toasts.error(error);
                return;
            }
        }
        if let Some(doc) = self.doc()
            && let Err(error) = doc.validate_pending_drafts()
        {
            let message = if matches!(&cmd, Cmd::Canvas(Action::Cut)) {
                format!("{}: {error}", t("Cut cancelled; original content was kept"))
            } else {
                error
            };
            self.toasts.error(message);
            return;
        }
        if let Some(doc) = self.doc_mut()
            && let Err(error) = doc.view.try_commit_board_label(&mut doc.doc)
        {
            self.toasts.error(error.to_string());
            return;
        }
        if matches!(
            cmd,
            Cmd::Save | Cmd::SaveAs | Cmd::SaveAll | Cmd::Export(_) | Cmd::Quit | Cmd::CloseWindow
        ) && !self.command_available(&cmd)
        {
            self.toasts.warning(t("Not possible here"));
            return;
        }
        match cmd {
            Cmd::Canvas(action) => self.canvas_action(ctx, action),
            Cmd::BoardTool(tool) => self.select_board_tool(ctx, tool),
            Cmd::BoardAction(action) => self.run_board_action(action),
            Cmd::BoardRoot(mode) => self.set_board_root(mode),
            Cmd::BoardGrid(grid) => self.set_board_grid(grid),
            Cmd::BoardSnap => self.toggle_board_snap(),
            Cmd::BoardMinimap => {
                if !self.command_available(&Cmd::BoardMinimap) {
                    self.toasts.warning(t("Not possible here"));
                } else if let Some(doc) = self.doc_mut() {
                    doc.view.set_minimap_visible(!doc.view.show_minimap);
                }
            }
            Cmd::BoardImage => {
                if self.command_available(&Cmd::BoardImage) {
                    self.begin_board_image(ctx);
                } else {
                    self.toasts.warning(t("Not possible here"));
                }
            }
            Cmd::BoardZoomSelection => self.zoom_to_board_selection(),
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
                self.new_doc(templates::instantiate_localized(id, &|s| t(s).to_owned()));
                self.settings.add_recent_template(id);
                self.save_settings();
                self.template_choice = None;
            }
            Cmd::Home => {
                if self.doc().is_some() {
                    self.home_return_to = self.doc().map(|doc| doc.key.clone());
                }
                if self.current_viewport != egui::ViewportId::ROOT {
                    let keys: Vec<_> = self
                        .document_windows
                        .iter()
                        .filter_map(|(key, owner)| {
                            (*owner == self.current_viewport).then_some(key.clone())
                        })
                        .collect();
                    for key in keys {
                        self.assign_document_window(&key, egui::ViewportId::ROOT);
                    }
                    self.window_active_documents.remove(&self.current_viewport);
                }
                self.template_choice = None;
                // The home screen is shown when no document is open; offer it as a "new" tab.
                self.active = self.docs.len();
            }
            Cmd::Open => {
                let exts: Vec<&str> = Format::IMPORTABLE
                    .iter()
                    .flat_map(|f| f.extensions().iter().copied())
                    .chain(std::iter::once("xml"))
                    .collect();
                if let Some(files) = files::pick_files(
                    files::Purpose::Open,
                    None,
                    &[(t("Mind maps"), &exts), DRAWIO_OPEN_FILTER],
                    self.settings.last_open_dir.as_deref(),
                    true,
                ) {
                    for f in files {
                        if let Some(dir) = f.parent() {
                            self.settings.last_open_dir = Some(dir.to_path_buf());
                        }
                        self.request_open_from_ui(ctx, &f, CardOpenOrder::CurrentInput);
                    }
                }
            }
            Cmd::OpenPath(p) => {
                self.request_open_from_ui(ctx, &p, CardOpenOrder::CurrentInput);
            }
            Cmd::Save => {
                self.save_card_with_pending_input(ctx, false);
            }
            Cmd::SaveAs => {
                self.save_card_with_pending_input(ctx, true);
            }
            Cmd::SaveAll => {
                self.begin_save_all(ctx, None);
            }
            Cmd::CloseTab(i) => self.request_close_tab(i),
            Cmd::CloseWindow => {
                if !files::is_web() {
                    ctx.send_viewport_cmd_to(self.current_viewport, ViewportCommand::Close);
                }
            }
            Cmd::MoveToNewWindow => self.move_active_to_new_window(ctx),
            Cmd::NextDoc => {
                let visible = self.visible_document_indices(self.current_viewport);
                if !visible.is_empty() {
                    let next = visible
                        .iter()
                        .position(|index| *index == self.active)
                        .map_or(0, |position| (position + 1) % visible.len());
                    self.active = visible[next];
                }
            }
            Cmd::Import => {
                let exts: Vec<&str> = Format::IMPORTABLE
                    .iter()
                    .flat_map(|f| f.extensions().iter().copied())
                    .chain(std::iter::once("xml"))
                    .collect();
                for f in files::pick_files(
                    files::Purpose::Import,
                    None,
                    &[(t("Import"), &exts), DRAWIO_OPEN_FILTER],
                    None,
                    false,
                )
                .unwrap_or_default()
                {
                    self.request_open_from_ui(ctx, &f, CardOpenOrder::CurrentInput);
                }
            }
            Cmd::ImportDrawioLink => {
                if self.command_available(&Cmd::ImportDrawioLink) {
                    self.begin_drawio_link(ctx);
                } else {
                    self.toasts.warning(t("Not possible here"));
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
                if self.doc().is_some_and(|doc| {
                    thinoda_core::equations::sheet_has_equations(doc.doc.sheet())
                }) {
                    self.toasts
                        .warning(t(thinoda_core::equations::EXPORT_ERROR));
                    return;
                }
                self.map_shot.requested = true;
                ctx.request_repaint();
            }
            Cmd::Labels => self.begin_labels(ctx),
            Cmd::LocalImage => self.begin_image(ctx),
            Cmd::Attachment => self.begin_attachment(ctx),
            Cmd::SaveAttachment => self.save_attachment(),
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
            Cmd::TaskPlanning => self.begin_task_planning(ctx, false),
            Cmd::EditFimNode => self.begin_fim_node(ctx, None, None),
            Cmd::AddFimChild => {
                self.begin_fim_node(ctx, None, Some(crate::fim::FimInsertion::Child))
            }
            Cmd::InsertFimBefore => {
                self.begin_fim_node(ctx, None, Some(crate::fim::FimInsertion::Before))
            }
            Cmd::InsertFimAfter => {
                self.begin_fim_node(ctx, None, Some(crate::fim::FimInsertion::After))
            }
            Cmd::WorkCalendar => self.begin_task_planning(ctx, true),
            Cmd::Gantt => self.open_gantt(),
            Cmd::CreateZone => self.begin_zone_editor(ctx, None),
            Cmd::AddToZone => self.begin_zone_membership(ctx),
            Cmd::RemoveFromZone => self.remove_from_zones(ctx),
            Cmd::RemoveZone => self.remove_selected_zone(ctx),
            Cmd::EditZone => {
                if let Some(thinoda_canvas::ObjectSelection::Zone(id)) =
                    self.doc().and_then(|doc| doc.view.selected_object)
                {
                    self.begin_zone_editor(ctx, Some(id));
                }
            }
            Cmd::Zones => self.open_zones(ctx),
            Cmd::Pitch => self.open_pitch(ctx, false),
            Cmd::PitchPresenter => self.open_pitch(ctx, true),
            Cmd::EditPresentation => self.begin_presentation_editor(ctx),
            Cmd::Settings => self.dialogs.settings = true,
            Cmd::AiSettings => {
                self.dialogs.settings_tab = SettingsTab::Ai;
                self.dialogs.settings = true;
            }
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
            Cmd::Quit => {
                self.pending_close = Some(CloseIntent::Quit);
                ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, ViewportCommand::Close);
            }
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
                    match d.doc.add_sheet(sheet) {
                        Ok(i) => {
                            d.doc.set_active(i);
                            d.view.reset();
                        }
                        Err(error) => {
                            self.toasts.error(format!("{}: {error}", t("New Sheet")));
                        }
                    }
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
                if let Err(error) = d
                    .commit_canvas_drafts()
                    .and_then(|()| d.flush_inspector_edit(ctx))
                {
                    self.toasts.error(format!(
                        "{}: {}",
                        t("Not possible here"),
                        i18n::localize_model_error(&error)
                    ));
                    return;
                }
                let topic = d.view.primary().unwrap();
                let source = d.doc.sheet();
                let owned = source.owned_subtree_ids(topic);
                let external_relationships = source.relationships.iter().any(|relationship| {
                    owned.contains(&relationship.from) != owned.contains(&relationship.to)
                });
                let external_dependencies =
                    source.scheduling.dependencies.iter().any(|dependency| {
                        owned.contains(&dependency.predecessor)
                            != owned.contains(&dependency.successor)
                    });
                let mut enclosing_annotation = source
                    .summaries
                    .iter()
                    .any(|summary| summary.topic == topic && !owned.contains(&summary.parent));
                let mut current = topic;
                while let Some(parent) = source.parent(current) {
                    if let Some(index) = source.index_in_parent(current) {
                        enclosing_annotation |= source.boundaries.iter().any(|boundary| {
                            boundary.parent == parent
                                && boundary.range.is_none_or(|range| range.contains(index))
                        }) || source.summaries.iter().any(|summary| {
                            summary.parent == parent && summary.range.contains(index)
                        });
                    }
                    current = parent;
                }
                let surrounding_context = external_dependencies
                    || enclosing_annotation
                    || !source.zones.is_empty()
                    || !source.canvas.is_empty()
                    || !source.presentation.slides.is_empty();
                match d.doc.sheet().copy_branch_as_sheet(topic) {
                    Ok(sheet) => {
                        if let Err(error) = d.doc.add_sheet(sheet) {
                            self.toasts
                                .error(format!("{}: {error}", t("Not possible here")));
                            return;
                        }
                        d.view.reset();
                        if external_relationships {
                            self.toasts.info(t(
                                "External relationships remain only in the original sheet",
                            ));
                        }
                        if surrounding_context {
                            self.toasts.info(t(
                                "Some surrounding board, annotation or task context remains only in the original sheet",
                            ));
                        }
                    }
                    Err(e) => {
                        let error = e.to_string();
                        self.toasts.error(format!(
                            "{}: {}",
                            t("Not possible here"),
                            i18n::localize_model_error(&error)
                        ));
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
                if self.mode == CenterMode::Map {
                    ctx.memory_mut(|memory| {
                        memory.request_focus(thinoda_canvas::MapCanvas::id());
                    });
                }
                self.clipboard_action(ctx, Action::Paste(None));
            }
            Cmd::CopyAsDrawio => self.copy_as_drawio(ctx),
            Cmd::Ai(a) => self.start_ai(a),
            Cmd::AiChat => self.toggle_ai_chat(),
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
                self.format_tab = FormatTab::Data;
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

    fn block_fim_canvas_during_gesture(&mut self, ctx: &Context, action: &Action) -> bool {
        let changes_hierarchy = matches!(
            action,
            Action::MoveUp
                | Action::MoveDown
                | Action::Indent
                | Action::Outdent
                | Action::SortAscending
                | Action::SortDescending
                | Action::Delete
                | Action::DeleteKeepChildren
                | Action::InsertFloating(_)
                | Action::InsertParent
                | Action::Undo
                | Action::Redo
        );
        let blocked = changes_hierarchy
            && self.doc().is_some_and(|doc| {
                doc.doc.sheet().fim.is_some()
                    && (doc.has_unresolved_ink()
                        || doc.view.board_tool == BoardTool::Connector
                        || ctx.input(|input| input.pointer.any_down() || input.any_touches())
                        || ctx.dragged_id().is_some()
                        || ctx.drag_stopped_id().is_some()
                        || ctx.interaction_snapshot(|snapshot| {
                            snapshot.clicked == Some(thinoda_canvas::MapCanvas::id())
                        }))
            });
        if blocked {
            self.toasts.warning(t("Not possible here"));
        }
        blocked
    }

    pub(crate) fn canvas_action(&mut self, ctx: &Context, action: Action) {
        // Preview actions can mutate the live view (for example by scheduling
        // a reveal), even when their sheet preview is later rejected.
        if self.block_fim_canvas_during_gesture(ctx, &action) {
            return;
        }
        if matches!(action, Action::Copy | Action::Cut | Action::Paste(_)) {
            self.clipboard_action(ctx, action);
            return;
        }
        if self.doc().is_some_and(|doc| doc.doc.sheet().fim.is_some()) {
            let insertion = match action {
                Action::InsertChild => Some(crate::fim::FimInsertion::Child),
                Action::InsertSibling => Some(crate::fim::FimInsertion::After),
                Action::InsertSiblingBefore => Some(crate::fim::FimInsertion::Before),
                _ => None,
            };
            if insertion.is_some() || action == Action::StartEdit {
                self.begin_fim_node(ctx, None, insertion);
                return;
            }
            if action == Action::Duplicate {
                self.begin_fim_duplicate(ctx);
                return;
            }
            if action == Action::Delete
                && self.doc().is_some_and(|doc| {
                    doc.view.selection.len() == 1 && doc.view.selected_object.is_none()
                })
            {
                self.request_fim_node_delete(ctx);
                return;
            }
        }
        let Some(d) = self.docs.get_mut(self.active) else {
            return;
        };
        if let Err(error) = d.view.try_commit_edit(&mut d.doc) {
            self.toasts.error(error.to_string());
            return;
        }
        if let Err(error) = d.view.try_commit_board_label(&mut d.doc) {
            self.toasts.error(error.to_string());
            return;
        }
        if !action.available(&d.doc, &d.view) {
            self.toasts.warning(t("Not possible here"));
            return;
        }
        let starting_relationship = action == Action::StartRelationship;
        if d.doc.sheet().fim.is_some()
            && matches!(
                action,
                Action::MoveUp
                    | Action::MoveDown
                    | Action::Indent
                    | Action::Outdent
                    | Action::SortAscending
                    | Action::SortDescending
                    | Action::Cut
                    | Action::Delete
                    | Action::Paste(_)
                    | Action::InsertFloating(_)
                    | Action::InsertParent
                    | Action::DeleteKeepChildren
            )
        {
            let mut preview = d.doc.preview();
            if let Err(error) = apply_action(&mut preview, &mut d.view, action) {
                self.toasts
                    .warning(format!("{}: {error}", t("Not possible here")));
                return;
            }
            if let Some(text) = d.view.clipboard_out.take() {
                ctx.copy_text(text);
            }
            self.accept_fim_canvas_preview(ctx, preview);
            return;
        }
        match apply_action(&mut d.doc, &mut d.view, action) {
            Ok(_) => {
                if starting_relationship && d.view.relationship_from.is_some() {
                    self.toasts.info(t("Pick a target topic, or click blank space to create a connected floating topic"));
                }
            }
            Err(e) => {
                log::debug!("action failed: {e}");
                let error = e.to_string();
                self.toasts.warning(format!(
                    "{}: {}",
                    t("Not possible here"),
                    i18n::localize_model_error(&error)
                ));
            }
        }
    }

    fn select_board_tool(&mut self, ctx: &Context, tool: BoardTool) {
        if !self.command_available(&Cmd::BoardTool(tool)) {
            self.toasts.warning(t("Not possible here"));
            return;
        }
        if self.finish_canvas_text_edit()
            && let Some(doc) = self.doc_mut()
        {
            doc.view.set_board_tool(tool);
            ctx.memory_mut(|memory| memory.request_focus(thinoda_canvas::MapCanvas::id()));
        }
    }

    fn run_board_action(&mut self, action: BoardAction) {
        if !self.command_available(&Cmd::BoardAction(action)) {
            self.toasts.warning(t("Not possible here"));
            return;
        }
        if !self.finish_canvas_text_edit() {
            return;
        }
        let Some(doc) = self.doc_mut() else { return };
        if let Err(error) = thinoda_canvas::apply_board_action(&mut doc.doc, &mut doc.view, action)
        {
            self.toasts
                .error(format!("{}: {error}", t("Not possible here")));
        }
    }

    fn zoom_to_board_selection(&mut self) {
        if !self.command_available(&Cmd::BoardZoomSelection) {
            self.toasts.warning(t("Not possible here"));
            return;
        }
        if !self.finish_canvas_text_edit() {
            return;
        }
        let Some(doc) = self.doc_mut() else { return };
        doc.view.prune_selection(doc.doc.sheet());
        let layout = doc.view.layout_for(&doc.doc);
        let sheet = doc.doc.sheet();
        let scene = thinoda_core::scene::build_scene(sheet, &layout);
        let mut selected: HashSet<_> = doc
            .view
            .selection
            .iter()
            .copied()
            .map(thinoda_core::SceneId::Topic)
            .chain(
                doc.view
                    .board_selection
                    .iter()
                    .copied()
                    .map(thinoda_core::SceneId::Object),
            )
            .collect();
        let members: Vec<_> = doc
            .view
            .selection
            .iter()
            .filter_map(|id| sheet.board_member_for_topic(*id))
            .chain(
                doc.view
                    .board_selection
                    .iter()
                    .copied()
                    .map(thinoda_core::board::BoardMember::Object),
            )
            .collect();
        if !members.is_empty() {
            let expanded = match sheet.expand_board_selection(&members) {
                Ok(expanded) => expanded,
                Err(error) => {
                    self.toasts.error(error);
                    return;
                }
            };
            selected.extend(expanded.into_iter().map(|member| match member {
                thinoda_core::board::BoardMember::Card(id) => thinoda_core::SceneId::Topic(id),
                thinoda_core::board::BoardMember::Object(id) => thinoda_core::SceneId::Object(id),
            }));
        }
        if let Some(thinoda_canvas::ObjectSelection::Zone(id)) = doc.view.selected_object {
            selected.insert(thinoda_core::SceneId::Zone(id));
        }
        let bounds = scene
            .items
            .iter()
            .filter(|item| selected.contains(&item.id))
            .filter_map(|item| item.visible_bounds())
            .reduce(|a, b| a.union(&b));
        let area = doc.view.canvas_rect().shrink(40.0);
        let Some(bounds) = bounds.filter(|_| area.is_positive()) else {
            self.toasts.warning(t("Not possible here"));
            return;
        };
        let zoom =
            (area.width() / bounds.size.x.max(1.0)).min(area.height() / bounds.size.y.max(1.0));
        doc.view.set_zoom(zoom);
        let zoom = doc.view.zoom;
        let center = bounds.center();
        doc.view.offset = egui::vec2(-center.x * zoom, -center.y * zoom);
    }

    fn set_board_root(&mut self, mode: SheetRootMode) {
        if !self.command_available(&Cmd::BoardRoot(mode)) {
            self.toasts.warning(t("Not possible here"));
            return;
        }
        if !self.finish_canvas_text_edit() {
            return;
        }
        let Some(doc) = self.doc_mut() else { return };
        let result = match mode {
            SheetRootMode::Central => doc
                .doc
                .show_central_topic()
                .map(|()| Some(doc.doc.sheet().root)),
            SheetRootMode::Hidden => doc.doc.hide_central_topic(),
        };
        match result {
            Ok(Some(topic)) => doc.view.select_only(topic),
            Ok(None) => doc.view.prune_selection(doc.doc.sheet()),
            Err(error) => {
                self.toasts
                    .error(format!("{}: {error}", t("Not possible here")));
            }
        }
    }

    fn set_board_grid(&mut self, grid: GridKind) {
        if !self.command_available(&Cmd::BoardGrid(grid)) {
            self.toasts.warning(t("Not possible here"));
            return;
        }
        if !self.finish_canvas_text_edit() {
            return;
        }
        let Some(doc) = self.doc_mut() else { return };
        let mut settings = doc.doc.sheet().board;
        settings.grid = grid;
        if let Err(error) = doc.doc.set_board_settings(settings) {
            self.toasts
                .error(format!("{}: {error}", t("Not possible here")));
        }
    }

    fn toggle_board_snap(&mut self) {
        if !self.command_available(&Cmd::BoardSnap) {
            self.toasts.warning(t("Not possible here"));
            return;
        }
        if !self.finish_canvas_text_edit() {
            return;
        }
        let Some(doc) = self.doc_mut() else { return };
        let mut settings = doc.doc.sheet().board;
        settings.snap = !settings.snap;
        if let Err(error) = doc.doc.set_board_settings(settings) {
            self.toasts
                .error(format!("{}: {error}", t("Not possible here")));
        }
    }

    pub fn switch_sheet(&mut self, index: usize) {
        let Some(d) = self.doc_mut() else { return };
        if let Err(error) = d.switch_sheet(index) {
            self.toasts.error(error.to_string());
        }
    }

    pub fn command_available(&self, cmd: &Cmd) -> bool {
        if self.is_pitch_viewport()
            && !matches!(cmd, Cmd::Save | Cmd::SaveAll | Cmd::CloseWindow | Cmd::Quit)
        {
            return false;
        }
        if self.is_gantt_viewport()
            && !matches!(
                cmd,
                Cmd::TaskPlanning
                    | Cmd::WorkCalendar
                    | Cmd::Gantt
                    | Cmd::Print
                    | Cmd::Canvas(Action::Undo | Action::Redo)
                    | Cmd::Save
                    | Cmd::SaveAll
                    | Cmd::CloseWindow
                    | Cmd::Quit
            )
        {
            return false;
        }
        match cmd {
            Cmd::Palette => {
                self.palette.open
                    || (!self.any_modal_open() && !self.dialogs.settings && !self.dialogs.about)
            }
            Cmd::ImportDrawioLink => {
                !self.any_modal_open() && !self.dialogs.settings && !self.dialogs.about
            }
            Cmd::Export(Format::FimXdf2 | Format::FimXdf3) => {
                self.save_all.is_none()
                    && !self.saving_document()
                    && self.doc().is_some_and(|doc| doc.doc.sheet().fim.is_some())
            }
            Cmd::Save | Cmd::SaveAs | Cmd::Export(_) => {
                self.save_all.is_none() && !self.saving_document() && self.doc().is_some()
            }
            Cmd::SaveAll => {
                self.save_all.is_none() && !self.saving_document() && !self.docs.is_empty()
            }
            Cmd::Quit => self.save_all.is_none() && !self.saving_document(),
            Cmd::CloseWindow => {
                !files::is_web() && self.save_all.is_none() && !self.saving_document()
            }
            Cmd::CloseTab(index) => {
                self.save_all.is_none() && !self.saving_document() && *index < self.docs.len()
            }
            Cmd::MoveToNewWindow => {
                !cfg!(target_arch = "wasm32")
                    && self.current_viewport == egui::ViewportId::ROOT
                    && self.save_all.is_none()
                    && !self.saving_document()
                    && !self.any_modal_open()
                    && self
                        .doc()
                        .is_some_and(|doc| !self.document_windows.contains_key(&doc.key))
            }
            Cmd::Print => {
                self.doc().is_some()
                    && !self.any_modal_open()
                    && !self.dialogs.settings
                    && !self.dialogs.about
            }
            Cmd::AiChat => self.ai_available() && self.doc().is_some() && !self.any_modal_open(),
            Cmd::Ai(action) => {
                self.ai_available()
                    && !self.any_modal_open()
                    && self.ai.pending.is_none()
                    && self.ai.review.is_none()
                    && !self.ai.chat.is_busy()
                    && !self.pitch.ai_busy()
                    && (!action.needs_topic()
                        || self.doc().is_some_and(|doc| {
                            doc.view
                                .primary()
                                .is_some_and(|id| doc.doc.sheet().contains(id))
                        }))
            }
            Cmd::EditFimNode | Cmd::AddFimChild | Cmd::InsertFimBefore | Cmd::InsertFimAfter => {
                !self.any_modal_open()
                    && self.doc().is_some_and(|doc| {
                        doc.view.selection.len() == 1
                            && doc.doc.sheet().fim.as_ref().is_some_and(|fim| {
                                doc.view
                                    .primary()
                                    .and_then(|id| fim.record_for_topic(id))
                                    .is_some_and(|record| match cmd {
                                        Cmd::EditFimNode => true,
                                        Cmd::AddFimChild => {
                                            fim.record_metadata(record).is_some_and(|metadata| {
                                                matches!(
                                                    metadata.kind,
                                                    thinoda_core::fim::FimKind::Schema
                                                        | thinoda_core::fim::FimKind::Group
                                                        | thinoda_core::fim::FimKind::Choice
                                                        | thinoda_core::fim::FimKind::Field
                                                )
                                            })
                                        }
                                        _ => fim.node_parent(record).is_some(),
                                    })
                            })
                    })
            }
            Cmd::WorkCalendar
            | Cmd::Gantt
            | Cmd::Zones
            | Cmd::Pitch
            | Cmd::PitchPresenter
            | Cmd::EditPresentation => self.doc().is_some() && !self.any_modal_open(),
            Cmd::CreateZone => !self.any_modal_open() && self.selected_zone_members().is_some(),
            Cmd::AddToZone => {
                !self.any_modal_open()
                    && self.selected_zone_members().is_some()
                    && self
                        .doc()
                        .is_some_and(|doc| !doc.doc.sheet().zones.is_empty())
            }
            Cmd::RemoveFromZone => !self.any_modal_open() && self.zone_selection_has_membership(),
            Cmd::EditZone | Cmd::RemoveZone => {
                !self.any_modal_open()
                    && self.doc().is_some_and(|doc| {
                        matches!(
                            doc.view.selected_object,
                            Some(thinoda_canvas::ObjectSelection::Zone(_))
                        )
                    })
            }
            Cmd::SaveAttachment => {
                self.command_available(&Cmd::Attachment)
                    && self.doc().is_some_and(|d| {
                        d.view
                            .primary()
                            .and_then(|id| d.doc.sheet().topic(id))
                            .is_some_and(|t| t.attachment.is_some())
                    })
            }
            Cmd::Attachment
            | Cmd::LocalImage
            | Cmd::Labels
            | Cmd::TaskPlanning
            | Cmd::EditField(_) => {
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
            Cmd::Canvas(Action::InsertChild)
                if self.doc().is_some_and(|doc| doc.doc.sheet().fim.is_some()) =>
            {
                self.command_available(&Cmd::AddFimChild)
            }
            Cmd::Canvas(
                Action::InsertSibling | Action::InsertSiblingBefore | Action::Duplicate,
            ) if self.doc().is_some_and(|doc| doc.doc.sheet().fim.is_some()) => {
                self.command_available(&Cmd::InsertFimAfter)
            }
            Cmd::Canvas(Action::InsertParent | Action::DeleteKeepChildren)
                if self.doc().is_some_and(|doc| doc.doc.sheet().fim.is_some()) =>
            {
                false
            }
            Cmd::Canvas(action) => self
                .doc()
                .is_some_and(|d| action.available(&d.doc, &d.view)),
            Cmd::BoardTool(_)
            | Cmd::BoardGrid(_)
            | Cmd::BoardSnap
            | Cmd::BoardMinimap
            | Cmd::BoardImage => {
                !self.any_modal_open() && self.mode == CenterMode::Map && self.doc().is_some()
            }
            Cmd::BoardZoomSelection => {
                !self.any_modal_open()
                    && self.mode == CenterMode::Map
                    && self.doc().is_some_and(|doc| {
                        !doc.view.selection.is_empty()
                            || !doc.view.board_selection.is_empty()
                            || matches!(
                                doc.view.selected_object,
                                Some(thinoda_canvas::ObjectSelection::Zone(_))
                            )
                    })
            }
            Cmd::BoardAction(action) => {
                !self.any_modal_open()
                    && self.mode == CenterMode::Map
                    && self
                        .doc()
                        .is_some_and(|doc| action.available(&doc.doc, &doc.view))
            }
            Cmd::BoardRoot(mode) => {
                !self.any_modal_open()
                    && self.doc().is_some_and(|doc| {
                        doc.doc.sheet().fim.is_none() && doc.doc.sheet().root_mode != *mode
                    })
            }
            Cmd::SaveQuickStyle => {
                !self.any_modal_open()
                    && self.doc().is_some_and(|d| {
                        d.view.selected_object.is_none()
                            && d.view.board_selection.is_empty()
                            && d.view.selection.len() == 1
                            && crate::quick_styles::StyleTargets::from_view(d.doc.sheet(), &d.view)
                                .is_some_and(|targets| {
                                    targets.topics == d.view.selection && targets.shapes.is_empty()
                                })
                    })
            }
            Cmd::ApplyQuickStyle(_) | Cmd::ResetStyle => {
                !self.any_modal_open()
                    && self.doc().is_some_and(|d| {
                        crate::quick_styles::StyleTargets::from_view(d.doc.sheet(), &d.view)
                            .is_some_and(|targets| !targets.is_empty())
                    })
            }
            Cmd::NewSheetFromTopic => self.doc().is_some_and(|d| {
                d.view.selection.len() == 1
                    && d.view.primary().is_some_and(|id| {
                        d.doc.sheet().contains(id) && !d.doc.sheet().is_anchor(id)
                    })
            }),
            Cmd::CopyStyle => self.doc().is_some_and(|d| {
                (d.view.board_selection.is_empty()
                    && d.view.selection.len() == 1
                    && d.view
                        .primary()
                        .is_some_and(|id| d.doc.sheet().contains(id)))
                    || (d.view.selection.is_empty()
                        && d.view.board_selection.len() == 1
                        && d.doc
                            .sheet()
                            .canvas
                            .object(d.view.board_selection[0])
                            .is_some())
            }),
            Cmd::PasteStyle => self.doc().is_some_and(|d| {
                let sheet = d.doc.sheet();
                let topics = d.view.board_selection.is_empty()
                    && !d.view.selection.is_empty()
                    && d.view.selection.iter().all(|id| sheet.contains(*id))
                    && self.style_clipboard.is_some()
                    && crate::quick_styles::StyleTargets::from_view(sheet, &d.view).is_some_and(
                        |targets| {
                            targets.topics.len() == d.view.selection.len()
                                && targets.shapes.is_empty()
                        },
                    );
                let objects = d.view.selection.is_empty()
                    && !d.view.board_selection.is_empty()
                    && self.board_style_clipboard.is_some_and(|style| {
                        d.view.board_selection.iter().all(|id| {
                            sheet
                                .canvas
                                .object(*id)
                                .is_some_and(|object| style.matches(object))
                        }) && sheet
                            .expand_board_selection(
                                &d.view
                                    .board_selection
                                    .iter()
                                    .copied()
                                    .map(thinoda_core::board::BoardMember::Object)
                                    .collect::<Vec<_>>(),
                            )
                            .is_ok_and(|members| {
                                members
                                    .iter()
                                    .all(|member| !sheet.is_board_member_locked(*member))
                            })
                    });
                topics || objects
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
            if let Some(id) = d.view.board_selection.first() {
                let style = d
                    .doc
                    .sheet()
                    .canvas
                    .object(*id)
                    .map(crate::board_ui::BoardStyleClipboard::from_object);
                self.board_style_clipboard = style;
                self.style_clipboard = None;
                self.card_style_color = None;
                self.toasts.info(t("Copied"));
            } else {
                let topic = d.view.primary().and_then(|id| d.doc.sheet().topic(id));
                let style = topic.map(|topic| topic.style.clone());
                let color = topic.and_then(|topic| topic.card.map(|card| card.color));
                self.style_clipboard = style;
                self.card_style_color = color;
                self.board_style_clipboard = None;
                self.toasts.info(t("Copied topic style overrides"));
            }
            return;
        }
        if cmd == Cmd::ResetStyle {
            self.reset_selection_style();
            return;
        }
        if self
            .doc()
            .is_some_and(|d| !d.view.board_selection.is_empty())
        {
            let style = self.board_style_clipboard.expect("command availability");
            let d = self.doc_mut().unwrap();
            let selection = d.view.board_selection.clone();
            if let Err(error) = d.doc.edit_sheet("Paste Style", |sheet| {
                for id in &selection {
                    sheet
                        .update_canvas_object(*id, |object| style.apply(object))
                        .map_err(thinoda_core::EditError::Invalid)?;
                }
                Ok(())
            }) {
                self.toasts
                    .error(format!("{}: {error}", t("Not possible here")));
            }
            return;
        }
        let style = self.style_clipboard.clone().unwrap();
        let card_color = self.card_style_color;
        let d = self.doc_mut().unwrap();
        d.view.commit_edit(&mut d.doc);
        let selection = d.view.selection.clone();
        if let Err(e) = d.doc.edit_sheet("Paste Style", |sheet| {
            sheet.update_style(&selection, |target| *target = style.clone())?;
            if let Some(color) = card_color {
                for id in &selection {
                    if let Some(mut card) = sheet.topic(*id).and_then(|topic| topic.card) {
                        card.color = color;
                        sheet
                            .set_topic_card(*id, Some(card))
                            .map_err(thinoda_core::EditError::Invalid)?;
                    }
                }
            }
            Ok(())
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
        if let Err(error) = d.commit_canvas_drafts() {
            self.toasts.error(error);
            return false;
        }
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
        if let Err(error) = d.commit_canvas_drafts() {
            self.toasts.error(error);
            return;
        }
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

    pub fn board_commands() -> Vec<(Cmd, &'static str)> {
        use thinoda_core::{Band, ShapeKind, ZOrder};
        vec![
            (Cmd::BoardTool(BoardTool::Select), "Select tool"),
            (Cmd::BoardTool(BoardTool::Hand), "Hand tool"),
            (Cmd::BoardTool(BoardTool::Sticky), "Sticky note"),
            (Cmd::BoardTool(BoardTool::Text), "Text card"),
            (
                Cmd::BoardTool(BoardTool::Shape(ShapeKind::Rectangle)),
                "Shape tool",
            ),
            (Cmd::BoardTool(BoardTool::Connector), "Connector tool"),
            (Cmd::BoardTool(BoardTool::Frame), "Frame tool"),
            (Cmd::BoardTool(BoardTool::Pen), "Pen"),
            (Cmd::BoardTool(BoardTool::Highlighter), "Highlighter"),
            (Cmd::BoardTool(BoardTool::ObjectEraser), "Object eraser"),
            (Cmd::BoardTool(BoardTool::PixelEraser), "Pixel eraser"),
            (Cmd::BoardTool(BoardTool::Lasso), "Lasso"),
            (Cmd::BoardImage, "Image card"),
            (Cmd::BoardZoomSelection, "Zoom to selection"),
            (Cmd::BoardRoot(SheetRootMode::Hidden), "Hide central topic"),
            (Cmd::BoardRoot(SheetRootMode::Central), "Show central topic"),
            (Cmd::BoardGrid(GridKind::None), "No grid"),
            (Cmd::BoardGrid(GridKind::Dots), "Dot grid"),
            (Cmd::BoardGrid(GridKind::Lines), "Line grid"),
            (Cmd::BoardSnap, "Snap to grid"),
            (Cmd::BoardMinimap, "Board minimap"),
            (
                Cmd::BoardAction(BoardAction::Group),
                "Group board selection",
            ),
            (
                Cmd::BoardAction(BoardAction::Ungroup),
                "Ungroup board selection",
            ),
            (
                Cmd::BoardAction(BoardAction::Lock(true)),
                "Lock board selection",
            ),
            (
                Cmd::BoardAction(BoardAction::Lock(false)),
                "Unlock board selection",
            ),
            (
                Cmd::BoardAction(BoardAction::ZOrder(ZOrder::Forward)),
                "Bring forward",
            ),
            (
                Cmd::BoardAction(BoardAction::ZOrder(ZOrder::Backward)),
                "Send backward",
            ),
            (
                Cmd::BoardAction(BoardAction::ZOrder(ZOrder::Front)),
                "Bring to front",
            ),
            (
                Cmd::BoardAction(BoardAction::ZOrder(ZOrder::Back)),
                "Send to back",
            ),
            (
                Cmd::BoardAction(BoardAction::SetBand(Band::Above)),
                "Above map",
            ),
            (
                Cmd::BoardAction(BoardAction::SetBand(Band::Below)),
                "Below map",
            ),
            (Cmd::BoardAction(BoardAction::Rotate(-15.0)), "Rotate left"),
            (Cmd::BoardAction(BoardAction::Rotate(15.0)), "Rotate right"),
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
            && self.doc().is_some_and(|d| {
                d.view.editing_topic().is_none() && d.view.editing_board_object().is_none()
            })
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
        match thinoda_core::clipboard::decode_sheet(&text) {
            Ok(sheet) => {
                let d = self.doc_mut().unwrap();
                if let Err(error) = d.commit_canvas_drafts() {
                    self.toasts.error(error);
                    return;
                }
                match d.doc.add_sheet(sheet) {
                    Ok(_) => d.view.reset(),
                    Err(error) => {
                        self.toasts
                            .error(format!("{}: {error}", t("Could not paste sheet")));
                    }
                }
            }
            Err(e) => {
                self.toasts
                    .error(format!("{}: {e}", t("Could not paste sheet")));
            }
        }
    }

    fn save_sheet_as(&mut self) {
        let Some(d) = self.doc_mut() else { return };
        if let Err(error) = d.ensure_no_pending_ink() {
            self.toasts.warning(error);
            return;
        }
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
        if self.dialogs.board_export.is_some() {
            return false;
        }
        if let Some(doc) = self.doc()
            && let Err(error) = doc.ensure_no_pending_ink()
        {
            self.toasts.warning(error);
            return false;
        }
        if Format::from_path(path) == Some(Format::MindManager) {
            if let Some(doc) = self.doc() {
                self.dialogs.mindmanager_save = Some((doc.key.clone(), path.to_path_buf(), true));
                self.dialogs.mindmanager_save_deliver = true;
            }
            return false;
        }
        self.request_save_preflight(path, self.settings.backup_on_save, true, true, false)
    }

    pub(crate) fn save_sheet_to_confirmed(
        &mut self,
        path: &Path,
        backup: bool,
        allow_board_loss: bool,
        allow_mindmanager_loss: bool,
    ) -> bool {
        let Some(d) = self.doc_mut() else {
            return false;
        };
        if let Err(error) = d.ensure_no_pending_ink() {
            self.toasts.warning(error);
            return false;
        }
        d.view.commit_edit(&mut d.doc);
        match d.save_sheet_to_with_consent(path, backup, allow_board_loss, allow_mindmanager_loss) {
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
        if self.saving_document() {
            self.toasts.warning(t("Not possible here"));
            return false;
        }
        if !save_as && let Some(saved) = self.cloud_save_active() {
            return saved;
        }
        if self.saving_document() {
            self.toasts.warning(t("Not possible here"));
            return false;
        }
        if self.dialogs.mindmanager_save.is_some() || self.dialogs.board_export.is_some() {
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
        if let Err(error) = d.ensure_no_pending_ink() {
            self.toasts.warning(error);
            return false;
        }
        d.view.commit_edit(&mut d.doc);
        let (target, chosen) = match (&d.path, save_as) {
            (Some(p), false) if is_save_target(d.format) => {
                if (d.externally_changed || (self.file_ops.inline() && d.changed_on_disk()))
                    && !d.ignore_conflict
                {
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
            self.dialogs.mindmanager_save_deliver = chosen;
            return false;
        }
        // Browser: a newly chosen file name is also downloaded; later saves stay in browser storage.
        if chosen {
            self.request_save_preflight(&target, backup, true, false, false)
        } else {
            self.save_active_to(&target, backup)
        }
    }

    /// Web: downloads a written file (desktop: no-op).
    pub(crate) fn deliver(&mut self, path: &Path) {
        if let Err(e) = files::deliver(path) {
            self.toasts.error(format!("{}: {e}", t("Download failed")));
        }
    }

    /// Saves the active document to `target`. Returns `true` once it was written; with
    /// background file work the write is only queued and `false` is returned while
    /// [`FileOps::saving_key`](crate::file_ops) reports the pending save. The result is then
    /// applied, including the Save All and close-tab continuations, when it completes.
    pub(crate) fn save_active_to(&mut self, target: &Path, backup: bool) -> bool {
        self.request_save_preflight(target, backup, false, false, false)
    }

    pub(crate) fn save_active_file(
        &mut self,
        target: &Path,
        backup: bool,
        deliver: bool,
        allow_board_loss: bool,
        allow_mindmanager_loss: bool,
    ) -> bool {
        if self.saving_document() {
            self.toasts.warning(t("Not possible here"));
            return false;
        }
        let backup_directory = match self.configured_backup_directory(backup) {
            Ok(directory) => directory,
            Err(error) => {
                self.toasts
                    .error(format!("{}: {error}", t("Could not save")));
                return false;
            }
        };
        let Some(d) = self.docs.get_mut(self.active) else {
            return false;
        };
        if let Err(error) = d.ensure_no_pending_ink() {
            self.toasts.warning(error);
            return false;
        }
        let same_file = match d.targets_current_file(target) {
            Ok(same_file) => same_file,
            Err(error) => {
                d.ignore_conflict = false;
                self.toasts
                    .error(format!("{}: {error}", t("Could not save")));
                return false;
            }
        };
        if same_file
            && (d.externally_changed || (self.file_ops.inline() && d.changed_on_disk()))
            && !d.ignore_conflict
        {
            self.dialogs.conflict = Some(d.key.clone());
            return false;
        }
        if !self.file_ops.inline() {
            if let Err(e) = self.queue_save_with_consent(
                self.active,
                target,
                backup,
                deliver,
                false,
                crate::export_preflight::SaveConsent {
                    board: allow_board_loss,
                    mindmanager: allow_mindmanager_loss,
                },
            ) {
                self.docs[self.active].ignore_conflict = false;
                self.toasts.error(format!("{}: {e}", t("Could not save")));
            }
            return false;
        }
        let d = &mut self.docs[self.active];
        match d.save_to_with_backup_directory_and_consent(
            target,
            backup,
            backup_directory,
            crate::export_preflight::SaveConsent {
                board: allow_board_loss,
                mindmanager: allow_mindmanager_loss,
            },
        ) {
            Ok(()) => {
                self.cloud_program.forget_document(&d.key);
                if !d.is_dirty()
                    && let Some(r) = &self.services.recovery
                {
                    d.cleanup_recovery(r);
                }
                self.settings.add_recent(target);
                self.save_settings();
                self.toasts
                    .success(format!("{} {}", t("Saved to"), target.display()));
                if deliver {
                    self.deliver(target);
                }
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
        if let Some(doc) = self.doc()
            && let Err(error) = doc.ensure_no_pending_ink()
        {
            self.toasts.warning(error);
            return;
        }
        if matches!(format, Format::FimXdf2 | Format::FimXdf3) {
            self.request_export_preflight(format, allow_symbol_color_loss, false);
            return;
        }
        if self.doc().is_some_and(|doc| {
            thinoda_io::note_format_loss(doc.doc.workbook(), format, &ExportOptions::default())
        }) {
            self.dialogs.note_export = Some((format, allow_symbol_color_loss));
            self.dialogs.export_owner = self
                .doc()
                .map(|doc| (doc.key.clone(), doc.doc.active_index(), doc.doc.state_id()));
            return;
        }
        if format == Format::PowerPoint {
            self.dialogs.powerpoint_export = true;
            self.dialogs.export_owner = self
                .doc()
                .map(|doc| (doc.key.clone(), doc.doc.active_index(), doc.doc.state_id()));
            return;
        }
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
        self.export_with_note_consent(format, allow_symbol_color_loss, false);
    }

    pub(crate) fn export_with_note_consent(
        &mut self,
        format: Format,
        allow_symbol_color_loss: bool,
        allow_note_format_loss: bool,
    ) {
        self.request_export_preflight(format, allow_symbol_color_loss, allow_note_format_loss);
    }

    pub(crate) fn perform_preflight_export(
        &mut self,
        format: Format,
        allow_symbol_color_loss: bool,
        allow_note_format_loss: bool,
        allow_board_loss: bool,
    ) {
        if let Some(doc) = self.doc()
            && let Err(error) = doc.ensure_no_pending_ink()
        {
            self.toasts.warning(error);
            return;
        }
        if matches!(format, Format::FimXdf2 | Format::FimXdf3) {
            self.export_fim(format);
            return;
        }
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
        let opts = crate::visual_export::options_for_gui_export(
            format,
            d.view.dark,
            self.dialogs.powerpoint_aspect_ratio,
            allow_symbol_color_loss,
            allow_note_format_loss,
        );
        let visual = crate::visual_export::for_view(d.view.accent, d.view.dark);
        let res = thinoda_io::export_bytes_with_consent_and_visual_context(
            d.doc.workbook(),
            format,
            &opts,
            crate::export_preflight::SaveConsent {
                board: allow_board_loss,
                mindmanager: format == Format::MindManager,
            }
            .for_io(format),
            visual,
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
        if self.fim_node_has_draft() {
            self.toasts
                .warning(t("Apply or cancel the open FIM node draft first."));
            return;
        }
        if self.save_all.is_some() || self.saving_document() {
            self.toasts.warning(t("Not possible here"));
            return;
        }
        if i >= self.docs.len() {
            return;
        }
        if self.docs[i].has_unresolved_ink() {
            self.note_pending_ink(i, true);
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
        if self.docs[i].has_unresolved_ink() {
            self.note_pending_ink(i, true);
            return;
        }
        let home = self.active >= self.docs.len();
        let d = self.docs.remove(i);
        self.cloud_program.forget_document(&d.key);
        #[cfg(target_arch = "wasm32")]
        if let Some(path) = &d.path {
            files::forget_picked(path);
        }
        let owner = self.document_windows.remove(&d.key);
        if let Some(owner) = owner {
            self.prune_empty_document_window(owner);
        }
        self.cleanup_closed_recovery(&d.key);
        if home {
            self.active = self.docs.len();
        } else if i < self.active {
            self.active -= 1;
        } else if self.active >= self.docs.len() {
            self.active = self.docs.len().saturating_sub(1);
        }
    }

    pub(crate) fn resolve_save_close_tab(&mut self, key: &str, saved: bool) {
        if self.dialogs.save_close_tab.as_deref() != Some(key) || self.document_save_pending(key) {
            return;
        }
        if !saved
            && (self.cloud_program.conflict_for(key)
                || self.dialogs.conflict.as_deref() == Some(key)
                || self
                    .dialogs
                    .mindmanager_save
                    .as_ref()
                    .is_some_and(|(pending, _, sheet_only)| pending == key && !sheet_only)
                || self.board_save_pending_for(key))
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
        self.docs.iter().any(|d| d.is_dirty()) || self.fim_node_has_draft()
    }

    fn note_pending_ink(&mut self, index: usize, interrupt: bool) {
        let Some(doc) = self.docs.get_mut(index) else {
            return;
        };
        if interrupt {
            doc.interrupt_ink(thinoda_canvas::ink_controller::InkCancelReason::Disabled);
        }
        self.active = index;
        self.toasts.warning(t(
            "Resolve pending ink before continuing. Retry, copy, or discard it.",
        ));
    }

    pub(crate) fn focus_pending_ink(&mut self, ctx: &Context, index: usize) {
        // A different viewport owns its own unprocessed pointer events.
        if let Some(doc) = self.docs.get(index)
            && self
                .document_windows
                .get(&doc.key)
                .copied()
                .unwrap_or(egui::ViewportId::ROOT)
                == ctx.viewport_id()
        {
            self.docs[index].suspend_ink(ctx);
        }
        self.note_pending_ink(index, false);
        let Some(doc) = self.docs.get(index) else {
            return;
        };
        let viewport = self
            .document_windows
            .get(&doc.key)
            .copied()
            .unwrap_or(egui::ViewportId::ROOT);
        if viewport == egui::ViewportId::ROOT {
            self.window_active_documents.remove(&viewport);
        } else {
            self.window_active_documents
                .insert(viewport, doc.key.clone());
        }
        ctx.send_viewport_cmd_to(viewport, ViewportCommand::Focus);
        ctx.request_repaint_of(viewport);
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
        self.poll_fim(ctx);
        self.poll_file_ops(ctx);
        self.poll_cloud_changes(ctx);
        self.poll_cloud_program(ctx);
        #[cfg(target_arch = "wasm32")]
        {
            for error in files::take_delivery_errors() {
                self.toasts
                    .error(format!("{}: {error}", t("Download failed")));
            }
            if let Some(report) = files::take_browser_migration() {
                for conflict in report.conflicts {
                    self.toasts.warning(format!(
                        "{}: {} -> {}",
                        t("Browser storage migration conflict"),
                        conflict.original,
                        conflict.preserved
                    ));
                }
            }
        }
        for doc in &mut self.docs {
            if let Some(error) = doc.view.take_title_error() {
                log::warn!("title commit: {error}");
                self.toasts.error(i18n::localize_model_error(&error));
            }
        }
        let dt = self.last_frame.elapsed().as_secs_f32();
        self.last_frame = Instant::now();
        self.toasts.tick(dt);
        files::set_repaint_context(ctx);
        for picked in files::take_picked() {
            let failure = match picked.purpose {
                files::Purpose::Attachment(_) => t("Could not insert attachment"),
                files::Purpose::Image(_) => t("Could not insert image"),
                _ => t("Could not open"),
            };
            for error in picked.errors {
                self.toasts.error(format!("{failure}: {error}"));
            }
            match picked.purpose {
                files::Purpose::Open | files::Purpose::Import => {
                    for path in &picked.paths {
                        self.request_open_from_ui(ctx, path, CardOpenOrder::AfterQueuedInput);
                    }
                }
                files::Purpose::Dropped => {
                    for path in &picked.paths {
                        if files::is_dropped_image_path(path) {
                            self.finish_dropped_image(picked.image_owner.clone(), path);
                            #[cfg(target_arch = "wasm32")]
                            if let Err(error) = thinoda_platform::fs::remove_file(path) {
                                self.toasts
                                    .error(format!("{}: {error}", t("Could not open")));
                            }
                        } else {
                            self.request_open_from_ui(ctx, path, CardOpenOrder::AfterQueuedInput);
                        }
                    }
                    for upload in picked.dropped_uploads {
                        match upload {
                            files::DroppedUpload::Image(image) => {
                                self.finish_dropped_image_upload(picked.image_owner.clone(), image);
                            }
                            files::DroppedUpload::Document(path) => {
                                self.request_open_from_ui(
                                    ctx,
                                    &path,
                                    CardOpenOrder::AfterQueuedInput,
                                );
                            }
                        }
                    }
                }
                files::Purpose::Merge => {
                    self.merge_files(&picked.paths);
                }
                files::Purpose::FimSchemas => {
                    self.install_fim_schema_files(&picked.paths);
                }
                files::Purpose::Attachment(request) => {
                    self.complete_attachment_pick(request, &picked.paths);
                    for path in &picked.paths {
                        let _ = thinoda_platform::fs::remove_file(path);
                    }
                }
                files::Purpose::Image(request) => {
                    if let Some(upload) = picked.image_upload {
                        self.complete_image_upload(request, upload);
                    } else if let Some(path) = picked.paths.first() {
                        self.finish_image(request, path);
                    }
                    for path in &picked.paths {
                        let _ = thinoda_platform::fs::remove_file(path);
                    }
                    if self
                        .image_request
                        .as_ref()
                        .is_some_and(|r| r.request == request)
                    {
                        self.image_request = None;
                    }
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
            let cleanup_retry_due = self.last_poll.elapsed() > Duration::from_millis(500);
            for d in &mut self.docs {
                if d.recovery_due(interval)
                    && (!d.recovery_cleanup_failed || d.is_dirty() || cleanup_retry_due)
                {
                    d.snapshot(&r);
                }
            }
        }
        // Autosave of documents that already have a native file.
        if self.settings.autosave && self.save_all.is_none() && !self.saving_document() {
            let interval = self.settings.autosave_interval_secs.max(5) as f32;
            let backup = self.settings.backup_on_save;
            let background = !self.file_ops.inline();
            let mut queued = None;
            for (index, d) in self.docs.iter_mut().enumerate() {
                if d.is_dirty()
                    && d.path.is_some()
                    && is_native(d.format)
                    && d.view.editing_topic().is_none()
                    && d.view.editing_board_object().is_none()
                    && d.idle_secs() >= interval
                    && d.autosave_due()
                    && !d.externally_changed
                    && (background || !d.changed_on_disk())
                {
                    if d.format == Format::XMind {
                        let losses = match crate::export_preflight::save_losses(d, false, d.format)
                        {
                            Ok(losses) => losses,
                            Err(error) => {
                                d.autosave_failed(interval);
                                self.toasts
                                    .error(format!("{}: {error}", t("Could not save")));
                                continue;
                            }
                        };
                        if !losses.is_empty() {
                            // Autosave cannot supply consent on the user's behalf.
                            d.autosave_failed(interval.max(60.0));
                            self.toasts.warning(t("Autosave skipped because this format cannot preserve board content. Save as Thinoda to keep it."));
                            continue;
                        }
                    }
                    let p = d.path.clone().unwrap();
                    if background {
                        // One write at a time, behind the busy indicator.
                        queued = Some((index, p));
                        break;
                    }
                    if let Err(e) = d.save_to(&p, backup) {
                        d.autosave_failed(interval);
                        self.toasts.error(format!("{}: {e}", t("Could not save")));
                    } else if !d.is_dirty()
                        && let Some(r) = &self.services.recovery
                    {
                        d.cleanup_recovery(r);
                    }
                }
            }
            if let Some((index, path)) = queued
                && let Err(e) = self.queue_save(index, &path, backup, false, true)
            {
                self.docs[index].autosave_failed(interval);
                self.toasts.error(format!("{}: {e}", t("Could not save")));
            }
        }
        // Files forwarded from secondary instances (e.g. double-click in Explorer/Finder).
        if self.last_poll.elapsed() > Duration::from_millis(500) {
            self.last_poll = Instant::now();
            self.retry_closed_recovery_cleanup();
            // The browser has no background flusher thread; this also refreshes the
            // recovery lock heartbeat of this tab.
            #[cfg(target_arch = "wasm32")]
            if let Some(Err(error)) = self.services.recovery.as_ref().map(|r| r.flush()) {
                self.update_recovery_warning(Some(&error.to_string()));
            }
            self.refresh_recovery_warning();
            self.refresh_recovery_cleanup_warning();
            let paths = self
                .services
                .instance
                .as_ref()
                .and_then(|i| i.poll().ok())
                .unwrap_or_default();
            if !paths.is_empty() {
                for p in paths {
                    self.request_open_from_ui(ctx, &p, CardOpenOrder::AfterQueuedInput);
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
        let viewport = ctx.viewport_id();
        if self.last_titles.get(&viewport) != Some(&title) {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.last_titles.insert(viewport, title);
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
            let viewport = ctx.viewport_id();
            let synthetic_card_close =
                viewport == egui::ViewportId::ROOT && crate::ui::take_synthetic_card_close(ctx);
            let pending = self.docs.iter().enumerate().find_map(|(index, doc)| {
                let owner = self
                    .document_windows
                    .get(&doc.key)
                    .copied()
                    .unwrap_or(egui::ViewportId::ROOT);
                (doc.has_unresolved_ink()
                    && (viewport == egui::ViewportId::ROOT || owner == viewport))
                    .then_some(index)
            });
            if let Some(index) = pending {
                self.approved_close = None;
                self.pending_close = None;
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                self.focus_pending_ink(ctx, index);
                return;
            }
            if viewport == egui::ViewportId::ROOT && self.fim_node_has_draft() {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                self.request_fim_draft_discard();
                return;
            }
            if viewport != egui::ViewportId::ROOT {
                for doc in &mut self.docs {
                    if self.document_windows.get(&doc.key) == Some(&viewport) {
                        if let Err(error) = doc.flush_inspector_edit(ctx) {
                            log::warn!("inspector commit before document window close: {error}");
                            self.toasts.error(error);
                            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                            return;
                        }
                        doc.view.commit_edit(&mut doc.doc);
                    }
                }
                self.return_planning_viewport(viewport);
                self.return_zone_viewport(viewport);
                self.return_pitch_viewport(viewport);
                self.return_board_preflight_viewport(viewport);
                self.return_ai_viewport(viewport);
                self.return_print_viewport(viewport);
                self.return_document_window_to_root(viewport);
                ctx.request_repaint();
                return;
            }
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
            if self.saving_document() {
                // Quitting would abandon a partly applied save; the file stays intact.
                self.toasts.warning(t("Wait until the file is saved"));
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                return;
            }
            for d in &mut self.docs {
                d.view.commit_edit(&mut d.doc);
            }
            let approved = self.approved_close.take();
            let was_approved = approved.is_some();
            let pending = self.pending_close.take();
            let intent = approved.or(pending).unwrap_or(CloseIntent::Window);
            if was_approved || !self.any_dirty() {
                self.shutdown();
                if synthetic_card_close {
                    crate::ui::authorize_synthetic_card_close(ctx);
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
            } else {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                self.dialogs.close = Some(intent);
            }
        }
    }

    /// Clean shutdown: removes recovery data of saved documents.
    pub fn shutdown(&mut self) {
        self.save_settings();
        if let Some(r) = self.services.recovery.take() {
            for d in &self.docs {
                if !d.is_dirty()
                    && let Err(error) = r.remove(&d.key)
                {
                    log::warn!("recovery snapshot cleanup failed: {error}");
                }
            }
            let result = if !self.any_dirty() {
                r.finish()
            } else {
                r.flush()
            };
            if let Err(error) = result {
                log::warn!("recovery shutdown failed: {error}");
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
            let snapshots = s
                .docs
                .iter()
                .map(|doc| {
                    doc.load_snapshot().map_err(|error| {
                        format!("{} {}: {error}", t("Could not open"), doc.meta.title)
                    })
                })
                .collect::<Result<Vec<_>, _>>();
            match snapshots {
                Ok(snapshots) => {
                    let restored =
                        s.docs
                            .iter()
                            .zip(snapshots)
                            .map(|(source, snapshot)| {
                                let mut doc = OpenDoc::new(snapshot.workbook);
                                doc.view.titles = new_topic_titles();
                                doc.path = source
                                    .meta
                                    .original_path
                                    .clone()
                                    .filter(|p| files::document_exists(p));
                                doc.import_source = source.meta.import_source.clone();
                                if let Some(p) = &doc.path {
                                    doc.format = Format::from_path(p).unwrap_or(Format::Thinoda);
                                    #[cfg(not(target_arch = "wasm32"))]
                                    {
                                        doc.fingerprint = thinoda_platform::fs::fingerprint(p).ok();
                                        doc.storage_revision = doc.fingerprint.as_ref().map(
                                            thinoda_platform::storage::revision_from_fingerprint,
                                        );
                                    }
                                    #[cfg(target_arch = "wasm32")]
                                    if !files::is_durable_document(p) {
                                        doc.fingerprint = thinoda_platform::fs::fingerprint(p).ok();
                                        doc.storage_revision = doc.fingerprint.as_ref().map(
                                            thinoda_platform::storage::revision_from_fingerprint,
                                        );
                                    }
                                }
                                // Rebind a safe recovered gesture only after the new
                                // document's recovery-dirty state has been established.
                                doc.doc.mark_dirty();
                                if let Some(pending_ink) = &snapshot.pending_ink {
                                    doc.restore_pending_ink(pending_ink).map_err(|error| {
                                        format!(
                                            "{} {}: {error}",
                                            t("Could not open"),
                                            source.meta.title
                                        )
                                    })?;
                                }
                                Ok((doc, snapshot.pending_ink))
                            })
                            .collect::<Result<Vec<_>, String>>();
                    let restored = match restored {
                        Ok(restored) => restored,
                        Err(message) => {
                            log::warn!("Recovery restore failed: {message}");
                            self.warn_incomplete_recovery_discovery();
                            self.toasts.error(message);
                            keep.push(s);
                            continue;
                        }
                    };
                    if let Some(recovery) = self.services.recovery.clone() {
                        let mut attempted = Vec::new();
                        let persisted = restored.iter().zip(&s.docs).try_for_each(
                            |((doc, pending_ink), source)| {
                                let mut meta = source.meta.clone();
                                meta.original_path = doc.path.clone();
                                meta.dirty = true;
                                attempted.push(doc.key.clone());
                                if let Some(pending_ink) = pending_ink {
                                    recovery.persist_snapshot_with_pending_ink(
                                        &doc.key,
                                        meta,
                                        doc.doc.workbook(),
                                        pending_ink,
                                    )
                                } else {
                                    recovery.persist_snapshot(&doc.key, meta, doc.doc.workbook())
                                }
                            },
                        );
                        if let Err(error) = persisted {
                            log::warn!("Recovery handoff failed: {error}");
                            self.toasts
                                .error(format!("{}: {error}", t("Recover Unsaved Work")));
                            for key in attempted {
                                self.cleanup_closed_recovery(&key);
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
                    for ((doc, _), source) in restored.into_iter().zip(&s.docs) {
                        #[cfg(target_arch = "wasm32")]
                        if let Some(path) = &doc.path
                            && files::is_durable_document(path)
                        {
                            self.refresh_recovered_revision(&doc.key, path);
                        }
                        self.docs.push(doc);
                        self.active = self.docs.len() - 1;
                        self.toasts
                            .success(format!("{}: {}", t("Restored"), source.meta.title));
                    }
                }
                Err(message) => {
                    log::warn!("Recovery restore failed: {message}");
                    self.warn_incomplete_recovery_discovery();
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

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn download_raw_recovery(&mut self, doc: &thinoda_platform::recovery::OrphanDoc) {
        const ANNOUNCEMENT: &str = "thinoda-recovery-download";
        set_browser_recovery_announcement(ANNOUNCEMENT, None);
        let result = doc.raw_browser_record().map_err(|_| ()).and_then(|blob| {
            thinoda_platform::web::download_blob("thinoda-recovery-record.utf16le", &blob)
                .map_err(|_| ())
        });
        let message = if result.is_ok() {
            let message = t(
                "Download requested. Check browser downloads; the original recovery data was kept.",
            );
            self.toasts.success(message);
            message
        } else {
            log::warn!("Raw recovery download could not be started");
            let message = t(
                "Raw download failed. Keep recovery data, check browser storage and downloads, close other tabs, and try again.",
            );
            self.toasts.error(message);
            message
        };
        set_browser_recovery_announcement(ANNOUNCEMENT, Some(message));
    }

    pub fn discard_orphans(&mut self) {
        if self.recovery_discovery_incomplete {
            log::warn!("Recovery discard blocked because discovery was incomplete");
            return;
        }
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
        for (sc, c) in self.profile_bindings(&self.settings.keymaps, self.mode, ctx.os()) {
            if matches!(c, Cmd::Canvas(_)) {
                if crate::keymaps::pressed(ctx, sc) {
                    break;
                }
                continue;
            }
            if ctx.text_edit_focused()
                && ((!sc.modifiers.command && !sc.modifiers.ctrl && !sc.modifiers.alt)
                    || crate::keymaps::text_shortcut_command(sc, ctx.os()).is_some())
            {
                continue;
            }
            if matches!(c, Cmd::CopyStyle | Cmd::PasteStyle) && ctx.text_edit_focused() {
                continue;
            }
            if ctx.input_mut(|i| i.consume_shortcut(&sc)) {
                if !sc.modifiers.command && !sc.modifiers.ctrl && !sc.modifiers.alt {
                    ctx.input_mut(|input| {
                        input
                            .events
                            .retain(|event| !matches!(event, egui::Event::Text(_)))
                    });
                }
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
                doc.view.editing_topic().is_none()
                    && doc.view.editing_board_object().is_none()
                    && doc.view.relationship_from.is_none()
            })
            && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
        {
            self.zen = false;
        }
        // Drag & drop of files onto the window.
        let image_owner =
            if ctx.input(|input| !input.raw.dropped_files.is_empty()) && self.doc().is_some() {
                self.capture_drop_owner(ctx)
            } else {
                None
            };
        for (path, owner) in files::dropped_files(ctx, image_owner) {
            if files::is_dropped_image_path(&path) {
                self.finish_dropped_image(owner, &path);
            } else {
                self.request_open_from_ui(ctx, &path, CardOpenOrder::AfterQueuedInput);
            }
        }
    }

    /// Ordered dispatch bindings, also used by command-palette and shortcut hints.
    pub fn global_shortcut_bindings_for_platform(
        active: usize,
        os: egui::os::OperatingSystem,
    ) -> Vec<(KeyboardShortcut, Cmd)> {
        Self::global_shortcut_bindings(active)
            .into_iter()
            .map(|(shortcut, command)| {
                let command = if os == egui::os::OperatingSystem::Mac
                    && !files::is_web()
                    && matches!(command, Cmd::CloseTab(_))
                {
                    Cmd::CloseWindow
                } else {
                    command
                };
                (shortcut, command)
            })
            .collect()
    }

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
            self.search.canvas_hits.clear();
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
            self.search.facets.clone(),
        );
        if key == self.search.last_key {
            return;
        }
        self.search.last_key = key;
        self.search.hits = if self.search.all_sheets {
            d.doc
                .workbook()
                .sheets
                .iter()
                .enumerate()
                .flat_map(|(index, sheet)| {
                    thinoda_core::search::search_faceted(
                        sheet,
                        index,
                        &self.search.query,
                        &self.search.opts,
                        &self.search.facets,
                    )
                })
                .collect()
        } else {
            thinoda_core::search::search_faceted(
                d.doc.sheet(),
                d.doc.active_index(),
                &self.search.query,
                &self.search.opts,
                &self.search.facets,
            )
        };
        self.search.canvas_hits = if self.search.facets.active() {
            Vec::new()
        } else if self.search.all_sheets {
            d.doc
                .workbook()
                .sheets
                .iter()
                .enumerate()
                .flat_map(|(index, sheet)| {
                    thinoda_core::search::search_canvas(
                        sheet,
                        index,
                        &self.search.query,
                        &self.search.opts,
                    )
                })
                .collect()
        } else {
            thinoda_core::search::search_canvas(
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
            d.view.filter = (self.search.open
                && self.search.dim
                && (!self.search.query.is_empty() || self.search.facets.active()))
            .then(|| {
                let mut topics: Vec<_> = self
                    .search
                    .hits
                    .iter()
                    .filter(|h| h.sheet == d.doc.active_index())
                    .map(|h| h.topic)
                    .collect();
                let ancestors: Vec<_> = topics
                    .iter()
                    .flat_map(|id| d.doc.sheet().ancestors(*id))
                    .collect();
                topics.extend(ancestors);
                topics.sort_unstable();
                topics.dedup();
                topics
            });
        }
    }

    pub(crate) fn finish_canvas_text_edit(&mut self) -> bool {
        let Some(doc) = self.docs.get_mut(self.active) else {
            return false;
        };
        let result = doc.commit_canvas_drafts();
        if let Err(error) = result {
            self.toasts
                .error(format!("{}: {error}", t("Not possible here")));
            return false;
        }
        true
    }

    /// Selects the next (`+1`) or previous (`-1`) search hit, expanding collapsed ancestors.
    pub fn goto_hit(&mut self, delta: isize) {
        if !self.finish_canvas_text_edit() {
            return;
        }
        self.update_search();
        let n = self.search.result_count();
        if n == 0 {
            return;
        }
        self.search.current =
            ((self.search.current as isize + delta).rem_euclid(n as isize)) as usize;
        let topic = self.search.hits.get(self.search.current).cloned();
        let object = self
            .search
            .current
            .checked_sub(self.search.hits.len())
            .and_then(|index| self.search.canvas_hits.get(index))
            .cloned();
        let sheet = topic
            .as_ref()
            .map(|hit| hit.sheet)
            .or_else(|| object.as_ref().map(|hit| hit.sheet))
            .expect("search index is within the result count");
        let Some(d) = self.docs.get_mut(self.active) else {
            return;
        };
        if d.doc.active_index() != sheet
            && let Err(error) = d.switch_sheet(sheet)
        {
            self.toasts.warning(error);
            return;
        }
        if let Some(hit) = object {
            d.view.select_board_object(hit.object);
            self.apply_search_filter();
            return;
        }
        let id = topic
            .expect("search result is a topic or canvas object")
            .topic;
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
        if self.search.facets.notes_only {
            self.toasts.info(t("Replaces topic titles only."));
            return;
        }
        if !self.finish_canvas_text_edit() {
            return;
        }
        self.update_search();
        let Some(d) = self.docs.get_mut(self.active) else {
            return;
        };
        let hits = self.search.hits.clone();
        let canvas_hits = self.search.canvas_hits.clone();
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
                total += thinoda_core::search::replace_in_titles(sheet, &topics, &q, &r, &o)
                    .map_err(thinoda_core::EditError::Invalid)?;
                let objects: Vec<_> = canvas_hits
                    .iter()
                    .filter(|hit| hit.sheet == index)
                    .map(|hit| hit.object)
                    .collect();
                if !objects.is_empty() {
                    total += thinoda_core::search::replace_in_canvas(sheet, &objects, &q, &r, &o)
                        .map_err(thinoda_core::EditError::Invalid)?;
                }
            }
            Ok(total)
        });
        match res {
            Ok(n) => self.toasts.info(format!("{} × {n}", t("Replace"))),
            Err(e) => {
                let error = e.to_string();
                self.toasts.error(format!(
                    "{}: {}",
                    t("Not possible here"),
                    i18n::localize_model_error(&error)
                ))
            }
        };
        self.update_search();
    }

    /// The live catalog resolves active-document commands and platform modifier names.
    pub fn command_catalog(&self, ctx: &Context) -> Vec<(Cmd, thinoda_ui::Command)> {
        let bindings = self.profile_bindings(&self.settings.keymaps, self.mode, ctx.os());
        Self::palette_commands()
            .into_iter()
            .map(|(mut command, mut metadata)| {
                if matches!(command, Cmd::CloseTab(_)) {
                    command = Cmd::CloseTab(self.active);
                }
                metadata.shortcut =
                    crate::keymaps::binding_hint(ctx, &bindings, &command, self.mode);
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
        if !files::is_web() && os == egui::os::OperatingSystem::Mac {
            match command {
                Cmd::CloseWindow => {
                    return Some(KeyboardShortcut::new(Modifiers::COMMAND, Key::W));
                }
                Cmd::CloseTab(_) => return None,
                _ => {}
            }
        }
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
        if let Some(bindings) = ctx.data(|data| {
            data.get_temp::<Vec<(KeyboardShortcut, Cmd)>>(egui::Id::unique(("active_keymap", mode)))
        }) {
            return crate::keymaps::binding_hint(ctx, &bindings, command, mode);
        }
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
            (Cmd::ImportDrawioLink, "Import draw.io link…", "", "File"),
            (Cmd::Settings, "Settings…", "", "Edit"),
            (Cmd::TaskPlanning, "Task Planning", "", "Insert"),
            (Cmd::EditFimNode, "Edit FIM node", "", "Edit"),
            (Cmd::AddFimChild, "Add FIM child", "", "Insert"),
            (Cmd::InsertFimBefore, "Insert FIM node before", "", "Insert"),
            (Cmd::InsertFimAfter, "Insert FIM node after", "", "Insert"),
            (Cmd::CreateZone, "Create Zone", "", "Zone"),
            (Cmd::AddToZone, "Add to Zone…", "", "Zone"),
            (Cmd::RemoveFromZone, "Remove from Zone", "", "Zone"),
            (Cmd::RemoveZone, "Remove Zone", "", "Zone"),
            (Cmd::EditZone, "Edit Zone", "", "Zone"),
            (Cmd::Zones, "Manage Zones", "", "Zone"),
            (Cmd::Pitch, "Pitch", "", "View"),
            (Cmd::PitchPresenter, "Presenter View", "", "View"),
            (Cmd::EditPresentation, "Edit Presentation", "", "View"),
            (Cmd::WorkCalendar, "Work Calendar", "", "Tools"),
            (Cmd::Gantt, "Gantt Chart", "", "View"),
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
            #[cfg(not(target_arch = "wasm32"))]
            (Cmd::MoveToNewWindow, "Move to New Window", "", "File"),
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
            (Cmd::LocalImage, "Local Image", "", "Insert"),
            (Cmd::Attachment, "Attachment", "", "Insert"),
            (Cmd::SaveAttachment, "Save/Download attachment", "", "File"),
            (Cmd::NewSheetFromTopic, "New Sheet from Topic", "", "Edit"),
            (Cmd::AiChat, "AI Chat", "", "AI"),
            (Cmd::AiSettings, "AI settings", "", "AI"),
            (
                Cmd::Ai(crate::ai::AiCommand::Rewrite),
                "AI: Rewrite Topic…",
                "",
                "AI",
            ),
            (
                Cmd::Ai(crate::ai::AiCommand::Translate),
                "AI: Translate Branch…",
                "",
                "AI",
            ),
            (
                Cmd::Ai(crate::ai::AiCommand::Grow),
                "AI: Grow Ideas…",
                "",
                "AI",
            ),
            (
                Cmd::Ai(crate::ai::AiCommand::Reorganize),
                "AI: Reorganize Branch…",
                "",
                "AI",
            ),
            (
                Cmd::Ai(crate::ai::AiCommand::Refine),
                "AI: Refine Branch…",
                "",
                "AI",
            ),
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
        v.extend(
            Self::board_commands()
                .into_iter()
                .map(|(command, label)| (command, label, "", "Boards")),
        );
        v.extend(
            Self::arrangement_commands()
                .into_iter()
                .filter_map(|(action, label)| {
                    if let Action::AlignFloating(arrangement) = action {
                        Some((
                            Cmd::BoardAction(BoardAction::Arrange(arrangement)),
                            label,
                            "",
                            "Boards",
                        ))
                    } else {
                        None
                    }
                }),
        );
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
        #[cfg(not(target_arch = "wasm32"))]
        v.push((Cmd::CloseWindow, "Close Window", "", "Window"));
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
        if self.doc().is_none_or(|d| {
            d.view.editing_topic().is_some() || d.view.editing_board_object().is_some()
        }) || ctx.text_edit_focused()
        {
            return;
        }
        for (shortcut, action) in self.active_canvas_bindings(ctx.os()) {
            if !Self::insertion_shortcut_bindings()
                .iter()
                .any(|(_, default)| {
                    Cmd::Canvas(default.clone()).shortcut_id()
                        == Cmd::Canvas(action.clone()).shortcut_id()
                })
            {
                if crate::keymaps::pressed(ctx, shortcut) {
                    return;
                }
                continue;
            }
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
    pub recovery_discovery_incomplete: bool,
}

#[cfg(not(target_arch = "wasm32"))]
fn spawn_recovery_flusher(
    recovery: &Arc<RecoveryManager>,
) -> std::io::Result<std::thread::JoinHandle<()>> {
    let weak = Arc::downgrade(recovery);
    std::thread::Builder::new()
        .name("thinoda-recovery".into())
        .spawn(move || {
            while let Some(recovery) = weak.upgrade() {
                let already_failed = recovery.last_flush_failed();
                match recovery.flush() {
                    Ok(()) => {}
                    Err(thinoda_platform::Error::Finished) => break,
                    Err(error) if !already_failed => {
                        log::warn!("recovery flush failed: {error}");
                    }
                    Err(_) => {}
                }
                drop(recovery);
                std::thread::sleep(Duration::from_millis(750));
            }
        })
}

/// Creates the services used by the real application (settings dir, recovery, single instance).
pub fn start_services(paths: Paths, instance: Option<SingleInstance>) -> Startup {
    let _ = paths.ensure();
    let (settings, warning) = match Settings::load_with_warning(&paths) {
        Ok(l) => (l.settings, l.warning),
        Err(e) => (Settings::default(), Some(e.to_string())),
    };
    let discovery = RecoveryManager::discover_orphans(&paths);
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
    let (recovery, recovery_flusher) = match recovery {
        Some(recovery) => match spawn_recovery_flusher(&recovery) {
            Ok(worker) => (Some(recovery), Some(worker)),
            Err(error) => {
                log::error!("crash recovery worker unavailable: {error}");
                if let Err(error) = recovery.finish() {
                    log::warn!("recovery session cleanup failed: {error}");
                }
                (None, None)
            }
        },
        None => (None, None),
    };
    let services = Services {
        instance: instance.filter(SingleInstance::is_primary),
        cloud: detect_cloud_folders(),
        recovery,
        #[cfg(not(target_arch = "wasm32"))]
        recovery_flusher,
        paths: Some(paths),
    };
    Startup {
        settings,
        services,
        warning,
        orphans: discovery.sessions,
        recovery_discovery_incomplete: discovery.incomplete,
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn app_with_sheet_branch_card(formatted: bool) -> (ThinodaApp, thinoda_core::TopicId) {
        let mut sheet = thinoda_core::Sheet::new("Board", "");
        sheet.root_mode = SheetRootMode::Hidden;
        let card = sheet
            .add_card(
                "Styled card",
                thinoda_core::Vec2::new(100.0, 100.0),
                thinoda_core::Card::new(
                    thinoda_core::CardKind::Sticky,
                    thinoda_core::Vec2::new(160.0, 100.0),
                ),
            )
            .unwrap();
        if formatted {
            sheet
                .topic_mut(card)
                .unwrap()
                .inline_text
                .update_range("Styled card", 0..6, |style| style.bold = Some(true))
                .unwrap();
        }
        let mut app = ThinodaApp::headless();
        let doc = app.doc_mut().unwrap();
        doc.doc.add_sheet(sheet).unwrap();
        doc.view.select_only(card);
        (app, card)
    }

    #[test]
    fn new_sheet_from_topic_preserves_rejected_formatted_title_draft() {
        let (mut app, card) = app_with_sheet_branch_card(true);
        let doc = app.doc_mut().unwrap();
        doc.view.begin_edit(doc.doc.sheet(), card, false);
        doc.view.set_editing_text("Pending title");
        let before = doc.doc.workbook().clone();

        app.exec(&egui::Context::default(), Cmd::NewSheetFromTopic);

        let doc = app.doc().unwrap();
        assert_eq!(doc.doc.workbook(), &before);
        assert_eq!(doc.view.editing_text(), Some("Pending title"));
        assert!(app.toasts.texts().any(|text| {
            text.contains(t(
                "Formatted card titles cannot be changed until rich-text editing is supported",
            ))
        }));
    }

    #[test]
    fn new_sheet_from_topic_commits_inspector_draft_before_copying() {
        let (mut app, card) = app_with_sheet_branch_card(false);
        let doc = app.doc_mut().unwrap();
        let source_index = doc.doc.active_index();
        doc.inspector_edit = Some(crate::doc::InspectorEdit {
            buffer_id: None,
            sheet_index: source_index,
            sheet_root: doc.doc.sheet().root,
            topic: card,
            field: crate::doc::InspectorField::Notes,
            original: None,
            format_notes: false,
            text: "Pending notes".into(),
        });
        let old_count = doc.doc.workbook().sheets.len();

        app.exec(&egui::Context::default(), Cmd::NewSheetFromTopic);

        let doc = app.doc().unwrap();
        assert!(doc.inspector_edit.is_none());
        assert_eq!(doc.doc.workbook().sheets.len(), old_count + 1);
        assert_eq!(
            doc.doc.workbook().sheets[source_index]
                .topic(card)
                .unwrap()
                .notes
                .as_deref(),
            Some("Pending notes")
        );
        assert_eq!(
            doc.doc.sheet().topic(card).unwrap().notes.as_deref(),
            Some("Pending notes")
        );
        assert_eq!(doc.doc.sheet().root_mode, SheetRootMode::Hidden);
        app.exec(
            &egui::Context::default(),
            Cmd::Canvas(thinoda_canvas::Action::Undo),
        );
        let doc = app.doc().unwrap();
        assert_eq!(doc.doc.workbook().sheets.len(), old_count);
        assert_eq!(
            doc.doc.workbook().sheets[source_index]
                .topic(card)
                .unwrap()
                .notes
                .as_deref(),
            Some("Pending notes")
        );
    }

    #[test]
    fn new_sheet_from_topic_preserves_conflicting_inspector_draft() {
        let (mut app, card) = app_with_sheet_branch_card(false);
        let doc = app.doc_mut().unwrap();
        doc.inspector_edit = Some(crate::doc::InspectorEdit {
            buffer_id: None,
            sheet_index: doc.doc.active_index(),
            sheet_root: doc.doc.sheet().root,
            topic: card,
            field: crate::doc::InspectorField::Notes,
            original: None,
            format_notes: false,
            text: "Pending notes".into(),
        });
        doc.doc
            .edit_sheet("Other edit", |sheet| {
                sheet.set_notes(card, Some("Changed elsewhere".into()))
            })
            .unwrap();
        let before = doc.doc.workbook().clone();

        app.exec(&egui::Context::default(), Cmd::NewSheetFromTopic);

        let doc = app.doc().unwrap();
        assert_eq!(doc.doc.workbook(), &before);
        assert_eq!(doc.inspector_edit.as_ref().unwrap().text, "Pending notes");
        assert!(
            app.toasts
                .texts()
                .any(|text| text.contains("draft retained"))
        );
    }

    #[test]
    fn new_sheet_from_topic_reports_omitted_board_context_and_external_relationships() {
        let mut sheet = thinoda_core::Sheet::new("Board", "Central");
        let card = sheet
            .add_card(
                "Card",
                thinoda_core::Vec2::new(100.0, 100.0),
                thinoda_core::Card::new(
                    thinoda_core::CardKind::Sticky,
                    thinoda_core::Vec2::new(160.0, 100.0),
                ),
            )
            .unwrap();
        sheet.add_relationship(sheet.root, card).unwrap();
        sheet
            .create_zone(
                "Zone",
                vec![card],
                thinoda_core::Rect::from_min_size(
                    thinoda_core::Vec2::ZERO,
                    thinoda_core::Vec2::new(300.0, 300.0),
                ),
            )
            .unwrap();
        let mut app = ThinodaApp::headless();
        let doc = app.doc_mut().unwrap();
        doc.doc.add_sheet(sheet).unwrap();
        doc.view.select_only(card);

        app.exec(&egui::Context::default(), Cmd::NewSheetFromTopic);

        assert_eq!(app.doc().unwrap().doc.sheet().floating, vec![card]);
        assert!(
            app.toasts.texts().any(|text| {
                text == t("External relationships remain only in the original sheet")
            })
        );
        assert!(app.toasts.texts().any(|text| {
            text == t(
                "Some surrounding board, annotation or task context remains only in the original sheet",
            )
        }));
        let doc = app.doc_mut().unwrap();
        doc.view.select_only(doc.doc.sheet().root);
        assert!(!app.command_available(&Cmd::NewSheetFromTopic));
    }

    fn recovery_fixture() -> (tempfile::TempDir, Arc<RecoveryManager>, ThinodaApp) {
        let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let recovery = Arc::new(RecoveryManager::start(&Paths::from_root(dir.path())).unwrap());
        let mut app = ThinodaApp::headless();
        app.services.recovery = Some(Arc::clone(&recovery));
        (dir, recovery, app)
    }

    #[test]
    fn recovery_warning_is_deduplicated_and_cleared_on_success() {
        let mut app = ThinodaApp::headless();
        let warning = "Crash recovery is unavailable. Save or download your work.";
        app.update_recovery_warning(Some("lease lost"));
        let first = app.recovery_warning.unwrap();
        assert_eq!(
            app.toasts.texts().filter(|text| *text == warning).count(),
            1
        );
        app.update_recovery_warning(Some("lease still lost"));
        assert_eq!(app.recovery_warning, Some(first));
        assert_eq!(
            app.toasts.texts().filter(|text| *text == warning).count(),
            1
        );
        app.update_recovery_warning(None);
        assert_eq!(app.recovery_warning, None);
        assert!(!app.toasts.texts().any(|text| text == warning));
        app.update_recovery_warning(Some("quota"));
        assert_ne!(app.recovery_warning, Some(first));
        assert_eq!(
            app.toasts.texts().filter(|text| *text == warning).count(),
            1
        );
    }

    #[test]
    fn missing_recovery_service_warns_on_startup() {
        let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let app = ThinodaApp::new(
            Settings::default(),
            Services {
                paths: Some(Paths::from_root(dir.path())),
                ..Services::default()
            },
            &[],
        );
        assert!(app.recovery_warning.is_some());
        assert!(
            app.toasts
                .texts()
                .any(|text| text == t("Crash recovery is unavailable. Save or download your work."))
        );
    }

    fn snapshot_path(recovery: &RecoveryManager, key: &str) -> PathBuf {
        recovery
            .session_dir()
            .join("docs")
            .join(format!("{key}.json"))
    }

    fn snapshot_edit(doc: &mut OpenDoc, recovery: &RecoveryManager, title: &str) {
        doc.doc
            .edit_sheet("Edit", |sheet| sheet.set_title(sheet.root, title))
            .unwrap();
        doc.snapshot(recovery);
    }

    #[test]
    fn failed_save_cleanup_is_retried_after_the_document_closes() {
        let (dir, recovery, mut app) = recovery_fixture();
        snapshot_edit(app.doc_mut().unwrap(), &recovery, "Saved document");
        recovery.suspend().unwrap();
        let snapshot = snapshot_path(&recovery, &app.docs[0].key);
        let original = std::fs::read(&snapshot).unwrap();
        let saved = dir.path().join("saved.thinoda");
        assert!(app.save_active_file(&saved, false, false, false, false));
        assert!(!app.docs[0].is_dirty());
        assert!(app.docs[0].recovery_cleanup_failed);
        app.refresh_recovery_cleanup_warning();
        assert!(app.recovery_cleanup_warning.is_some());
        assert!(
            app.toasts
                .texts()
                .any(|text| text == t(RECOVERY_CLEANUP_WARNING))
        );

        app.close_tab_now(0);
        assert_eq!(app.recovery_cleanup_pending.len(), 1);
        app.retry_closed_recovery_cleanup();
        assert_eq!(std::fs::read(&snapshot).unwrap(), original);
        recovery.resume().unwrap();
        app.retry_closed_recovery_cleanup();
        app.refresh_recovery_cleanup_warning();
        assert!(!snapshot.exists());
        assert!(saved.exists());
        assert!(app.recovery_cleanup_warning.is_none());
        recovery.finish().unwrap();
    }

    #[test]
    fn cleanup_warning_waits_for_each_failed_open_document() {
        let (_dir, recovery, mut app) = recovery_fixture();
        app.new_doc(templates::instantiate("mind-map"));
        for doc in &mut app.docs {
            snapshot_edit(doc, &recovery, "Saved");
        }
        recovery.suspend().unwrap();
        for doc in &mut app.docs {
            doc.doc.mark_saved();
            doc.snapshot(&recovery);
            assert!(doc.recovery_cleanup_failed);
        }
        app.refresh_recovery_cleanup_warning();
        let warning = app.recovery_cleanup_warning.unwrap();
        app.refresh_recovery_cleanup_warning();
        assert_eq!(app.recovery_cleanup_warning, Some(warning));

        recovery.resume().unwrap();
        app.docs[0].snapshot(&recovery);
        app.refresh_recovery_cleanup_warning();
        assert_eq!(app.recovery_cleanup_warning, Some(warning));
        app.docs[1].snapshot(&recovery);
        app.refresh_recovery_cleanup_warning();
        assert!(app.recovery_cleanup_warning.is_none());
        recovery.finish().unwrap();
    }

    #[test]
    fn failed_cleanup_preserves_pending_bytes_until_retry_or_flush() {
        let (_dir, recovery, mut app) = recovery_fixture();
        let doc = app.doc_mut().unwrap();
        snapshot_edit(doc, &recovery, "Queued before save");
        let path = snapshot_path(&recovery, &doc.key);
        std::fs::create_dir(&path).unwrap();
        doc.doc.mark_saved();
        doc.snapshot(&recovery);
        assert!(doc.recovery_cleanup_failed);
        assert!(recovery.has_unflushed_snapshots());
        std::fs::remove_dir(&path).unwrap();
        recovery.flush().unwrap();
        let snapshot: thinoda_platform::recovery::Snapshot =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            snapshot.workbook.active().root_topic().title,
            "Queued before save"
        );
        assert!(doc.recovery_cleanup_failed);
        doc.snapshot(&recovery);
        assert!(!path.exists());
        assert!(!doc.recovery_cleanup_failed);
        recovery.finish().unwrap();
    }

    #[test]
    fn failed_cleanup_never_removes_new_edits_or_unresolved_ink() {
        use thinoda_canvas::ink_controller::InkControllerError;

        let (_dir, recovery, mut app) = recovery_fixture();
        snapshot_edit(app.doc_mut().unwrap(), &recovery, "Before");
        recovery.suspend().unwrap();
        let path = snapshot_path(&recovery, &app.docs[0].key);
        let original = std::fs::read(&path).unwrap();
        app.docs[0].doc.mark_saved();
        app.docs[0].snapshot(&recovery);
        assert!(app.docs[0].recovery_cleanup_failed);
        recovery.resume().unwrap();

        app.docs[0]
            .ink_feedback
            .record(InkControllerError::GesturePending);
        app.docs[0].snapshot(&recovery);
        app.close_tab_now(0);
        assert_eq!(app.docs.len(), 1);
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert!(!app.docs[0].recovery_cleanup_failed);
        assert!(recovery.has_unflushed_snapshots());
        recovery.flush().unwrap();
        let pending: thinoda_platform::recovery::Snapshot =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert!(pending.pending_ink.is_some());
        assert_eq!(pending.workbook.active().root_topic().title, "Before");
        app.docs[0].discard_pending_ink();
        snapshot_edit(&mut app.docs[0], &recovery, "After");
        assert!(!app.docs[0].recovery_cleanup_failed);
        recovery.flush().unwrap();
        let snapshot: thinoda_platform::recovery::Snapshot =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(snapshot.workbook.active().root_topic().title, "After");
        assert!(app.docs[0].is_dirty());
        recovery.finish().unwrap();
    }

    #[test]
    fn closed_cleanup_respects_claims_and_a_reopened_document_key() {
        let (dir, recovery, mut app) = recovery_fixture();
        snapshot_edit(app.doc_mut().unwrap(), &recovery, "Original");
        let key = app.docs[0].key.clone();
        let path = snapshot_path(&recovery, &key);
        recovery.suspend().unwrap();
        let original = std::fs::read(&path).unwrap();
        let claim = RecoveryManager::find_orphans(&Paths::from_root(dir.path()));
        assert_eq!(claim.len(), 1);
        app.close_tab_now(0);
        app.retry_closed_recovery_cleanup();
        assert!(recovery.resume().is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);

        app.new_doc(templates::instantiate("mind-map"));
        app.docs[0].key = key;
        snapshot_edit(&mut app.docs[0], &recovery, "New unsaved edit");
        app.retry_closed_recovery_cleanup();
        assert_eq!(std::fs::read(&path).unwrap(), original);
        drop(claim);
        recovery.resume().unwrap();
        recovery.flush().unwrap();
        let newer = std::fs::read(&path).unwrap();
        app.retry_closed_recovery_cleanup();
        assert_eq!(std::fs::read(&path).unwrap(), newer);
        let snapshot: thinoda_platform::recovery::Snapshot =
            serde_json::from_slice(&newer).unwrap();
        assert_eq!(
            snapshot.workbook.active().root_topic().title,
            "New unsaved edit"
        );
        app.close_tab_now(0);
        assert!(!path.exists());
        assert!(app.recovery_cleanup_pending.is_empty());
        recovery.finish().unwrap();
    }

    #[test]
    fn closed_cleanup_retries_are_round_robin_and_bounded() {
        let (_dir, recovery, mut app) = recovery_fixture();
        app.new_doc(templates::instantiate("mind-map"));
        for doc in &mut app.docs {
            snapshot_edit(doc, &recovery, "Closed");
        }
        let first = snapshot_path(&recovery, &app.docs[0].key);
        let second = snapshot_path(&recovery, &app.docs[1].key);
        recovery.suspend().unwrap();
        app.close_tab_now(0);
        app.close_tab_now(0);
        assert_eq!(app.recovery_cleanup_pending.len(), 2);
        recovery.resume().unwrap();
        let first_bytes = std::fs::read(&first).unwrap();
        std::fs::remove_file(&first).unwrap();
        std::fs::create_dir(&first).unwrap();

        app.retry_closed_recovery_cleanup();
        assert!(second.exists());
        assert_eq!(app.recovery_cleanup_pending.len(), 2);
        app.retry_closed_recovery_cleanup();
        assert!(!second.exists());
        assert_eq!(app.recovery_cleanup_pending.len(), 1);
        app.refresh_recovery_cleanup_warning();
        assert!(app.recovery_cleanup_warning.is_some());
        std::fs::remove_dir(&first).unwrap();
        std::fs::write(&first, first_bytes).unwrap();
        app.retry_closed_recovery_cleanup();
        app.refresh_recovery_cleanup_warning();
        assert!(!first.exists());
        assert!(app.recovery_cleanup_warning.is_none());
        recovery.finish().unwrap();
    }

    #[test]
    fn cleanup_queue_overflow_remains_visible() {
        let (_dir, recovery, mut app) = recovery_fixture();
        for index in 0..=MAX_PENDING_RECOVERY_CLEANUPS {
            app.defer_recovery_cleanup(&format!("closed-{index}"), true);
        }
        assert_eq!(
            app.recovery_cleanup_pending.len(),
            MAX_PENDING_RECOVERY_CLEANUPS
        );
        assert!(app.recovery_cleanup_overflow);
        app.refresh_recovery_cleanup_warning();
        app.recovery_cleanup_pending.clear();
        app.refresh_recovery_cleanup_warning();
        assert!(app.recovery_cleanup_warning.is_some());
        recovery.finish().unwrap();
    }

    #[test]
    fn inline_autosave_reports_failed_cleanup() {
        let (dir, recovery, mut app) = recovery_fixture();
        let path = dir.path().join("autosave.thinoda");
        app.docs[0].save_to(&path, false).unwrap();
        snapshot_edit(&mut app.docs[0], &recovery, "Autosaved");
        recovery.suspend().unwrap();
        app.settings.autosave = true;
        app.settings.autosave_interval_secs = 5;
        app.docs[0].tick();
        app.docs[0].backdate_change(60);
        app.last_poll = Instant::now() - Duration::from_secs(1);
        app.housekeeping(&Context::default());
        assert!(!app.docs[0].is_dirty());
        assert!(app.docs[0].recovery_cleanup_failed);
        assert!(app.recovery_cleanup_warning.is_some());
        recovery.resume().unwrap();
        app.docs[0].snapshot(&recovery);
        app.refresh_recovery_cleanup_warning();
        assert!(app.recovery_cleanup_warning.is_none());
        recovery.finish().unwrap();
    }

    #[test]
    fn failed_handoff_only_defers_cleanup_for_attempted_copies() {
        let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let paths = Paths::from_root(dir.path());
        let original = RecoveryManager::start(&paths).unwrap();
        for key in ["one", "two", "three"] {
            original
                .snapshot(key, Default::default(), &Workbook::default())
                .unwrap();
        }
        original.suspend().unwrap();
        let recovery = Arc::new(RecoveryManager::start(&paths).unwrap());
        let mut app = ThinodaApp::new(
            Settings::default(),
            Services {
                recovery: Some(Arc::clone(&recovery)),
                ..Services::default()
            },
            &[],
        );
        app.dialogs.recovery = RecoveryManager::find_orphans(&paths);
        assert_eq!(app.dialogs.recovery[0].docs.len(), 3);
        recovery.suspend().unwrap();
        app.restore_orphans(None);
        assert!(app.docs.is_empty());
        assert_eq!(app.dialogs.recovery[0].docs.len(), 3);
        assert_eq!(app.recovery_cleanup_pending.len(), 1);
        recovery.resume().unwrap();
        app.retry_closed_recovery_cleanup();
        assert!(app.recovery_cleanup_pending.is_empty());
        app.restore_orphans(None);
        assert_eq!(app.docs.len(), 3);
        assert!(app.dialogs.recovery.is_empty());
        recovery.finish().unwrap();
    }

    #[test]
    fn failed_pending_ink_handoff_keeps_the_claimed_original_until_retry() {
        use thinoda_canvas::ink_controller::InkControllerError;

        let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let paths = Paths::from_root(dir.path());
        let original = RecoveryManager::start(&paths).unwrap();
        let mut doc = OpenDoc::new(Workbook::default());
        doc.ink_feedback.record(InkControllerError::GesturePending);
        let pending_ink = doc.pending_ink_for_recovery(false).unwrap();
        original
            .persist_snapshot_with_pending_ink(
                "ink",
                Default::default(),
                doc.doc.workbook(),
                &pending_ink,
            )
            .unwrap();
        let source = original.session_dir().join("docs").join("ink.json");
        let previous = std::fs::read(&source).unwrap();
        assert!(original.try_flush_for_crash());
        original.suspend().unwrap();
        drop(original);

        let recovery = Arc::new(RecoveryManager::start(&paths).unwrap());
        let mut app = ThinodaApp::new(
            Settings::default(),
            Services {
                recovery: Some(Arc::clone(&recovery)),
                ..Services::default()
            },
            &[],
        );
        app.dialogs.recovery = RecoveryManager::find_orphans(&paths);
        assert_eq!(app.dialogs.recovery.len(), 1);
        assert!(RecoveryManager::find_orphans(&paths).is_empty());
        recovery.suspend().unwrap();
        app.restore_orphans(None);
        assert!(app.docs.is_empty());
        assert_eq!(app.dialogs.recovery.len(), 1);
        assert_eq!(std::fs::read(&source).unwrap(), previous);

        recovery.resume().unwrap();
        app.retry_closed_recovery_cleanup();
        app.restore_orphans(None);
        assert_eq!(app.docs.len(), 1);
        assert!(app.doc().unwrap().has_unresolved_ink());
        assert!(app.dialogs.recovery.is_empty());
        assert!(!source.exists());
        let destination = snapshot_path(&recovery, &app.docs[0].key);
        let snapshot: thinoda_platform::recovery::Snapshot =
            serde_json::from_slice(&std::fs::read(destination).unwrap()).unwrap();
        assert_eq!(snapshot.pending_ink.as_deref(), Some(pending_ink.as_str()));
        recovery.finish().unwrap();
    }

    #[test]
    fn unknown_pending_ink_data_cannot_be_restored_or_discarded_implicitly() {
        let dir = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let paths = Paths::from_root(dir.path());
        let original = RecoveryManager::start(&paths).unwrap();
        let future = r#"{"version":999,"gesture":null,"feedback":[],"overflow":1,"retry_tolerance_bits":0,"retry_smooth":false}"#;
        original
            .persist_snapshot_with_pending_ink(
                "future",
                Default::default(),
                &Workbook::default(),
                future,
            )
            .unwrap();
        let source = original.session_dir().join("docs").join("future.json");
        let previous = std::fs::read(&source).unwrap();
        assert!(original.try_flush_for_crash());
        original.suspend().unwrap();
        drop(original);
        let recovery = Arc::new(RecoveryManager::start(&paths).unwrap());
        let mut app = ThinodaApp::new(
            Settings::default(),
            Services {
                recovery: Some(Arc::clone(&recovery)),
                ..Services::default()
            },
            &[],
        );
        app.dialogs.recovery = RecoveryManager::find_orphans(&paths);
        app.restore_orphans(None);
        assert!(app.docs.is_empty());
        assert_eq!(app.dialogs.recovery.len(), 1);
        assert!(app.recovery_discovery_incomplete);
        app.discard_orphans();
        assert_eq!(std::fs::read(&source).unwrap(), previous);
        app.dialogs.recovery.remove(0).discard().unwrap();
        recovery.finish().unwrap();
    }

    #[test]
    fn oversized_ink_feedback_warns_without_overwriting_previous_recovery() {
        use thinoda_canvas::{ink_controller::InkControllerError, ink_input::InkSample};
        use thinoda_platform::recovery::MAX_PENDING_INK_BYTES;

        let (_dir, recovery, mut app) = recovery_fixture();
        snapshot_edit(app.doc_mut().unwrap(), &recovery, "Before");
        recovery.flush().unwrap();
        let path = snapshot_path(&recovery, &app.docs[0].key);
        let previous = std::fs::read(&path).unwrap();
        app.docs[0]
            .ink_feedback
            .record(InkControllerError::InvalidSettings {
                reason: "a".repeat(MAX_PENDING_INK_BYTES + 1),
                sample: InkSample::new(thinoda_core::Vec2::ZERO, None),
            });
        app.docs[0].snapshot(&recovery);
        assert!(app.docs[0].recovery_snapshot_failed);
        assert!(app.docs[0].has_unresolved_ink());
        assert_eq!(std::fs::read(&path).unwrap(), previous);
        app.refresh_recovery_warning();
        assert!(app.recovery_warning.is_some());
        app.docs[0].discard_pending_ink();
        app.docs[0].snapshot(&recovery);
        recovery.flush().unwrap();
        app.refresh_recovery_warning();
        assert!(!app.docs[0].recovery_snapshot_failed);
        assert!(app.recovery_warning.is_none());
        recovery.finish().unwrap();
    }

    #[test]
    fn snapshot_queue_failure_remains_visible_until_the_retry_is_persisted() {
        let (_dir, recovery, mut app) = recovery_fixture();
        let doc = app.doc_mut().unwrap();
        doc.doc
            .edit_sheet("Before", |sheet| sheet.set_title(sheet.root, "Before"))
            .unwrap();
        doc.snapshot(&recovery);
        recovery.flush().unwrap();
        let key = doc.key.clone();
        let path = recovery
            .session_dir()
            .join("docs")
            .join(format!("{key}.json"));
        let previous = std::fs::read(&path).unwrap();
        doc.key = "invalid/key".into();
        doc.doc
            .edit_sheet("After", |sheet| sheet.set_title(sheet.root, "After"))
            .unwrap();
        doc.snapshot(&recovery);
        assert!(doc.recovery_snapshot_failed);
        assert_eq!(std::fs::read(&path).unwrap(), previous);

        app.refresh_recovery_warning();
        assert!(app.recovery_warning.is_some());
        recovery.flush().unwrap();
        app.refresh_recovery_warning();
        assert!(
            app.recovery_warning.is_some(),
            "an empty successful flush cannot repair a failed snapshot"
        );

        let doc = app.doc_mut().unwrap();
        doc.key = key;
        doc.snapshot(&recovery);
        assert!(!doc.recovery_snapshot_failed);
        assert!(recovery.has_unflushed_snapshots());
        app.refresh_recovery_warning();
        assert!(
            app.recovery_warning.is_some(),
            "queueing is not the same as persisting the retry"
        );
        recovery.flush().unwrap();
        app.refresh_recovery_warning();
        assert!(app.recovery_warning.is_none());
        let snapshot: thinoda_platform::recovery::Snapshot =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(snapshot.workbook.active().root_topic().title, "After");
        recovery.finish().unwrap();
    }

    #[test]
    fn stale_recovery_draft_stays_visible_and_preserves_the_previous_snapshot() {
        use crate::doc::{InspectorEdit, InspectorField};

        let (_dir, recovery, mut app) = recovery_fixture();
        let doc = app.doc_mut().unwrap();
        doc.doc
            .edit_sheet("Before", |sheet| sheet.set_title(sheet.root, "Before"))
            .unwrap();
        doc.snapshot(&recovery);
        recovery.flush().unwrap();
        let path = recovery
            .session_dir()
            .join("docs")
            .join(format!("{}.json", doc.key));
        let previous = std::fs::read(&path).unwrap();
        let root = doc.doc.sheet().root;
        doc.inspector_edit = Some(InspectorEdit {
            buffer_id: None,
            sheet_index: 0,
            sheet_root: root,
            topic: root,
            field: InspectorField::Notes,
            original: Some("outdated".into()),
            format_notes: false,
            text: "Pending notes".into(),
        });
        doc.snapshot(&recovery);
        assert!(doc.recovery_snapshot_failed);
        assert_eq!(std::fs::read(&path).unwrap(), previous);
        assert_eq!(doc.inspector_edit.as_ref().unwrap().text, "Pending notes");
        app.refresh_recovery_warning();
        assert!(app.recovery_warning.is_some());
        recovery.flush().unwrap();
        app.refresh_recovery_warning();
        assert!(app.recovery_warning.is_some());

        let doc = app.doc_mut().unwrap();
        doc.inspector_edit.as_mut().unwrap().original = None;
        doc.snapshot(&recovery);
        assert!(!doc.recovery_snapshot_failed);
        assert_eq!(doc.inspector_edit.as_ref().unwrap().text, "Pending notes");
        app.refresh_recovery_warning();
        assert!(app.recovery_warning.is_some());
        recovery.flush().unwrap();
        app.refresh_recovery_warning();
        assert!(app.recovery_warning.is_none());
        let snapshot: thinoda_platform::recovery::Snapshot =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(
            snapshot.workbook.active().root_topic().notes.as_deref(),
            Some("Pending notes")
        );
        recovery.finish().unwrap();
    }

    #[test]
    fn abandoning_a_failed_draft_restores_the_previous_recovery_state() {
        use crate::doc::{InspectorEdit, InspectorField};

        let (_dir, recovery, mut app) = recovery_fixture();
        let doc = app.doc_mut().unwrap();
        doc.doc
            .edit_sheet("Before", |sheet| sheet.set_title(sheet.root, "Before"))
            .unwrap();
        doc.snapshot(&recovery);
        recovery.flush().unwrap();
        let path = recovery
            .session_dir()
            .join("docs")
            .join(format!("{}.json", doc.key));
        let previous = std::fs::read(&path).unwrap();
        let root = doc.doc.sheet().root;
        doc.inspector_edit = Some(InspectorEdit {
            buffer_id: None,
            sheet_index: 0,
            sheet_root: root,
            topic: root,
            field: InspectorField::Notes,
            original: Some("outdated".into()),
            format_notes: false,
            text: "Pending notes".into(),
        });
        doc.snapshot(&recovery);
        app.refresh_recovery_warning();
        assert!(app.recovery_warning.is_some());

        let doc = app.doc_mut().unwrap();
        doc.inspector_edit = None;
        doc.snapshot(&recovery);
        assert!(!doc.recovery_snapshot_failed);
        assert!(!recovery.has_unflushed_snapshots());
        app.refresh_recovery_warning();
        assert!(app.recovery_warning.is_none());
        assert_eq!(std::fs::read(path).unwrap(), previous);
        recovery.finish().unwrap();
    }

    #[test]
    fn recovery_flush_failure_warns_and_clears_after_successful_retry() {
        let (_dir, recovery, mut app) = recovery_fixture();
        let docs = recovery.session_dir().join("docs");
        std::fs::remove_dir(&docs).unwrap();
        std::fs::write(&docs, b"blocker").unwrap();
        let doc = app.doc_mut().unwrap();
        doc.doc
            .edit_sheet("Edit", |sheet| sheet.set_title(sheet.root, "Unsaved"))
            .unwrap();
        doc.snapshot(&recovery);
        assert!(recovery.flush().is_err());
        app.refresh_recovery_warning();
        assert!(app.recovery_warning.is_some());
        std::fs::remove_file(&docs).unwrap();
        app.refresh_recovery_warning();
        assert!(app.recovery_warning.is_some());
        recovery.flush().unwrap();
        app.refresh_recovery_warning();
        assert!(app.recovery_warning.is_none());
        let doc = app.doc().unwrap();
        let path = docs.join(format!("{}.json", doc.key));
        let snapshot: thinoda_platform::recovery::Snapshot =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(snapshot.workbook.active().root_topic().title, "Unsaved");
        recovery.finish().unwrap();
    }

    #[test]
    fn recovery_warning_waits_for_every_failed_document() {
        let (_dir, recovery, mut app) = recovery_fixture();
        app.new_doc(templates::instantiate("mind-map"));
        let keys: Vec<_> = app.docs.iter().map(|doc| doc.key.clone()).collect();
        for doc in &mut app.docs {
            doc.key = format!("invalid/{}", doc.key);
            doc.doc
                .edit_sheet("Edit", |sheet| sheet.set_title(sheet.root, "Unsaved"))
                .unwrap();
            doc.snapshot(&recovery);
        }
        app.refresh_recovery_warning();
        assert!(app.recovery_warning.is_some());

        app.docs[0].key.clone_from(&keys[0]);
        app.docs[0].snapshot(&recovery);
        recovery.flush().unwrap();
        app.refresh_recovery_warning();
        assert!(app.recovery_warning.is_some());

        app.docs[1].key.clone_from(&keys[1]);
        app.docs[1].snapshot(&recovery);
        app.refresh_recovery_warning();
        assert!(app.recovery_warning.is_some());
        recovery.flush().unwrap();
        app.refresh_recovery_warning();
        assert!(app.recovery_warning.is_none());
        recovery.finish().unwrap();
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn native_worker_reports_failures_to_housekeeping_and_retries() {
        let (_dir, recovery, mut app) = recovery_fixture();
        let docs = recovery.session_dir().join("docs");
        std::fs::remove_dir(&docs).unwrap();
        std::fs::write(&docs, b"blocker").unwrap();
        let doc = app.doc_mut().unwrap();
        doc.doc
            .edit_sheet("Edit", |sheet| sheet.set_title(sheet.root, "Unsaved"))
            .unwrap();
        doc.snapshot(&recovery);
        app.services.recovery_flusher = Some(spawn_recovery_flusher(&recovery).unwrap());

        let deadline = Instant::now() + Duration::from_secs(5);
        while !recovery.last_flush_failed() {
            assert!(
                Instant::now() < deadline,
                "worker did not report the failure"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        app.last_poll = Instant::now() - Duration::from_secs(1);
        app.housekeeping(&Context::default());
        assert!(app.recovery_warning.is_some());
        assert!(recovery.has_unflushed_snapshots());
        std::fs::remove_file(&docs).unwrap();

        let deadline = Instant::now() + Duration::from_secs(5);
        while recovery.last_flush_failed() || recovery.has_unflushed_snapshots() {
            assert!(
                Instant::now() < deadline,
                "worker did not persist the retry"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        app.last_poll = Instant::now() - Duration::from_secs(1);
        app.housekeeping(&Context::default());
        assert!(app.recovery_warning.is_none());
        let path = docs.join(format!("{}.json", app.doc().unwrap().key));
        let snapshot: thinoda_platform::recovery::Snapshot =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(snapshot.workbook.active().root_topic().title, "Unsaved");
        recovery.finish().unwrap();
        app.services
            .recovery_flusher
            .take()
            .unwrap()
            .join()
            .unwrap();
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn stopped_recovery_worker_stays_visible_after_an_empty_flush() {
        let (_dir, recovery, mut app) = recovery_fixture();
        let worker = std::thread::spawn(|| {});
        let deadline = Instant::now() + Duration::from_secs(5);
        while !worker.is_finished() {
            assert!(Instant::now() < deadline, "test worker did not stop");
            std::thread::sleep(Duration::from_millis(10));
        }
        app.services.recovery_flusher = Some(worker);
        recovery.flush().unwrap();
        app.refresh_recovery_warning();
        assert!(app.recovery_warning.is_some());
        recovery.flush().unwrap();
        app.refresh_recovery_warning();
        assert!(app.recovery_warning.is_some());
        recovery.finish().unwrap();
        app.services
            .recovery_flusher
            .take()
            .unwrap()
            .join()
            .unwrap();
    }

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
        assert!(
            cmds.iter()
                .any(|(command, _)| *command == Cmd::Export(Format::PowerPoint))
        );
        assert!(
            cmds.iter()
                .any(|(command, _)| *command == Cmd::MoveToNewWindow)
        );
    }

    #[test]
    fn powerpoint_export_requests_an_explicit_slide_ratio() {
        let mut app = ThinodaApp::headless();
        let document = app.doc().unwrap();
        let expected_owner = (
            document.key.clone(),
            document.doc.active_index(),
            document.doc.state_id(),
        );

        app.export(Format::PowerPoint, false);

        assert!(app.dialogs.powerpoint_export);
        assert_eq!(app.dialogs.export_owner, Some(expected_owner));
        assert_eq!(
            app.dialogs.powerpoint_aspect_ratio,
            thinoda_io::PresentationAspectRatio::Widescreen16x9
        );
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
    fn board_search_navigates_sheets_and_replaces_atomically() {
        use thinoda_core::{CanvasObject, CanvasObjectKind, Rect, ShapeKind, Sheet, Vec2};

        fn shape(text: &str, locked: bool) -> CanvasObject {
            let mut object = CanvasObject::shape(
                ShapeKind::Rectangle,
                Rect::from_min_size(Vec2::new(500.0, 300.0), Vec2::new(100.0, 60.0)),
            );
            if let CanvasObjectKind::Shape { label, .. } = &mut object.kind {
                *label = text.into();
            }
            object.alt_text = text.into();
            object.locked = locked;
            object
        }

        let mut app = ThinodaApp::headless();
        let (first, second) = app
            .doc_mut()
            .unwrap()
            .doc
            .edit_book("Board search fixture", |book| {
                let sheet = &mut book.sheets[0];
                sheet.set_title(sheet.root, "Needle topic")?;
                let first = sheet
                    .add_canvas_object(shape("Needle shape", false))
                    .map_err(thinoda_core::EditError::Invalid)?;
                let mut other = Sheet::new("Other", "Anchor");
                let second = other
                    .add_canvas_object(shape("Needle object", true))
                    .map_err(thinoda_core::EditError::Invalid)?;
                book.sheets.push(other);
                Ok((first, second))
            })
            .unwrap();
        app.search.open = true;
        app.search.query = "needle".into();
        app.search.replace = "Goal".into();
        app.search.all_sheets = true;
        app.update_search();
        assert_eq!(app.search.hits.len(), 1);
        assert_eq!(app.search.canvas_hits.len(), 2);
        assert_eq!(app.search.result_count(), 3);
        app.goto_hit(1);
        assert_eq!(app.doc().unwrap().view.board_selection, vec![first]);
        app.goto_hit(1);
        assert_eq!(app.doc().unwrap().doc.active_index(), 1);
        assert_eq!(app.doc().unwrap().view.board_selection, vec![second]);
        assert_eq!(app.search.current_sheet(), Some(1));

        let locked = app.doc().unwrap().doc.workbook().clone();
        app.replace_all();
        assert_eq!(app.doc().unwrap().doc.workbook(), &locked);
        app.doc_mut()
            .unwrap()
            .doc
            .edit_sheet("Unlock", |sheet| {
                sheet
                    .set_canvas_objects_locked(&[second], false)
                    .map_err(thinoda_core::EditError::Invalid)
            })
            .unwrap();
        let before = app.doc().unwrap().doc.workbook().clone();
        app.replace_all();
        let after = app.doc().unwrap().doc.workbook().clone();
        assert_ne!(before, after);
        assert_eq!(after.sheets[0].root_topic().title, "Goal topic");
        assert_eq!(
            after.sheets[0].canvas.object(first).unwrap().label(),
            "Goal shape"
        );
        assert_eq!(
            after.sheets[1].canvas.object(second).unwrap().alt_text,
            "Goal object"
        );
        assert!(app.doc_mut().unwrap().doc.undo());
        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        assert!(app.doc_mut().unwrap().doc.redo());
        assert_eq!(app.doc().unwrap().doc.workbook(), &after);

        app.search.facets.notes_only = true;
        app.update_search();
        assert!(app.search.canvas_hits.is_empty());
    }

    #[test]
    fn replace_all_rejects_formatted_titles_without_cross_sheet_partial_changes() {
        use thinoda_core::{Card, CardKind, Sheet, Vec2};

        let mut app = ThinodaApp::headless();
        app.doc_mut()
            .unwrap()
            .doc
            .edit_book("Formatted title search fixture", |book| {
                let first = &mut book.sheets[0];
                first.set_title(first.root, "Needle topic")?;

                let mut second = Sheet::new("Other", "Anchor");
                let id = second
                    .add_card(
                        "Needle card",
                        Vec2::new(100.0, 100.0),
                        Card::new(CardKind::Sticky, Vec2::new(140.0, 80.0)),
                    )
                    .map_err(thinoda_core::EditError::Invalid)?;
                second
                    .topic_mut(id)
                    .unwrap()
                    .inline_text
                    .update_range("Needle card", 0..6, |style| style.bold = Some(true))
                    .map_err(thinoda_core::EditError::Invalid)?;
                book.sheets.push(second);
                Ok(())
            })
            .unwrap();

        app.search.open = true;
        app.search.query = "needle".into();
        app.search.replace = "Goal".into();
        app.search.all_sheets = true;
        app.update_search();
        let before = app.doc().unwrap().doc.workbook().clone();

        app.replace_all();

        assert_eq!(app.doc().unwrap().doc.workbook(), &before);
        assert_eq!(app.search.hits.len(), 2);
        assert_eq!(app.search.canvas_hits.len(), 0);
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
    fn moving_a_document_to_a_window_preserves_its_single_live_state() {
        let ctx = egui::Context::default();
        let mut app = ThinodaApp::headless();
        let key = app.doc().unwrap().key.clone();
        let root = app.doc().unwrap().doc.sheet().root;
        app.doc_mut()
            .unwrap()
            .doc
            .edit_sheet("Edit", |sheet| sheet.set_title(root, "Unsaved window move"))
            .unwrap();
        let revision = app.doc().unwrap().doc.state_id();

        app.move_active_to_new_window(&ctx);

        let viewport = app.document_windows[&key];
        assert_ne!(viewport, egui::ViewportId::ROOT);
        assert_eq!(app.docs.len(), 1);
        assert!(
            app.visible_document_indices(egui::ViewportId::ROOT)
                .is_empty()
        );
        assert_eq!(app.visible_document_indices(viewport), vec![0]);
        assert_eq!(app.docs[0].doc.state_id(), revision);
        assert!(app.docs[0].is_dirty());

        app.return_document_window_to_root(viewport);
        app.activate_document_window(egui::ViewportId::ROOT);
        assert_eq!(
            app.visible_document_indices(egui::ViewportId::ROOT),
            vec![0]
        );
        assert!(app.docs[0].is_dirty());
        assert_eq!(app.docs[0].doc.state_id(), revision);
    }

    #[test]
    fn document_window_fallback_renders_without_cloning_or_recursing() {
        let ctx = egui::Context::default();
        let mut app = ThinodaApp::headless();
        let key = app.doc().unwrap().key.clone();
        app.move_active_to_new_window(&ctx);
        let viewport = app.document_windows[&key];

        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.draw(ui));

        assert!(!output.shapes.is_empty());
        assert_eq!(app.docs.len(), 1);
        assert_eq!(app.document_windows.get(&key), Some(&viewport));
        assert_eq!(app.active, app.docs.len());
        assert!(!app.drawing_document_window);
        output.textures_delta.set.clear();
        output.textures_delta.free.clear();
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
