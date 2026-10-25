use std::collections::VecDeque;

use crate::{
    app::{CloseIntent, ThinodaApp},
    i18n::t,
};

pub(crate) struct SaveAll {
    remaining: VecDeque<String>,
    origin: Option<String>,
    close_after: Option<CloseIntent>,
}

impl ThinodaApp {
    pub(crate) fn begin_save_all(&mut self, ctx: &egui::Context, close_after: Option<CloseIntent>) {
        if self.save_all.is_some()
            || self.any_modal_open()
            || self.dialogs.settings
            || self.dialogs.about
        {
            self.toasts.warning(t("Not possible here"));
            return;
        }
        for doc in &mut self.docs {
            doc.view.commit_edit(&mut doc.doc);
        }
        self.save_all = Some(SaveAll {
            origin: self.doc().map(|doc| doc.key.clone()),
            remaining: self
                .docs
                .iter()
                .filter(|doc| doc.is_dirty())
                .map(|doc| doc.key.clone())
                .collect(),
            close_after,
        });
        self.continue_save_all(ctx);
    }

    fn restore_save_origin(&mut self) {
        if let Some(batch) = &self.save_all {
            self.active = batch
                .origin
                .as_ref()
                .and_then(|key| self.docs.iter().position(|doc| &doc.key == key))
                .unwrap_or(self.docs.len());
        }
    }

    fn stop_save_all(&mut self) {
        self.restore_save_origin();
        if let Some(batch) = self.save_all.take() {
            for key in batch.remaining {
                if let Some(doc) = self.docs.iter_mut().find(|doc| doc.key == key) {
                    doc.ignore_conflict = false;
                }
            }
        }
    }

    fn save_dialog_for(&self, key: &str) -> bool {
        self.dialogs.conflict.as_deref() == Some(key)
            || self
                .dialogs
                .mindmanager_save
                .as_ref()
                .is_some_and(|(pending, _, sheet_only)| pending == key && !sheet_only)
    }

    fn continue_save_all(&mut self, ctx: &egui::Context) {
        while let Some(key) = self
            .save_all
            .as_ref()
            .and_then(|batch| batch.remaining.front())
            .cloned()
        {
            let Some(index) = self.docs.iter().position(|doc| doc.key == key) else {
                self.toasts.warning(t("Not possible here"));
                self.stop_save_all();
                return;
            };
            self.active = index;
            if !self.docs[index].is_dirty() || self.save_active(false) {
                self.save_all.as_mut().unwrap().remaining.pop_front();
                continue;
            }
            if self.save_dialog_for(&key) {
                self.restore_save_origin();
            } else {
                self.stop_save_all();
            }
            return;
        }
        let close_after = self
            .save_all
            .as_ref()
            .and_then(|batch| batch.close_after.clone());
        self.stop_save_all();
        if let Some(intent) = close_after {
            if self.any_dirty() {
                self.toasts.warning(t("Not possible here"));
            } else {
                self.approved_close = Some(intent);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    /// Returns whether this dialog belongs to the batch, excluding tab-close saves.
    pub(crate) fn resolve_save_all(&mut self, ctx: &egui::Context, key: &str, saved: bool) -> bool {
        if self
            .save_all
            .as_ref()
            .and_then(|batch| batch.remaining.front())
            .is_none_or(|pending| pending != key)
        {
            return false;
        }
        if saved {
            self.continue_save_all(ctx);
        } else if self.save_dialog_for(key) {
            self.restore_save_origin();
        } else {
            self.stop_save_all();
        }
        true
    }
}
