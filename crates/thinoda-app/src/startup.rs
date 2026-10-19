//! Startup splash: shows real launch progress until the main UI is ready.
//!
//! eframe keeps the native window hidden until its first painted frame, so the splash is
//! the first thing that frame paints. Launch files are imported on a loader thread while
//! the UI thread keeps repainting; each result is added on the UI thread in launch order.
//! The application is *ready* once every launch file was opened or reported as an error
//! and the splash was presented; that frame draws the main UI instead of the splash and
//! only then are pending crash-recovery prompts and settings warnings shown.

use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, TryRecvError},
};
use web_time::Instant;

use egui::{Align, Color32, CornerRadius, Layout, Margin, RichText, Stroke, Ui};
use thinoda_platform::recovery::OrphanSession;
use thinoda_ui::tokens::Palette;

use crate::{Startup, ThinodaApp, doc::OpenDoc, i18n::t};

type Imported = Result<thinoda_io::ImportReport, String>;

/// What the splash currently waits for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchPhase {
    /// The window is being presented; nothing countable is pending.
    Starting,
    /// Launch file `index` (1-based) of `total` is being opened.
    Opening {
        index: usize,
        total: usize,
        name: String,
    },
}

/// Pending launch work; dropped once the application is ready.
pub struct Launch {
    pending: VecDeque<PathBuf>,
    total: usize,
    loader: Option<Receiver<(PathBuf, Imported)>>,
    presented: bool,
    orphans: Vec<OrphanSession>,
    warning: Option<String>,
    created: Instant,
}

impl Launch {
    fn new(files: &[PathBuf], orphans: Vec<OrphanSession>, warning: Option<String>) -> Self {
        let mut pending = VecDeque::new();
        for f in files {
            if !pending.contains(f) {
                pending.push_back(f.clone());
            }
        }
        let total = pending.len();
        Self {
            loader: spawn_loader(&pending),
            pending,
            total,
            presented: false,
            orphans,
            warning,
            created: Instant::now(),
        }
    }

    pub fn phase(&self) -> LaunchPhase {
        match self.pending.front() {
            Some(path) if self.presented => LaunchPhase::Opening {
                index: self.total - self.pending.len() + 1,
                total: self.total,
                name: display_name(path),
            },
            _ => LaunchPhase::Starting,
        }
    }

    /// Takes the next finished import, in launch order.
    fn next_result(&mut self) -> Option<(PathBuf, Imported)> {
        let expected = self.pending.front()?.clone();
        let result = match &self.loader {
            Some(rx) => match rx.try_recv() {
                Ok((path, result)) => {
                    debug_assert_eq!(path, expected);
                    result
                }
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Disconnected) => {
                    Err(t("The file loader stopped unexpectedly").to_string())
                }
            },
            // No loader thread could be started: import here, one file per frame.
            None => import(&expected),
        };
        self.pending.pop_front();
        Some((expected, result))
    }
}

fn import(path: &Path) -> Imported {
    crate::doc::import_report(path)
}

fn spawn_loader(paths: &VecDeque<PathBuf>) -> Option<Receiver<(PathBuf, Imported)>> {
    if paths.is_empty() {
        return None;
    }
    let (tx, rx) = mpsc::channel();
    let paths: Vec<PathBuf> = paths.iter().cloned().collect();
    match std::thread::Builder::new()
        .name("thinoda-open".into())
        .spawn(move || {
            for path in paths {
                let result = import(&path);
                if tx.send((path, result)).is_err() {
                    break;
                }
            }
        }) {
        Ok(_) => Some(rx),
        Err(e) => {
            log::warn!("could not start the file loader thread, opening on the UI thread: {e}");
            None
        }
    }
}

fn display_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

impl ThinodaApp {
    /// Creates the application for an interactive launch: the splash is shown until
    /// `files` are opened, then the main UI and any pending recovery prompt appear.
    pub fn launch(startup: Startup, files: &[PathBuf]) -> Self {
        let mut app = Self::new(startup.settings, startup.services, &[]);
        app.launch = Some(Launch::new(files, startup.orphans, startup.warning));
        app
    }

    /// The current launch phase, or `None` once the application is ready.
    pub fn launch_phase(&self) -> Option<LaunchPhase> {
        self.launch.as_ref().map(Launch::phase)
    }

    /// Advances startup and draws the splash. Returns `true` once the application is
    /// ready, in which case the caller draws the main UI in this same frame.
    pub fn launch_frame(&mut self, ui: &mut Ui) -> bool {
        let ctx = ui.ctx().clone();
        let Some(launch) = self.launch.as_mut() else {
            return true;
        };
        if launch.presented
            && let Some((path, result)) = launch.next_result()
        {
            self.add_opened(
                &path,
                result.and_then(|report| OpenDoc::from_import_report(&path, report)),
            );
        }
        let Some(launch) = self.launch.as_mut() else {
            return true;
        };
        if launch.presented && launch.pending.is_empty() {
            let launch = self.launch.take().expect("launch state checked above");
            log::info!(
                "startup ready {:?} after the splash was created ({} launch files)",
                launch.created.elapsed(),
                launch.total
            );
            self.dialogs.recovery = launch.orphans;
            if let Some(w) = launch.warning {
                self.toasts.warning(w);
            }
            return true;
        }
        let phase = launch.phase();
        let total = launch.total;
        launch.presented = true;

        self.apply_appearance(&ctx);
        // No document is open yet, so a close request simply quits; recovery data of
        // earlier sessions and unopened launch files stay untouched.
        self.handle_close_request(&ctx);
        splash(ui, &phase, total);
        ctx.request_repaint();
        false
    }
}

/// Draws the centered splash card.
///
/// The card is laid out inside the panel rather than in an `egui::Area`: areas skip
/// painting on their first frame (sizing pass), but the very first frame is the one
/// that makes the native window visible and must already show the splash.
fn splash(ui: &mut Ui, phase: &LaunchPhase, total: usize) {
    let palette = Palette::of(ui.ctx());
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(palette.bg))
        .show(ui, |ui| {
            let height_id = egui::Id::unique("thinoda_startup_splash_height");
            // Measured on the previous frame; the estimate only matters for frame one.
            let height = ui
                .ctx()
                .data(|d| d.get_temp::<f32>(height_id))
                .unwrap_or(190.0);
            let size = egui::vec2(CARD_WIDTH + 2.0 * CARD_MARGIN as f32, height);
            let rect = egui::Rect::from_center_size(ui.max_rect().center(), size);
            let card = ui
                .scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(rect)
                        .layout(Layout::top_down(Align::Min)),
                    |ui| {
                        egui::Frame::new()
                            .fill(palette.surface)
                            .stroke(Stroke::new(1.0, palette.border))
                            .corner_radius(CornerRadius::same(12))
                            .inner_margin(Margin::same(CARD_MARGIN))
                            .show(ui, |ui| {
                                ui.set_width(CARD_WIDTH);
                                splash_content(ui, phase, total, palette.text_muted);
                            })
                            .response
                            .rect
                    },
                )
                .inner;
            ui.ctx()
                .data_mut(|d| d.insert_temp(height_id, card.height()));
        });
}

const CARD_WIDTH: f32 = 340.0;
const CARD_MARGIN: i8 = 24;

fn splash_content(ui: &mut Ui, phase: &LaunchPhase, total: usize, muted: Color32) {
    ui.horizontal(|ui| {
        crate::brand::logo(ui, 72.0);
        ui.add_space(8.0);
        ui.vertical(|ui| {
            ui.heading(RichText::new("Thinoda").size(26.0).strong());
            ui.label(RichText::new(t(crate::brand::TAGLINE)).italics());
        });
    });
    ui.add_space(10.0);
    match phase {
        LaunchPhase::Starting => {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new());
                ui.label(t("Starting…"));
            });
        }
        LaunchPhase::Opening { index, name, .. } => {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new());
                ui.add(egui::Label::new(format!("{} {name}", t("Opening file:"))).truncate());
            });
            // Progress within one file cannot be measured; the spinner covers that case.
            if total > 1 {
                ui.add(
                    egui::ProgressBar::new((*index - 1) as f32 / total as f32)
                        .text(format!("{index} / {total}")),
                );
            }
        }
    }
    ui.add_space(6.0);
    ui.label(
        RichText::new(format!(
            "{} {}",
            t("Version"),
            thinoda_platform::system::app_version()
        ))
        .small()
        .color(muted),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_launch_files_are_opened_once() {
        let a = PathBuf::from("a.thinoda");
        let launch = Launch::new(&[a.clone(), "b.opml".into(), a], Vec::new(), None);
        assert_eq!(launch.total, 2);
        assert_eq!(launch.phase(), LaunchPhase::Starting);
    }

    #[test]
    fn no_launch_files_start_without_a_loader() {
        let launch = Launch::new(&[], Vec::new(), None);
        assert!(launch.loader.is_none());
        assert_eq!(launch.phase(), LaunchPhase::Starting);
    }
}
