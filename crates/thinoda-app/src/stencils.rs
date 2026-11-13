//! Format panel "Stencils" tab: draw.io-style shape libraries.
//!
//! Built-in libraries mirror draw.io's General, Basic, Flowchart and Arrows
//! sections; users can import draw.io `<mxlibrary>` files or versioned JSON
//! packages and collect board content in a scratchpad. Templates use the draw.io
//! decoder; package definitions use the shared interpreter. Both insert through
//! validated board clipboard payloads and retain the same click/drag behavior.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, OnceLock},
};

use egui::{Rect, Ui, vec2};
use thinoda_canvas::{BoardTool, PaintOptions, Xf};
use thinoda_core::{
    CanvasObjectKind, Layout, ShapeKind, Sheet, SheetRootMode, Vec2,
    clipboard::{BoardClipboardPayload, paste_board_selection},
    layout::{ApproxMeasure, layout_sheet},
    stencil::{StencilDefinition, StencilPackage},
};
use thinoda_io::drawio_library::{self, StencilEntry, StencilLibrary};
use thinoda_platform::stencils::{SavedLibrary, SavedPackage, StencilState};
use thinoda_ui::widgets;

use crate::{
    app::{Cmd, ThinodaApp},
    i18n::t,
};

/// Library ID of the user's scratchpad.
pub(crate) const SCRATCHPAD_ID: &str = "scratchpad";
const TILE: f32 = 44.0;
const GAP: f32 = 4.0;
/// Entries decoded per frame, so opening a large library never stalls.
const DECODES_PER_FRAME: usize = 12;
/// Decoded entries kept; the cache is rebuilt lazily after it is cleared.
const CACHE_LIMIT: usize = 600;
/// Approximate retained preview size; one oversized preview may exceed it.
const CACHE_ESTIMATED_BYTES: usize = 64 * 1024 * 1024;
/// Undo label of an inserted stencil.
pub(crate) const INSERT_LABEL: &str = "Insert stencil";

/// A library entry converted to board content, with a thumbnail sample.
pub(crate) struct Prepared {
    pub(crate) payload: Arc<BoardClipboardPayload>,
    pub(crate) bounds: thinoda_core::Rect,
    sheet: Sheet,
    layout: Layout,
    /// draw.io content that was omitted or simplified while converting.
    pub(crate) simplified: bool,
}

fn prepare(entry: &StencilEntry, dark: bool, builtin: bool) -> Option<Prepared> {
    let pasted = thinoda_io::drawio_clipboard::decode(&entry.xml).ok()?;
    let bounds = pasted.bounds?;
    let mut sheet = Sheet::new("Stencil", "");
    sheet.root_mode = SheetRootMode::Hidden;
    paste_board_selection(&mut sheet, &pasted.payload, Vec2::ZERO).ok()?;
    // Restyle only the built-in thumbnail; the insertion payload keeps its original style.
    if builtin && entry.title == "Double ellipse" {
        for object in &mut sheet.canvas.objects {
            if matches!(
                object.kind,
                CanvasObjectKind::Shape {
                    shape: ShapeKind::DoubleEllipse,
                    ..
                }
            ) {
                object.style.fill = None;
                object.style.stroke = None;
            }
        }
    }
    let layout = layout_sheet(&sheet, &ApproxMeasure, dark);
    Some(Prepared {
        payload: Arc::new(pasted.payload),
        bounds,
        sheet,
        layout,
        simplified: !pasted.warnings.is_empty() || !pasted.losses.is_empty(),
    })
}

fn prepare_definition(
    package: &StencilPackage,
    definition: &StencilDefinition,
    dark: bool,
) -> Result<Prepared, String> {
    let parameters = definition.resolve_parameters(&Default::default());
    let [width, height] = definition.default_size();
    package
        .evaluate(
            &definition.id,
            &thinoda_core::stencil::Instance {
                width,
                height,
                stroke_width: 1.0,
                parameters: &parameters,
            },
        )
        .map_err(|error| error.to_string())?;
    let mut sheet = Sheet::new("Stencil", "");
    sheet.root_mode = SheetRootMode::Hidden;
    let object = thinoda_core::stencil_object::new_object(package, definition, Vec2::ZERO);
    let id = sheet
        .add_canvas_object(object)
        .map_err(|error| error.to_string())?;
    let layout = layout_sheet(&sheet, &ApproxMeasure, dark);
    let payload = thinoda_core::clipboard::copy_board_selection(&sheet, &layout, &[], &[id])
        .map_err(|error| error.to_string())?
        .ok_or_else(|| t("The library has no usable shapes").to_owned())?;
    let bounds = payload
        .content_bounds()
        .ok_or_else(|| t("The library has no usable shapes").to_owned())?;
    Ok(Prepared {
        payload: Arc::new(payload),
        bounds,
        sheet,
        layout,
        simplified: false,
    })
}

fn library_decoded_bytes(library: &StencilLibrary) -> usize {
    library
        .entries
        .iter()
        .fold(0usize, |total, entry| total.saturating_add(entry.xml.len()))
}

fn builtin_libraries() -> &'static [Arc<StencilLibrary>] {
    static LIBRARIES: OnceLock<Vec<Arc<StencilLibrary>>> = OnceLock::new();
    LIBRARIES.get_or_init(|| {
        drawio_library::builtin_libraries()
            .iter()
            .cloned()
            .map(Arc::new)
            .collect()
    })
}

fn library_error(error: thinoda_io::IoError) -> String {
    match error {
        thinoda_io::IoError::Invalid(message) if message == drawio_library::DECODE_LIMIT_ERROR => {
            t("Stencil libraries exceed the size or processing limit").to_owned()
        }
        error => error.to_string(),
    }
}

fn saved_state_error(error: String) -> String {
    if error == thinoda_platform::stencils::SIZE_LIMIT_ERROR {
        t("Stencil libraries exceed the size or processing limit").to_owned()
    } else {
        error
    }
}

fn prepared_cache_bytes(prepared: &Prepared) -> usize {
    let shared_points = prepared
        .sheet
        .canvas
        .objects
        .iter()
        .fold(0usize, |sum, object| {
            let bytes = match &object.kind {
                CanvasObjectKind::Ink(stroke) => stroke
                    .points
                    .len()
                    .saturating_mul(std::mem::size_of::<Vec2>())
                    .saturating_add(stroke.pressure.as_ref().map_or(0, |pressure| {
                        pressure.len().saturating_mul(std::mem::size_of::<f32>())
                    })),
                CanvasObjectKind::Connector(connector) => connector
                    .waypoints
                    .len()
                    .saturating_mul(std::mem::size_of::<Vec2>()),
                _ => 0,
            };
            sum.saturating_add(bytes)
        });
    let layout_items = prepared
        .layout
        .topics
        .len()
        .saturating_add(prepared.layout.connectors.len())
        .saturating_add(prepared.layout.boundaries.len())
        .saturating_add(prepared.layout.summaries.len())
        .saturating_add(prepared.layout.relationships.len());
    thinoda_core::document::approx_sheet_bytes(&prepared.sheet)
        .saturating_mul(2)
        .saturating_add(prepared.sheet.resources.byte_len().saturating_mul(2))
        .saturating_add(shared_points.saturating_mul(2))
        .saturating_add(layout_items.saturating_mul(512))
}

type CacheKey = (String, usize);

struct CachedPrepared {
    value: Option<Arc<Prepared>>,
    estimated_bytes: usize,
    failure: Option<String>,
}

/// Libraries, preferences and decoded entries of the Stencils tab.
#[derive(Default)]
pub(crate) struct StencilPanel {
    loaded: bool,
    /// `false` after a failed load, so a damaged file is never overwritten.
    persist: bool,
    pub(crate) state: StencilState,
    /// Parsed `state.libraries`, in the same order.
    imported: Vec<Arc<StencilLibrary>>,
    packages: Vec<(String, Arc<StencilPackage>)>,
    cache: HashMap<CacheKey, CachedPrepared>,
    cache_bytes: usize,
    cache_dark: bool,
    pub(crate) search: String,
}

/// A visible library section.
struct Section {
    id: String,
    title: String,
    builtin: bool,
    content: SectionContent,
}

enum SectionContent {
    Drawio(Arc<StencilLibrary>),
    Package(Arc<StencilPackage>),
}

#[derive(Clone, Copy)]
enum SectionEntry<'a> {
    Drawio(&'a StencilEntry),
    Definition(&'a StencilDefinition),
}

impl<'a> SectionEntry<'a> {
    fn title(self) -> &'a str {
        match self {
            Self::Drawio(entry) => &entry.title,
            Self::Definition(definition) => &definition.title,
        }
    }

    fn matches_query(self, query: &str) -> bool {
        self.title().to_lowercase().contains(query)
            || matches!(self, Self::Definition(definition) if definition.tags.iter().any(|tag| tag.to_lowercase().contains(query)))
    }
}

impl Section {
    fn entries(&self) -> Vec<(usize, SectionEntry<'_>)> {
        match &self.content {
            SectionContent::Drawio(library) => library
                .entries
                .iter()
                .enumerate()
                .map(|(index, entry)| (index, SectionEntry::Drawio(entry)))
                .collect(),
            SectionContent::Package(package) => package
                .definitions
                .iter()
                .enumerate()
                .map(|(index, definition)| (index, SectionEntry::Definition(definition)))
                .collect(),
        }
    }

    #[cfg(test)]
    fn library(&self) -> &Arc<StencilLibrary> {
        match &self.content {
            SectionContent::Drawio(library) => library,
            SectionContent::Package(_) => panic!("expected a draw.io template library"),
        }
    }
}

enum TileAction {
    Insert(Arc<Prepared>),
    Remove(usize),
    Unavailable(String),
}

fn parse_package(text: &str) -> Result<StencilPackage, String> {
    let package = StencilPackage::from_json(text).map_err(|error| error.to_string())?;
    if package.definitions.is_empty() {
        return Err(t("The library has no usable shapes").into());
    }
    if thinoda_core::stencil_object::find_package(&package.id).is_some() {
        return Err(t("A stencil package with this ID is already installed").into());
    }
    for definition in &package.definitions {
        thinoda_core::stencil_object::StencilRef::new(&package.id, &definition.id).validate()?;
    }
    Ok(package)
}

impl StencilPanel {
    /// Loads the saved state once. Returns a message when it was unusable.
    pub(crate) fn ensure_loaded(
        &mut self,
        paths: Option<&thinoda_platform::Paths>,
    ) -> Option<String> {
        if self.loaded {
            return None;
        }
        self.loaded = true;
        let Some(paths) = paths else {
            self.persist = false;
            self.reload_imported();
            return None;
        };
        match thinoda_platform::stencils::load(paths) {
            Ok(state) => {
                self.state = state;
                let (message, safe_to_save) = self.reload_imported();
                self.persist = safe_to_save;
                message
            }
            Err(error) => {
                log::warn!("stencils: {error}");
                self.persist = false;
                self.reload_imported();
                Some(error.to_string())
            }
        }
    }

    /// Parses saved libraries with one shared expansion and retention budget.
    fn reload_imported(&mut self) -> (Option<String>, bool) {
        let mut messages = Vec::new();
        let mut failed = false;
        let mut imported = Vec::new();
        let mut retained = 0usize;
        let mut work_remaining = drawio_library::MAX_DECODE_WORK_BYTES;
        self.state.libraries.retain(|saved| {
            let available = drawio_library::MAX_DECODED_BYTES.saturating_sub(retained);
            match drawio_library::parse_library_with_budget(
                &saved.title,
                &saved.xml,
                available,
                &mut work_remaining,
            ) {
                Ok(parsed) if saved.id == SCRATCHPAD_ID && parsed.skipped > 0 => {
                    messages.push(format!(
                        "{}: {} — {}",
                        saved.title,
                        t("Some library entries could not be used"),
                        parsed.skipped
                    ));
                    failed = true;
                    false
                }
                Ok(parsed) => {
                    if parsed.skipped > 0 {
                        messages.push(format!(
                            "{}: {} — {}",
                            saved.title,
                            t("Some library entries could not be used"),
                            parsed.skipped
                        ));
                    }
                    let mut library = parsed.library;
                    library.id = saved.id.clone();
                    retained += library_decoded_bytes(&library);
                    imported.push(Arc::new(library));
                    true
                }
                Err(error) => {
                    messages.push(format!("{}: {}", saved.title, library_error(error)));
                    failed = true;
                    false
                }
            }
        });
        self.imported = imported;
        let mut packages: Vec<(String, Arc<StencilPackage>)> = Vec::new();
        self.state.packages.retain(|saved| {
            let result = parse_package(&saved.json).and_then(|package| {
                if packages
                    .iter()
                    .any(|(_, installed)| installed.id == package.id)
                {
                    Err(t("A stencil package with this ID is already installed").to_owned())
                } else {
                    Ok(package)
                }
            });
            match result {
                Ok(package) => {
                    packages.push((saved.id.clone(), Arc::new(package)));
                    true
                }
                Err(error) => {
                    messages.push(format!("{}: {error}", saved.id));
                    failed = true;
                    false
                }
            }
        });
        self.packages = packages;
        if !self.imported.iter().any(|lib| lib.id == SCRATCHPAD_ID) {
            if self.library_count() >= thinoda_platform::stencils::MAX_LIBRARIES {
                messages.push(t("Too many stencil libraries; remove one first").to_owned());
                failed = true;
            }
            let scratchpad = StencilLibrary {
                id: SCRATCHPAD_ID.into(),
                title: "Scratchpad".into(),
                entries: Vec::new(),
            };
            self.state.libraries.insert(
                0,
                SavedLibrary {
                    id: SCRATCHPAD_ID.into(),
                    title: scratchpad.title.clone(),
                    xml: drawio_library::encode_library(&scratchpad),
                },
            );
            self.imported.insert(0, Arc::new(scratchpad));
        }
        self.clear_cache();
        ((!messages.is_empty()).then(|| messages.join("\n")), !failed)
    }

    /// Saves the state when persistence is available.
    pub(crate) fn save(&self, paths: Option<&thinoda_platform::Paths>) -> Result<(), String> {
        match paths {
            Some(paths) if self.persist => {
                thinoda_platform::stencils::save(paths, &self.state).map_err(|e| e.to_string())
            }
            Some(_) => Err(t(
                "Changes cannot be saved because existing stencil libraries could not be loaded",
            )
            .to_owned()),
            None => Ok(()),
        }
    }

    fn sections(&self) -> Vec<Section> {
        let imported = self.imported.iter().map(|library| Section {
            id: library.id.clone(),
            title: if library.id == SCRATCHPAD_ID {
                t("Scratchpad").to_owned()
            } else {
                library.title.clone()
            },
            builtin: false,
            content: SectionContent::Drawio(Arc::clone(library)),
        });
        let builtin = builtin_libraries()
            .iter()
            .filter(|library| !self.state.hidden_builtin.contains(&library.id))
            .map(|library| Section {
                id: library.id.clone(),
                title: builtin_title(&library.title),
                builtin: true,
                content: SectionContent::Drawio(Arc::clone(library)),
            });
        let packages = self.packages.iter().map(|(id, package)| Section {
            id: id.clone(),
            title: if package.title.trim().is_empty() {
                package.id.clone()
            } else {
                package.title.clone()
            },
            builtin: false,
            content: SectionContent::Package(Arc::clone(package)),
        });
        // Like draw.io: the scratchpad first, then the shape sections, then imports.
        let (scratchpad, others): (Vec<_>, Vec<_>) =
            imported.partition(|section| section.id == SCRATCHPAD_ID);
        scratchpad
            .into_iter()
            .chain(builtin)
            .chain(others)
            .chain(packages)
            .collect()
    }

    fn prepared(
        &mut self,
        section: &Section,
        index: usize,
        entry: SectionEntry<'_>,
        dark: bool,
        budget: &mut usize,
    ) -> Option<Option<Arc<Prepared>>> {
        if self.cache_dark != dark {
            self.clear_cache();
            self.cache_dark = dark;
        }
        let key = (section.id.clone(), index);
        if let Some(prepared) = self.cache.get(&key) {
            return Some(prepared.value.clone());
        }
        if *budget == 0 {
            return None;
        }
        *budget -= 1;
        let result = match (&section.content, entry) {
            (SectionContent::Drawio(_), SectionEntry::Drawio(entry)) => {
                prepare(entry, dark, section.builtin)
                    .ok_or_else(|| t("Some library entries could not be used").to_owned())
            }
            (SectionContent::Package(package), SectionEntry::Definition(definition)) => {
                prepare_definition(package, definition, dark)
            }
            _ => Err(t("Some library entries could not be used").to_owned()),
        };
        let failure = result.as_ref().err().cloned();
        let prepared = result.ok().map(Arc::new);
        let source_bytes = match entry {
            SectionEntry::Drawio(entry) => entry.xml.len(),
            SectionEntry::Definition(_) => 0,
        };
        let estimated_bytes = prepared.as_ref().map_or(256, |prepared| {
            prepared_cache_bytes(prepared).saturating_add(source_bytes)
        });
        self.remember(
            key.clone(),
            prepared.clone(),
            estimated_bytes,
            CACHE_ESTIMATED_BYTES,
        );
        if let Some(cached) = self.cache.get_mut(&key) {
            cached.failure = failure;
        }
        Some(prepared)
    }

    fn remember(
        &mut self,
        key: CacheKey,
        value: Option<Arc<Prepared>>,
        estimated_bytes: usize,
        limit: usize,
    ) {
        if let Some(previous) = self.cache.remove(&key) {
            self.cache_bytes = self.cache_bytes.saturating_sub(previous.estimated_bytes);
        }
        if self.cache.len() >= CACHE_LIMIT
            || self.cache_bytes.saturating_add(estimated_bytes) > limit
        {
            self.clear_cache();
        }
        self.cache_bytes = self.cache_bytes.saturating_add(estimated_bytes);
        self.cache.insert(
            key,
            CachedPrepared {
                value,
                estimated_bytes,
                failure: None,
            },
        );
    }

    fn clear_cache(&mut self) {
        self.cache.clear();
        self.cache_bytes = 0;
    }

    fn invalidate(&mut self, library: &str) {
        self.cache.retain(|(id, _), _| id != library);
        self.cache_bytes = self.cache.values().fold(0usize, |total, value| {
            total.saturating_add(value.estimated_bytes)
        });
    }

    /// Adds a parsed draw.io library; returns its ID.
    #[cfg(test)]
    pub(crate) fn import(&mut self, title: &str, text: &str) -> Result<(String, usize), String> {
        let mut work_remaining = drawio_library::MAX_DECODE_WORK_BYTES;
        self.import_with_work(title, text, &mut work_remaining)
    }

    fn import_with_work(
        &mut self,
        title: &str,
        text: &str,
        work_remaining: &mut usize,
    ) -> Result<(String, usize), String> {
        if text.trim_start().starts_with('{') {
            return self.import_package(text);
        }
        if self.library_count() >= thinoda_platform::stencils::MAX_LIBRARIES {
            return Err(t("Too many stencil libraries; remove one first").into());
        }
        let available = drawio_library::MAX_DECODED_BYTES.saturating_sub(self.retained_bytes());
        let parsed =
            drawio_library::parse_library_with_budget(title, text, available, work_remaining)
                .map_err(library_error)?;
        if parsed.library.entries.is_empty() {
            return Err(t("The library has no usable shapes").into());
        }
        let id = self.next_library_id()?;
        let mut library = parsed.library;
        library.id = id.clone();
        let title = library
            .title
            .char_indices()
            .take_while(|(at, c)| at + c.len_utf8() <= thinoda_platform::stencils::MAX_TITLE_BYTES)
            .map(|(_, c)| c)
            .collect::<String>();
        let title = if title.is_empty() {
            t("Library").to_owned()
        } else {
            title
        };
        library.title = title.clone();
        self.state.libraries.push(SavedLibrary {
            id: id.clone(),
            title,
            xml: text.to_owned(),
        });
        if let Err(error) = thinoda_platform::stencils::validate(&self.state) {
            self.state.libraries.pop();
            return Err(saved_state_error(error));
        }
        self.imported.push(Arc::new(library));
        Ok((id, parsed.skipped))
    }

    fn library_count(&self) -> usize {
        self.state.libraries.len() + self.state.packages.len()
    }

    fn next_library_id(&self) -> Result<String, String> {
        (1..=thinoda_platform::stencils::MAX_LIBRARIES)
            .map(|n| format!("lib-{n}"))
            .find(|id| {
                self.state.libraries.iter().all(|saved| &saved.id != id)
                    && self.state.packages.iter().all(|saved| &saved.id != id)
            })
            .ok_or_else(|| t("Too many stencil libraries; remove one first").to_owned())
    }

    fn import_package(&mut self, text: &str) -> Result<(String, usize), String> {
        if self.library_count() >= thinoda_platform::stencils::MAX_LIBRARIES {
            return Err(t("Too many stencil libraries; remove one first").into());
        }
        let package = parse_package(text)?;
        if self
            .packages
            .iter()
            .any(|(_, installed)| installed.id == package.id)
        {
            return Err(t("A stencil package with this ID is already installed").into());
        }
        let id = self.next_library_id()?;
        let unavailable = package
            .definitions
            .iter()
            .filter(|definition| !definition.missing_capabilities().is_empty())
            .count();
        self.state.packages.push(SavedPackage {
            id: id.clone(),
            json: text.to_owned(),
        });
        if let Err(error) = thinoda_platform::stencils::validate(&self.state) {
            self.state.packages.pop();
            return Err(saved_state_error(error));
        }
        self.packages.push((id.clone(), Arc::new(package)));
        Ok((id, unavailable))
    }

    fn retained_bytes(&self) -> usize {
        self.imported
            .iter()
            .map(|library| library_decoded_bytes(library))
            .sum::<usize>()
            .saturating_add(
                self.state
                    .packages
                    .iter()
                    .map(|saved| saved.json.len())
                    .sum::<usize>(),
            )
    }

    pub(crate) fn remove_library(&mut self, id: &str) {
        if id == SCRATCHPAD_ID {
            return;
        }
        self.state.libraries.retain(|saved| saved.id != id);
        self.imported.retain(|library| library.id != id);
        self.state.packages.retain(|package| package.id != id);
        self.packages.retain(|(package_id, _)| package_id != id);
        self.state.collapsed.retain(|collapsed| collapsed != id);
        self.invalidate(id);
    }

    fn scratchpad_mut(&mut self) -> Option<&mut StencilLibrary> {
        self.imported
            .iter_mut()
            .find(|library| library.id == SCRATCHPAD_ID)
            .map(Arc::make_mut)
    }

    fn store_scratchpad(&mut self) {
        let Some(xml) = self
            .imported
            .iter()
            .find(|library| library.id == SCRATCHPAD_ID)
            .map(|library| drawio_library::encode_library(library))
        else {
            return;
        };
        if let Some(saved) = self
            .state
            .libraries
            .iter_mut()
            .find(|saved| saved.id == SCRATCHPAD_ID)
        {
            saved.xml = xml;
        }
        self.invalidate(SCRATCHPAD_ID);
    }

    pub(crate) fn add_to_scratchpad(&mut self, entry: StencilEntry) -> Result<(), String> {
        let index = self
            .imported
            .iter()
            .position(|library| library.id == SCRATCHPAD_ID)
            .ok_or_else(|| t("Not possible here"))?;
        let scratchpad = &self.imported[index];
        if scratchpad.entries.len() >= drawio_library::MAX_LIBRARY_ENTRIES
            || library_decoded_bytes(scratchpad).saturating_add(entry.xml.len())
                > drawio_library::MAX_LIBRARY_BYTES
            || self.retained_bytes().saturating_add(entry.xml.len())
                > drawio_library::MAX_DECODED_BYTES
        {
            return Err(t("The scratchpad is full").into());
        }
        let mut candidate = scratchpad.as_ref().clone();
        candidate.entries.push(entry);
        let xml = drawio_library::encode_library(&candidate);
        if xml.len() > drawio_library::MAX_LIBRARY_BYTES {
            return Err(t("The scratchpad is full").into());
        }
        let saved = self
            .state
            .libraries
            .iter()
            .position(|library| library.id == SCRATCHPAD_ID)
            .ok_or_else(|| t("Not possible here"))?;
        let previous = std::mem::replace(&mut self.state.libraries[saved].xml, xml);
        if let Err(error) = thinoda_platform::stencils::validate(&self.state) {
            self.state.libraries[saved].xml = previous;
            return Err(saved_state_error(error));
        }
        self.imported[index] = Arc::new(candidate);
        self.invalidate(SCRATCHPAD_ID);
        Ok(())
    }

    pub(crate) fn remove_scratchpad_entry(&mut self, index: usize) {
        if let Some(scratchpad) = self.scratchpad_mut()
            && index < scratchpad.entries.len()
        {
            scratchpad.entries.remove(index);
            self.store_scratchpad();
        }
    }

    #[cfg(test)]
    pub(crate) fn scratchpad_len(&self) -> usize {
        self.imported
            .iter()
            .find(|library| library.id == SCRATCHPAD_ID)
            .map_or(0, |library| library.entries.len())
    }

    fn set_collapsed(&mut self, id: &str, collapsed: bool) {
        self.state.collapsed.retain(|entry| entry != id);
        if collapsed && self.state.collapsed.len() < thinoda_platform::stencils::MAX_IDS {
            self.state.collapsed.push(id.to_owned());
        }
    }

    fn set_builtin_visible(&mut self, id: &str, visible: bool) {
        self.state.hidden_builtin.retain(|entry| entry != id);
        if !visible {
            self.state.hidden_builtin.push(id.to_owned());
        }
    }
}

/// English titles of the built-in libraries and their entries, as catalog keys.
pub(crate) const BUILTIN_TITLES: &[&str] = &[
    "General",
    "Basic",
    "Flowchart",
    "Arrows",
    "Rectangle",
    "Rounded rectangle",
    "Text",
    "Ellipse",
    "Square",
    "Circle",
    "Double ellipse",
    "Diamond",
    "Parallelogram",
    "Hexagon",
    "Triangle",
    "Cylinder",
    "Cloud",
    "Document",
    "Chevron",
    "Trapezoid",
    "Callout",
    "Plus",
    "Line",
    "Arrow",
    "Bidirectional arrow",
    "Parametric shapes",
    "Message",
    "Folder",
    "Note",
    "Card",
    "Cube",
    "Star",
    "Pentagon",
    "Octagon",
    "Right triangle",
    "Process",
    "Decision",
    "Terminator",
    "Data",
    "Predefined process",
    "Manual input",
    "Preparation",
    "Database",
    "Delay",
    "Off-page connector",
    "On-page reference",
    "Extract",
    "Merge",
    "Double arrow",
    "draw.io shapes",
    "Tape",
    "Data storage",
    "Internal storage",
    "Corner",
    "Tee",
    "Or gate",
    "Xor gate",
    "Loop limit",
    "Display",
    "Curly bracket",
    "Sequential data",
    "Or",
    "Summing junction",
    "Data store",
    "Dimension",
    "Switch",
    "Actor",
    "Frame",
    "Module",
    "Component",
    "Provided interface",
    "Required interface",
    "Isometric square",
    "U-turn arrow",
    "Partial concentric ellipse",
    "Arc",
    "Ribbon banner",
    "Shaded cube",
    "Pyramid step",
    "Shaded cylinder",
];

/// Catalog key of a built-in title.
fn builtin_key(title: &str) -> Option<&'static str> {
    BUILTIN_TITLES.iter().copied().find(|key| *key == title)
}

pub(crate) fn builtin_title(title: &str) -> String {
    builtin_key(title).map_or_else(|| title.to_owned(), |key| t(key).to_owned())
}

/// Localizes built-in entry titles; imported titles are shown as written.
fn entry_title(section: &Section, entry: SectionEntry<'_>) -> String {
    let title = entry.title();
    if title.is_empty() {
        t("Untitled").to_owned()
    } else if section.builtin {
        builtin_title(title)
    } else {
        title.to_owned()
    }
}

fn localize_edit_error(error: thinoda_core::EditError) -> String {
    crate::i18n::localize_model_error(&error.to_string()).to_owned()
}

fn paint_sample(painter: &egui::Painter, prepared: &Prepared, rect: Rect, dark: bool) {
    let bounds = prepared.bounds;
    let rect = rect.shrink(4.0);
    let zoom = (rect.width() / bounds.size.x.max(1.0))
        .min(rect.height() / bounds.size.y.max(1.0))
        .min(2.0);
    let center = bounds.center();
    let xf = Xf {
        origin: rect.center() - vec2(center.x, center.y) * zoom,
        zoom,
    };
    thinoda_canvas::paint_map(
        &painter.with_clip_rect(rect.expand(2.0)),
        &prepared.sheet,
        &prepared.layout,
        xf,
        &PaintOptions {
            dark,
            ..Default::default()
        },
    );
}

impl ThinodaApp {
    /// `true` when stencils can be inserted into the active sheet.
    pub(crate) fn stencils_available(&self) -> bool {
        self.command_available(&Cmd::BoardTool(BoardTool::Shape(ShapeKind::Rectangle)))
            && self.doc().is_some_and(|doc| doc.doc.sheet().fim.is_none())
    }

    fn ensure_stencils_loaded(&mut self) {
        let paths = self.services.paths.clone();
        if let Some(error) = self.stencils.ensure_loaded(paths.as_ref()) {
            if self.stencils.persist {
                self.toasts.warning(error);
            } else {
                self.toasts.error(format!(
                    "{}: {error}",
                    t("Could not load stencil libraries")
                ));
            }
        }
    }

    fn save_stencils(&mut self) {
        let paths = self.services.paths.clone();
        if let Err(error) = self.stencils.save(paths.as_ref()) {
            self.toasts.error(format!(
                "{}: {error}",
                t("Could not save stencil libraries")
            ));
        }
    }

    pub(crate) fn stencils_tab(&mut self, ui: &mut Ui) {
        self.ensure_stencils_loaded();
        let available = self.stencils_available();
        let dark = ui.visuals().dark_mode;

        let mut search = std::mem::take(&mut self.stencils.search);
        let width = ui.available_width();
        ui.add(
            egui::TextEdit::singleline(&mut search)
                .hint_text(t("Search stencils"))
                .desired_width(width),
        )
        .accessible_name(t("Search stencils"));
        self.stencils.search = search;
        if available {
            widgets::caption(ui, t("Click to insert or drag onto the board"));
        } else {
            widgets::caption(ui, t("Open a board or mind map to insert stencils"));
        }
        ui.add_space(4.0);

        let query = self.stencils.search.trim().to_lowercase();
        let sections = self.stencils.sections();
        let mut budget = DECODES_PER_FRAME;
        let mut action = None;
        let mut add_selection = false;
        let mut remove_library = None;
        let mut collapse_changes = Vec::new();
        let mut pending = false;

        ui.add_enabled_ui(available, |ui| {
            if query.is_empty() {
                for section in &sections {
                    let collapsed = self.stencils.state.collapsed.contains(&section.id);
                    let header =
                        egui::CollapsingHeader::new(egui::RichText::new(&section.title).strong())
                            .id_salt(("stencil_section", &section.id))
                            .default_open(!collapsed)
                            .show(ui, |ui| {
                                if section.id == SCRATCHPAD_ID {
                                    let response = ui
                                        .add(egui::Button::new(t("Add selection")))
                                        .on_hover_text(t(
                                            "Add the selected board items to the scratchpad",
                                        ));
                                    add_selection |= response.clicked()
                                        && widgets::touch_activation_allowed(ui.ctx());
                                    if section.entries().is_empty() {
                                        widgets::caption(
                                            ui,
                                            t("Select board items and add them here for reuse"),
                                        );
                                    }
                                }
                                let entries = section.entries();
                                let (picked, waiting) =
                                    self.stencil_grid(ui, section, &entries, dark, &mut budget);
                                action = action.take().or(picked);
                                pending |= waiting;
                            });
                    if header.header_response.clicked() {
                        // Read egui's toggled state rather than inverting ours,
                        // which can lag behind restored UI memory.
                        let open = egui::collapsing_header::CollapsingState::load(
                            ui.ctx(),
                            ui.make_persistent_id(("stencil_section", &section.id)),
                        )
                        .is_some_and(|state| state.is_open());
                        collapse_changes.push((section.id.clone(), !open));
                    }
                    if !section.builtin && section.id != SCRATCHPAD_ID {
                        header.header_response.context_menu(|ui| {
                            if ui.button(t("Remove library")).clicked() {
                                remove_library = Some(section.id.clone());
                                ui.close();
                            }
                        });
                    }
                    if let SectionContent::Package(package) = &section.content {
                        header.header_response.on_hover_ui(|ui| {
                            ui.label(format!("{} · v{}", package.id, package.revision));
                            if let Some(license) = &package.license {
                                ui.label(license);
                            }
                            widgets::caption(
                                ui,
                                t("Removing a library does not change existing objects"),
                            );
                        });
                    }
                }
            } else {
                let mut found = false;
                for section in &sections {
                    let entries: Vec<_> = section
                        .entries()
                        .into_iter()
                        .filter(|(_, entry)| {
                            entry_title(section, *entry).to_lowercase().contains(&query)
                                || entry.matches_query(&query)
                                || section.title.to_lowercase().contains(&query)
                        })
                        .collect();
                    if entries.is_empty() {
                        continue;
                    }
                    found = true;
                    widgets::caption(ui, &section.title);
                    ui.push_id(("stencil_results", &section.id), |ui| {
                        let (picked, waiting) =
                            self.stencil_grid(ui, section, &entries, dark, &mut budget);
                        action = action.take().or(picked);
                        pending |= waiting;
                    });
                }
                if !found {
                    widgets::caption(ui, t("No results"));
                }
            }
        });
        if pending {
            ui.ctx().request_repaint();
        }

        ui.add_space(8.0);
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.menu_button(t("More shapes…"), |ui| {
                for library in drawio_library::builtin_libraries() {
                    let mut visible = !self.stencils.state.hidden_builtin.contains(&library.id);
                    if ui
                        .checkbox(&mut visible, builtin_title(&library.title))
                        .changed()
                    {
                        self.stencils.set_builtin_visible(&library.id, visible);
                        self.save_stencils();
                    }
                }
            });
            if ui.button(t("Import library…")).clicked()
                && widgets::touch_activation_allowed(ui.ctx())
            {
                self.pick_stencil_libraries();
            }
        });

        let mut changed = false;
        for (id, collapsed) in collapse_changes {
            self.stencils.set_collapsed(&id, collapsed);
            changed = true;
        }
        if let Some(id) = remove_library {
            self.stencils.remove_library(&id);
            changed = true;
        }
        match action {
            Some((_, TileAction::Insert(prepared))) => self.insert_stencil(ui.ctx(), &prepared),
            Some((_, TileAction::Remove(index))) => {
                self.stencils.remove_scratchpad_entry(index);
                changed = true;
            }
            Some((_, TileAction::Unavailable(error))) => {
                self.toasts.warning(error);
            }
            None => {}
        }
        if add_selection {
            changed |= self.add_selection_to_scratchpad();
        }
        if changed {
            self.save_stencils();
        }
    }

    /// Tiles of one section. Returns the picked action and whether entries
    /// are still waiting to be decoded.
    fn stencil_grid(
        &mut self,
        ui: &mut Ui,
        section: &Section,
        entries: &[(usize, SectionEntry<'_>)],
        dark: bool,
        budget: &mut usize,
    ) -> (Option<(usize, TileAction)>, bool) {
        let mut action = None;
        let mut pending = false;
        if entries.is_empty() {
            return (None, false);
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(GAP, GAP);
            for &(index, entry) in entries {
                let title = entry_title(section, entry);
                let (_, rect) = ui.allocate_space(egui::Vec2::splat(TILE));
                let visible = ui.is_rect_visible(rect);
                let mut failed = false;
                let prepared = if visible {
                    match self.stencils.prepared(section, index, entry, dark, budget) {
                        Some(prepared) => {
                            failed = prepared.is_none();
                            prepared
                        }
                        None => {
                            pending = true;
                            None
                        }
                    }
                } else {
                    None
                };
                let failure = failed.then(|| {
                    self.stencils
                        .cache
                        .get(&(section.id.clone(), index))
                        .and_then(|cached| cached.failure.clone())
                        .unwrap_or_else(|| t("Some library entries could not be used").to_owned())
                });
                if let Some(picked) = stencil_tile(
                    ui,
                    rect,
                    &section.id,
                    index,
                    &title,
                    prepared.as_ref(),
                    failure.as_deref(),
                    dark,
                    section.id == SCRATCHPAD_ID,
                ) {
                    action = action.take().or(Some((index, picked)));
                }
            }
        });
        (action, pending)
    }

    /// Inserts a stencil centred in the view as one undoable edit.
    fn insert_stencil(&mut self, ctx: &egui::Context, prepared: &Prepared) {
        if !self.stencils_available() {
            self.toasts.warning(t("Not possible here"));
            return;
        }
        if !self.finish_canvas_text_edit() {
            return;
        }
        let Some(doc) = self.doc_mut() else { return };
        match doc.view.paste_foreign_board(
            &mut doc.doc,
            &prepared.payload,
            Some(prepared.bounds),
            INSERT_LABEL,
        ) {
            Ok(_) => {
                ctx.memory_mut(|memory| memory.request_focus(thinoda_canvas::MapCanvas::id()));
                if prepared.simplified {
                    self.toasts.warning(t(
                        "Inserted; some draw.io content may have been omitted or simplified",
                    ));
                }
            }
            Err(error) => {
                self.toasts.error(format!(
                    "{}: {}",
                    t("Could not insert stencil"),
                    localize_edit_error(error)
                ));
            }
        }
    }

    /// Converts the board selection into a scratchpad entry.
    pub(crate) fn add_selection_to_scratchpad(&mut self) -> bool {
        self.ensure_stencils_loaded();
        let Some(doc) = self.doc_mut() else {
            return false;
        };
        let payload = match doc.view.selection_board_payload(&doc.doc) {
            Ok(Some(payload)) => payload,
            Ok(None) => {
                self.toasts
                    .warning(t("Select board items to add to the scratchpad"));
                return false;
            }
            Err(error) => {
                self.toasts.error(format!(
                    "{}: {}",
                    t("Could not add to the scratchpad"),
                    localize_edit_error(error)
                ));
                return false;
            }
        };
        let encoded = thinoda_io::drawio_clipboard::encode_with_report(&payload)
            .map_err(|error| error.to_string())
            .and_then(|copy| {
                let decoded = thinoda_io::drawio_clipboard::decode(&copy.text)
                    .map_err(|error| error.to_string())?;
                let size = decoded
                    .bounds
                    .map_or(Vec2::new(80.0, 80.0), |bounds| bounds.size);
                drawio_library::entry_from_clipboard("", &copy.text, size.x, size.y)
                    .map(|entry| (entry, !copy.losses.is_empty()))
                    .map_err(|error| error.to_string())
            });
        let result = encoded
            .and_then(|(entry, lossy)| self.stencils.add_to_scratchpad(entry).map(|()| lossy));
        match result {
            Ok(lossy) => {
                if lossy {
                    self.toasts.warning(t(
                        "Added to the scratchpad; some content may differ when inserted",
                    ));
                }
                true
            }
            Err(error) => {
                self.toasts.error(format!(
                    "{}: {}",
                    t("Could not add to the scratchpad"),
                    crate::i18n::localize_model_error(&error)
                ));
                false
            }
        }
    }

    fn pick_stencil_libraries(&mut self) {
        if let Some(paths) = crate::files::pick_files(
            crate::files::Purpose::StencilLibrary,
            Some(t("Import library…")),
            &[(t("Stencils"), &["xml", "json"])],
            None,
            true,
        ) {
            self.import_stencil_libraries(&paths);
        }
    }

    /// Imports picked draw.io template libraries or declarative JSON packages.
    pub(crate) fn import_stencil_libraries(&mut self, paths: &[PathBuf]) {
        self.ensure_stencils_loaded();
        let mut changed = false;
        let mut work_remaining = drawio_library::MAX_DECODE_WORK_BYTES;
        for path in paths {
            let name = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default();
            let result =
                thinoda_platform::fs::read_bounded(path, drawio_library::MAX_LIBRARY_BYTES)
                    .map_err(|error| error.to_string())
                    .and_then(|bytes| String::from_utf8(bytes).map_err(|error| error.to_string()))
                    .and_then(|text| {
                        if path
                            .extension()
                            .and_then(|extension| extension.to_str())
                            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
                        {
                            self.stencils.import_package(&text)
                        } else {
                            self.stencils
                                .import_with_work(&name, &text, &mut work_remaining)
                        }
                    });
            match result {
                Ok((_, skipped)) => {
                    changed = true;
                    if skipped > 0 {
                        self.toasts.warning(format!(
                            "{name}: {} — {skipped}",
                            t("Some library entries could not be used")
                        ));
                    }
                }
                Err(error) => {
                    self.toasts.error(format!(
                        "{}: {name}: {error}",
                        t("Could not import library")
                    ));
                }
            }
        }
        if changed {
            self.format_tab = thinoda_platform::settings::FormatTab::Stencils;
            self.save_stencils();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn stencil_tile(
    ui: &mut Ui,
    rect: Rect,
    library: &str,
    index: usize,
    title: &str,
    prepared: Option<&Arc<Prepared>>,
    failure: Option<&str>,
    dark: bool,
    removable: bool,
) -> Option<TileAction> {
    let palette = thinoda_ui::Palette::of_ui(ui);
    let touch_allowed = widgets::touch_activation_allowed(ui.ctx());
    let id = ui.make_persistent_id(("stencil_tile", library, index));
    let draggable = prepared.is_some() && !ui.input(|input| input.any_touches());
    let sense = if draggable {
        egui::Sense::click_and_drag()
    } else {
        egui::Sense::click()
    };
    let response = ui.interact(rect, id, sense);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), title));
    if response.gained_focus() {
        response.scroll_to_me(None);
    }
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered() || response.is_pointer_button_down_on();
        let fill = if hovered && ui.is_enabled() {
            ui.style().interact(&response).bg_fill
        } else {
            palette.surface_alt
        };
        ui.painter().rect(
            rect.shrink(0.5),
            7,
            fill,
            egui::Stroke::new(1.0, palette.border),
            egui::StrokeKind::Inside,
        );
        match prepared {
            Some(prepared) => paint_sample(ui.painter(), prepared, rect.shrink(2.0), dark),
            None => {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    if failure.is_some() { "×" } else { "…" },
                    egui::FontId::proportional(14.0),
                    palette.text_muted,
                );
            }
        }
        if response.has_focus() && ui.is_enabled() {
            ui.painter().rect_stroke(
                rect.shrink(2.0),
                6,
                egui::Stroke::new(2.0, palette.text),
                egui::StrokeKind::Inside,
            );
        }
    }
    let response = response.on_hover_ui(|ui| {
        ui.label(egui::RichText::new(title).strong());
        if let Some(error) = failure {
            widgets::caption(ui, error);
        }
        if let Some(prepared) = prepared {
            let (preview, _) = ui.allocate_exact_size(vec2(160.0, 120.0), egui::Sense::hover());
            let theme = &prepared.sheet.theme;
            ui.painter().rect_filled(
                preview,
                4,
                thinoda_canvas::color32(
                    thinoda_core::theme::Theme::for_settings(theme).background(theme, dark),
                ),
            );
            paint_sample(ui.painter(), prepared, preview, dark);
            if prepared.simplified {
                widgets::caption(ui, t("Some draw.io details are simplified"));
            }
        }
    });
    if removable {
        response.context_menu(|ui| {
            if ui.button(t("Remove from scratchpad")).clicked() {
                ui.close();
                // Handled below through the returned action.
                ui.ctx()
                    .data_mut(|data| data.insert_temp(id.with("remove"), true));
            }
        });
        if ui
            .ctx()
            .data_mut(|data| data.remove_temp::<bool>(id.with("remove")))
            .is_some()
        {
            return Some(TileAction::Remove(index));
        }
    }
    let keyboard = ui.is_enabled()
        && response.has_focus()
        && ui.input_mut(|input| {
            input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                || input.consume_key(egui::Modifiers::NONE, egui::Key::Space)
        });
    let activated = ui.is_enabled() && ((touch_allowed && response.clicked()) || keyboard);
    if let Some(error) = failure
        && activated
    {
        return Some(TileAction::Unavailable(error.to_owned()));
    }
    let prepared = prepared?;
    if ui.is_enabled() && response.drag_started() {
        response.dnd_set_drag_payload(thinoda_canvas::StencilDrop {
            payload: Arc::clone(&prepared.payload),
            bounds: prepared.bounds,
            label: INSERT_LABEL.into(),
            simplified: prepared.simplified,
        });
        return None;
    }
    activated.then(|| TileAction::Insert(Arc::clone(prepared)))
}

#[cfg(test)]
#[path = "stencils_tests.rs"]
mod tests;
