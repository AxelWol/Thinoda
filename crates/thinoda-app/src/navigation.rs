//! Read-only navigation controls shared by desktop and web search.

use std::collections::BTreeSet;

use egui::Ui;
use thinoda_core::{markers, search::Completion};
use thinoda_ui::widgets;

use crate::{ThinodaApp, i18n::t};

fn choice<T: PartialEq + Clone>(
    ui: &mut Ui,
    values: &mut Vec<T>,
    value: T,
    label: &str,
    allowed: bool,
) {
    let mut selected = values.contains(&value);
    if ui.checkbox(&mut selected, label).changed() && allowed {
        values.retain(|existing| *existing != value);
        if selected {
            values.push(value);
        }
    }
}

impl ThinodaApp {
    pub(crate) fn search_facets_ui(&mut self, ui: &mut Ui, allowed: bool) {
        let before = self.search.facets.clone();
        ui.checkbox(&mut self.search.facets.notes_only, t("View All Notes"));
        let mut markers_present = BTreeSet::new();
        let mut labels = BTreeSet::new();
        if let Some(doc) = self.doc() {
            for (index, sheet) in doc.doc.workbook().sheets.iter().enumerate() {
                if !self.search.all_sheets && index != doc.doc.active_index() {
                    continue;
                }
                for topic in sheet.topics.values() {
                    markers_present.extend(
                        topic
                            .markers
                            .iter()
                            .filter(|m| !m.starts_with("priority-"))
                            .cloned(),
                    );
                    labels.extend(topic.labels.iter().cloned());
                }
            }
        }
        // Selected values remain removable after their last matching topic is deleted.
        markers_present.extend(self.search.facets.markers.iter().cloned());
        labels.extend(self.search.facets.labels.iter().cloned());
        egui::CollapsingHeader::new(t("Filters"))
            .id_salt("search_facets")
            .show(ui, |ui| {
                widgets::caption(
                    ui,
                    t("Any selected value per group; all selected groups must match."),
                );
                for (heading, values) in [
                    (t("To-Do"), &mut self.search.facets.todos),
                    (t("Task Planning"), &mut self.search.facets.tasks),
                ] {
                    for (value, status) in [
                        (Completion::Incomplete, t("Incomplete")),
                        (Completion::Complete, t("Completed")),
                    ] {
                        choice(ui, values, value, &format!("{heading}: {status}"), allowed);
                    }
                }
                ui.label(t("Priority"));
                ui.horizontal_wrapped(|ui| {
                    for priority in 1..=9 {
                        choice(
                            ui,
                            &mut self.search.facets.priorities,
                            priority,
                            &priority.to_string(),
                            allowed,
                        );
                    }
                });
                if !markers_present.is_empty() {
                    ui.label(t("Markers"));
                    for marker in markers_present {
                        let name = markers::GROUPS
                            .iter()
                            .flat_map(|g| g.markers)
                            .find(|m| m.id == marker)
                            .map(|m| t(m.name))
                            .unwrap_or(&marker);
                        let label = format!("{} {name}", markers::glyph(&marker));
                        choice(ui, &mut self.search.facets.markers, marker, &label, allowed);
                    }
                }
                if !labels.is_empty() {
                    ui.label(t("Labels"));
                    for label in labels {
                        choice(
                            ui,
                            &mut self.search.facets.labels,
                            label.clone(),
                            &label,
                            allowed,
                        );
                    }
                }
            });
        if !allowed {
            self.search.facets = before;
        }
        if (!self.search.query.is_empty() || self.search.facets.active())
            && ui.button(t("Clear search and filters")).clicked()
            && allowed
        {
            self.search.query.clear();
            self.search.facets = Default::default();
        }
        self.update_search();
        if !self.search.facets.active() {
            return;
        }
        let rows: Vec<_> = self
            .search
            .hits
            .iter()
            .take(100)
            .filter_map(|hit| {
                let sheet = self.doc()?.doc.workbook().sheets.get(hit.sheet)?;
                let topic = sheet.topic(hit.topic)?;
                let title = format!("{} / {}", sheet.title, topic.title);
                let note: String = topic
                    .notes
                    .as_deref()
                    .unwrap_or("")
                    .chars()
                    .take(240)
                    .collect();
                Some((hit.sheet, hit.topic, title, note))
            })
            .collect();
        if self.search.hits.len() > 100 {
            widgets::caption(
                ui,
                t("First 100 results shown. Use Previous or Next for all results."),
            );
        }
        for (sheet, topic, title, note) in rows {
            ui.push_id((sheet, topic), |ui| {
                if ui.add(egui::Button::new(&title).wrap()).clicked() && allowed {
                    // Resolve by identity again: never dispatch a stale row index.
                    self.update_search();
                    if let Some(index) = self
                        .search
                        .hits
                        .iter()
                        .position(|hit| hit.sheet == sheet && hit.topic == topic)
                    {
                        self.search.current = index;
                        self.goto_hit(0);
                    }
                }
                if self.search.facets.notes_only {
                    ui.add(egui::Label::new(note).wrap());
                }
            });
        }
    }
}
