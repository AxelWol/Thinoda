//! Thread management stays outside document/export state.
use super::*;
use thinoda_platform::chat_history::{self as disk, Binding, Entry, History, Store, Thread};

#[cfg(test)]
#[path = "chat_history_tests.rs"]
mod tests;

#[derive(Default)]
pub(super) struct HistoryState {
    store: Option<Store>,
    loaded: bool,
    visited: BTreeSet<String>,
    pub(super) documents: BTreeMap<String, DurableDoc>,
    detached: BTreeMap<String, DurableDoc>,
    error: Option<String>,
    name: String,
    confirm: Option<Action>,
    retention: Option<u32>,
    owner: Option<(String, String, egui::ViewportId)>,
    pub(super) composing: bool,
    blocked_binding: BTreeMap<String, Binding>,
}

impl HistoryState {
    pub(super) fn confirming(&self) -> bool {
        self.confirm.is_some()
    }
}

pub(super) struct DurableDoc {
    binding: Binding,
    session: Conversation,
    parked: BTreeMap<String, Conversation>,
}

#[derive(Clone, Copy)]
enum Action {
    Enable,
    Disable,
    Delete,
    Retain,
    DeleteAll,
}

fn now() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn conversation(thread: &Thread) -> Conversation {
    Conversation {
        thread_id: thread.id.clone(),
        messages: thread
            .messages
            .iter()
            .map(|message| Message {
                role: if message.assistant {
                    "assistant"
                } else {
                    "user"
                }
                .into(),
                content: message.text.clone(),
            })
            .collect(),
        ..Default::default()
    }
}

impl ThinodaApp {
    fn chat_commit(&mut self, next: Store) -> bool {
        let result = self
            .services
            .paths
            .as_ref()
            .zip(self.ai.chat.history.store.as_ref())
            .ok_or_else(|| "Local history storage is unavailable".to_string())
            .and_then(|(paths, store)| {
                store.commit(paths, next).map_err(|error| error.to_string())
            });
        match result {
            Ok(store) => {
                self.ai.chat.history.store = Some(store);
                self.ai.chat.history.error = None;
                true
            }
            Err(error) => {
                self.ai.chat.history.error = Some(error);
                false
            }
        }
    }

    fn load_chat_history(&mut self, key: &str) {
        if !self.ai.chat.history.loaded {
            self.ai.chat.history.loaded = true;
            if let Some(paths) = &self.services.paths {
                match Store::load(paths) {
                    Ok(store) => {
                        self.ai.chat.history.store = Some(store);
                        self.ai.chat.history.error = None;
                    }
                    Err(error) => self.ai.chat.history.error = Some(error.to_string()),
                }
            }
        }
        if !self.ai.chat.history.visited.insert(key.into()) {
            return;
        }
        let Some(binding) = self.doc().and_then(|doc| doc.chat_identity.clone()) else {
            return;
        };
        // A second open tab is not a second writer to the same persisted history.
        if self
            .ai
            .chat
            .history
            .documents
            .values()
            .any(|doc| doc.binding == binding)
        {
            return;
        }
        let history = self
            .ai
            .chat
            .history
            .store
            .as_ref()
            .and_then(|store| store.documents.iter().find(|doc| doc.binding == binding))
            .cloned();
        if let Some(history) = history {
            self.attach_chat_history(key, history);
        }
    }

    fn attach_chat_history(&mut self, key: &str, history: History) {
        let mut session = self.ai.chat.threads.remove(key).unwrap_or_default();
        let mut parked: BTreeMap<_, _> = history
            .threads
            .iter()
            .map(|thread| (thread.id.clone(), conversation(thread)))
            .collect();
        if let Some(id) = history.threads.first().map(|thread| thread.id.clone()) {
            self.ai
                .chat
                .threads
                .insert(key.into(), parked.remove(&id).unwrap());
        } else {
            // Until the first named thread is created, keep the original
            // session conversation visible rather than replacing it with an
            // empty shadow session that could displace its unsent draft.
            self.ai
                .chat
                .threads
                .insert(key.into(), std::mem::take(&mut session));
        }
        self.ai.chat.history.documents.insert(
            key.into(),
            DurableDoc {
                binding: history.binding,
                session,
                parked,
            },
        );
    }

    /// Only successful saves in this OpenDoc can advance its content binding.
    /// Save As detaches; recovery never supplies an automatic binding.
    pub(super) fn sync_chat_bindings(&mut self) {
        self.ai
            .chat
            .history
            .visited
            .retain(|key| self.docs.iter().any(|doc| &doc.key == key));
        self.ai
            .chat
            .history
            .detached
            .retain(|key, _| self.docs.iter().any(|doc| &doc.key == key));
        self.ai
            .chat
            .history
            .blocked_binding
            .retain(|key, _| self.docs.iter().any(|doc| &doc.key == key));
        let keys: Vec<_> = self.ai.chat.history.documents.keys().cloned().collect();
        for key in keys {
            let Some(doc) = self.docs.iter().find(|doc| doc.key == key) else {
                self.ai.chat.history.documents.remove(&key);
                continue;
            };
            let old = self.ai.chat.history.documents[&key].binding.clone();
            let new = doc.chat_identity.clone();
            if new.as_ref() == Some(&old) {
                continue;
            }
            if let Some(new) = new.filter(|new| new.path == old.path) {
                if self.ai.chat.history.blocked_binding.get(&key) == Some(&new) {
                    continue;
                }
                self.ai
                    .chat
                    .history
                    .blocked_binding
                    .insert(key.clone(), new.clone());
                let Some(mut next) = self.ai.chat.history.store.clone() else {
                    continue;
                };
                if next.documents.iter().any(|doc| doc.binding == new) {
                    self.ai.chat.history.error = Some("History binding conflict".into());
                    continue;
                }
                if let Some(history) = next.documents.iter_mut().find(|doc| doc.binding == old) {
                    history.binding = new.clone();
                    if self.chat_commit(next) {
                        self.ai
                            .chat
                            .history
                            .documents
                            .get_mut(&key)
                            .unwrap()
                            .binding = new;
                        self.ai.chat.history.blocked_binding.remove(&key);
                    }
                }
            } else {
                // Preserve visible messages/draft, but never transfer durability to
                // a different path, unsaved/recovered document, or missing identity.
                let local = self.ai.chat.history.documents.remove(&key).unwrap();
                self.ai.chat.history.detached.insert(key, local);
            }
        }
    }

    pub(super) fn persist_chat_thread(&mut self, key: &str) {
        let Some(local) = self.ai.chat.history.documents.get(key) else {
            return;
        };
        if self
            .docs
            .iter()
            .find(|doc| doc.key == key)
            .and_then(|doc| doc.chat_identity.as_ref())
            != Some(&local.binding)
        {
            self.ai.chat.history.error = Some(
                "Document history binding could not be saved; transcript remains in memory".into(),
            );
            return;
        }
        let Some(thread) = self.ai.chat.threads.get(key) else {
            return;
        };
        if thread.thread_id.is_empty() {
            return;
        }
        let Some(mut next) = self.ai.chat.history.store.clone() else {
            return;
        };
        let Some(saved) = next
            .documents
            .iter_mut()
            .find(|doc| doc.binding == local.binding)
            .and_then(|doc| {
                doc.threads
                    .iter_mut()
                    .find(|saved| saved.id == thread.thread_id)
            })
        else {
            return;
        };
        saved.messages = thread
            .messages
            .iter()
            .map(|message| Entry {
                assistant: message.role == "assistant",
                text: message.content.clone(),
            })
            .collect();
        saved.updated = now();
        self.chat_commit(next);
    }

    pub(super) fn clear_durable_chat(&mut self, key: &str, thread: &Conversation) -> bool {
        if thread.thread_id.is_empty() {
            return true;
        }
        let Some(local) = self.ai.chat.history.documents.get(key) else {
            return true;
        };
        let Some(mut next) = self.ai.chat.history.store.clone() else {
            return false;
        };
        let Some(saved) = next
            .documents
            .iter_mut()
            .find(|doc| doc.binding == local.binding)
            .and_then(|doc| {
                doc.threads
                    .iter_mut()
                    .find(|saved| saved.id == thread.thread_id)
            })
        else {
            self.ai.chat.history.error = Some("History thread is no longer available".into());
            return false;
        };
        saved.messages.clear();
        saved.updated = now();
        self.chat_commit(next)
    }

    fn enable_chat_history(&mut self, key: &str) {
        if self.ai.chat.is_busy() {
            return;
        }
        let Some(doc) = self.doc().filter(|doc| !doc.is_dirty()) else {
            return;
        };
        let Some(binding) = doc.chat_identity.clone() else {
            return;
        };
        if doc.changed_on_disk()
            || self
                .ai
                .chat
                .history
                .documents
                .values()
                .any(|doc| doc.binding == binding)
        {
            return;
        }
        let Some(mut next) = self.ai.chat.history.store.clone() else {
            return;
        };
        let history =
            if let Some(history) = next.documents.iter().find(|doc| doc.binding == binding) {
                history.clone()
            } else {
                let history = History {
                    binding,
                    threads: Vec::new(),
                };
                next.documents.push(history.clone());
                history
            };
        if self.chat_commit(next) {
            self.attach_chat_history(key, history);
        }
    }

    fn disable_chat_history(&mut self, key: &str) {
        if self.ai.chat.is_busy() {
            return;
        }
        let Some(local) = self.ai.chat.history.documents.get(key) else {
            return;
        };
        let Some(mut next) = self.ai.chat.history.store.clone() else {
            return;
        };
        next.documents.retain(|doc| doc.binding != local.binding);
        if self.chat_commit(next) {
            // Disable forgets disk history but keeps the current transcript usable.
            // Preserve all other transcripts in session memory, including the old
            // session-only conversation, until this document is closed.
            let local = self.ai.chat.history.documents.remove(key).unwrap();
            self.ai.chat.history.detached.insert(key.into(), local);
        }
    }

    fn edit_chat_thread(&mut self, key: &str, rename: bool) {
        if self.ai.chat.is_busy() {
            return;
        }
        let name = self.ai.chat.history.name.trim().to_owned();
        if !disk::validate_name(&name) || self.ai.chat.history.name.chars().any(char::is_control) {
            self.ai.chat.history.error =
                Some(t("Enter a unique thread name (1–80 bytes, no control characters).").into());
            return;
        }
        let Some(local) = self.ai.chat.history.documents.get(key) else {
            return;
        };
        let Some(mut next) = self.ai.chat.history.store.clone() else {
            return;
        };
        let Some(history) = next
            .documents
            .iter_mut()
            .find(|doc| doc.binding == local.binding)
        else {
            return;
        };
        let active = self
            .ai
            .chat
            .threads
            .get(key)
            .map(|thread| thread.thread_id.clone())
            .unwrap_or_default();
        if history.threads.iter().any(|thread| {
            thread.name.to_lowercase() == name.to_lowercase() && (!rename || thread.id != active)
        }) {
            self.ai.chat.history.error =
                Some(t("Enter a unique thread name (1–80 bytes, no control characters).").into());
            return;
        }
        let id = if rename {
            let Some(thread) = history
                .threads
                .iter_mut()
                .find(|thread| thread.id == active)
            else {
                return;
            };
            thread.name = name;
            active.clone()
        } else {
            let id = uuid::Uuid::new_v4().to_string();
            history.threads.push(Thread {
                id: id.clone(),
                name,
                updated: now(),
                messages: Vec::new(),
            });
            id
        };
        if self.chat_commit(next) {
            if !rename {
                let previous = self.ai.chat.threads.insert(
                    key.into(),
                    Conversation {
                        thread_id: id,
                        ..Default::default()
                    },
                );
                if let Some(previous) = previous {
                    let local = self.ai.chat.history.documents.get_mut(key).unwrap();
                    if previous.thread_id.is_empty() {
                        local.session = previous;
                    } else {
                        local.parked.insert(active, previous);
                    }
                }
            }
            self.ai.chat.history.name.clear();
        }
    }

    fn select_chat_thread(&mut self, key: &str, id: &str) {
        if self.ai.chat.is_busy() {
            return;
        }
        let local = self.ai.chat.history.documents.get_mut(key).unwrap();
        let saved = self
            .ai
            .chat
            .history
            .store
            .as_ref()
            .unwrap()
            .documents
            .iter()
            .find(|doc| doc.binding == local.binding)
            .unwrap()
            .threads
            .iter()
            .find(|thread| thread.id == id)
            .unwrap();
        let selected = local
            .parked
            .remove(id)
            .unwrap_or_else(|| conversation(saved));
        if let Some(previous) = self.ai.chat.threads.insert(key.into(), selected) {
            if previous.thread_id.is_empty() {
                local.session = previous;
            } else {
                local.parked.insert(previous.thread_id.clone(), previous);
            }
        }
    }

    fn select_session_chat(&mut self, key: &str) {
        if self.ai.chat.is_busy() {
            return;
        }
        let Some(local) = self.ai.chat.history.documents.get_mut(key) else {
            return;
        };
        let session = std::mem::take(&mut local.session);
        if let Some(previous) = self.ai.chat.threads.insert(key.into(), session) {
            if previous.thread_id.is_empty() {
                local.session = previous;
            } else {
                local.parked.insert(previous.thread_id.clone(), previous);
            }
        }
    }

    fn delete_chat_thread(&mut self, key: &str) {
        if self.ai.chat.is_busy() {
            return;
        }
        let Some(thread) = self.ai.chat.threads.get(key) else {
            return;
        };
        let id = thread.thread_id.clone();
        let Some(local) = self.ai.chat.history.documents.get(key) else {
            return;
        };
        let Some(mut next) = self.ai.chat.history.store.clone() else {
            return;
        };
        let Some(history) = next
            .documents
            .iter_mut()
            .find(|doc| doc.binding == local.binding)
        else {
            return;
        };
        history.threads.retain(|thread| thread.id != id);
        let replacement = history.threads.first().map(|thread| thread.id.clone());
        if self.chat_commit(next) {
            self.ai.chat.threads.remove(key);
            self.ai
                .chat
                .history
                .documents
                .get_mut(key)
                .unwrap()
                .parked
                .remove(&id);
            if let Some(id) = replacement {
                self.select_chat_thread(key, &id);
            } else {
                self.select_session_chat(key);
            }
        }
    }

    pub(super) fn chat_history_controls(&mut self, ui: &mut Ui, key: &str) {
        self.load_chat_history(key);
        let durable = self.ai.chat.history.documents.contains_key(key);
        widgets::caption(
            ui,
            t(
                if durable
                    && self
                        .ai
                        .chat
                        .threads
                        .get(key)
                        .is_some_and(|thread| !thread.thread_id.is_empty())
                {
                    "Local chat history is enabled."
                } else {
                    "Chat history is session-only."
                },
            ),
        );
        if self.ai.chat.history.error.is_some() {
            ui.label(
                RichText::new(t("History operation failed; transcripts kept in memory."))
                    .color(ui.visuals().error_fg_color),
            );
        }
        let allowed = widgets::touch_activation_allowed(ui.ctx());
        egui::CollapsingHeader::new(t("Local chat history")).id_salt(("chat_history_controls", key)).show(ui, |ui| {
            widgets::caption(ui, t("Prompts and replies may contain sensitive data. Local history is not encrypted. Never included in maps or exports."));
            if let Some(error) = &self.ai.chat.history.error {
                ui.label(RichText::new(t("History operation failed; transcripts kept in memory.")).color(ui.visuals().error_fg_color))
                    .on_hover_text(error);
                if ui.button(t("Retry")).clicked() && allowed {
                    if self.ai.chat.history.store.is_none() {
                        self.ai.chat.history.loaded = false;
                        self.ai.chat.history.visited.remove(key);
                    } else {
                        self.ai.chat.history.blocked_binding.remove(key);
                        self.sync_chat_bindings();
                        self.persist_chat_thread(key);
                    }
                }
            }
            let ready = !self.ai.chat.is_busy() && !self.ai.chat.history.composing && self.ai.chat.history.store.is_some();
            if let Some(local) = self.ai.chat.history.detached.get(key) {
                let ids: Vec<_> = local.parked.keys().cloned().collect();
                let mut selected = None;
                ui.add_enabled_ui(!self.ai.chat.is_busy(), |ui| {
                    ui.collapsing(t("Session-only conversation"), |ui| {
                        if ui.button(t("Restore")).clicked() && allowed { selected = Some(None); }
                        for (index, id) in ids.iter().enumerate() {
                            if ui.button(format!("{} {}", t("Conversation"), index + 1)).clicked() && allowed { selected = Some(Some(id.clone())); }
                        }
                    });
                });
                if let Some(selected) = selected {
                    let local = self.ai.chat.history.detached.get_mut(key).unwrap();
                    let next = match selected {
                        Some(id) => local.parked.remove(&id).unwrap(),
                        None => std::mem::take(&mut local.session),
                    };
                    if let Some(previous) = self.ai.chat.threads.insert(key.into(), next) {
                        if previous.thread_id.is_empty() { local.session = previous; }
                        else { local.parked.insert(previous.thread_id.clone(), previous); }
                    }

                }
            }
                ui.add_enabled_ui(ready, |ui| {
                if !durable {
                    let saved = self.doc().is_some_and(|doc| !doc.is_dirty() && doc.chat_identity.is_some());
                    widgets::caption(ui, t("Save the document first. Save As, moved files and recovery do not inherit history."));
                    if self.ai.chat.history.detached.contains_key(key) {
                        widgets::caption(ui, t("Close and reopen this document before enabling history again. Session transcripts remain available here."));
                    }
                    if ui.add_enabled(saved && !self.ai.chat.history.detached.contains_key(key), egui::Button::new(t("Enable local history"))).clicked() && allowed {
                        self.ai.chat.history.confirm = Some(Action::Enable);
                    }
                } else {
                    let binding = &self.ai.chat.history.documents[key].binding;
                    let threads = self.ai.chat.history.store.as_ref().unwrap().documents.iter()
                        .find(|doc| &doc.binding == binding).map(|doc| doc.threads.clone()).unwrap_or_default();
                    let active = self.ai.chat.threads.get(key).map(|thread| thread.thread_id.clone()).unwrap_or_default();
                    let mut selected = active.clone();
                    egui::ComboBox::from_id_salt(("chat_thread", key))
                        .width(ui.available_width().min(220.0))
                        .wrap_mode(egui::TextWrapMode::Truncate)
                        .selected_text(threads.iter().find(|thread| thread.id == active).map(|thread| thread.name.as_str()).unwrap_or(t("New thread")))
                        .show_ui(ui, |ui| {
                            for thread in &threads { ui.selectable_value(&mut selected, thread.id.clone(), &thread.name); }
                        });
                    if selected != active && allowed { self.select_chat_thread(key, &selected); }
                    if ui.add_enabled(!active.is_empty(), egui::Button::new(t("Session-only conversation"))).clicked() && allowed {
                        self.select_session_chat(key);
                    }
                    ui.add(egui::TextEdit::singleline(&mut self.ai.chat.history.name)
                        .id(egui::Id::unique("chat_thread_name"))
                        .hint_text(t("Thread name")).char_limit(80).desired_width(ui.available_width()));
                    ui.horizontal_wrapped(|ui| {
                        if ui.add_enabled(threads.len() < disk::MAX_THREADS, egui::Button::new(t("New thread"))).clicked() && allowed { self.edit_chat_thread(key, false); }
                        if ui.add_enabled(!active.is_empty(), egui::Button::new(t("Rename"))).clicked() && allowed { self.edit_chat_thread(key, true); }
                        if ui.add_enabled(!active.is_empty(), egui::Button::new(t("Delete thread"))).clicked() && allowed { self.ai.chat.history.confirm = Some(Action::Delete); }
                    });
                    if ui.button(t("Disable and forget this file")).clicked() && allowed { self.ai.chat.history.confirm = Some(Action::Disable); }
                }
                ui.horizontal_wrapped(|ui| {
                    for days in [7, 30, 90] {
                        if ui.button(format!("{days} {}", t("days"))).clicked() && allowed {
                            self.ai.chat.history.retention = Some(days);
                            self.ai.chat.history.confirm = Some(Action::Retain);
                        }
                    }
                });
                widgets::caption(ui, t("Retention is applied only on confirmation; it deletes older threads across all files."));
                if ui.button(t("Delete all local chat history")).clicked() && allowed { self.ai.chat.history.confirm = Some(Action::DeleteAll); }
            });
        });
        if let Some(action) = self.ai.chat.history.confirm {
            let owner = (
                key.to_owned(),
                self.ai
                    .chat
                    .threads
                    .get(key)
                    .map(|thread| thread.thread_id.clone())
                    .unwrap_or_default(),
                self.current_viewport,
            );
            if self
                .ai
                .chat
                .history
                .owner
                .as_ref()
                .is_some_and(|previous| previous != &owner)
            {
                self.ai.chat.history.confirm = None;
                self.ai.chat.history.owner = None;
                return;
            }
            self.ai.chat.history.owner = Some(owner);
            let mut confirm = false;
            let mut cancel = false;
            let labels = crate::i18n::dialog_labels();
            let response = thinoda_ui::Dialog::new(egui::Id::unique("local_chat_history_confirm"), t("Local chat history"))
                .labels(&labels).modal(true).show(ui.ctx(), |ui| {
                    ui.label(t("Confirm local history change?"));
                    ui.label(t(match action {
                        Action::Enable => "Enable local history",
                        Action::Disable => "Disable and forget this file",
                        Action::Delete => "Delete thread",
                        Action::Retain => "Retention is applied only on confirmation; it deletes older threads across all files.",
                        Action::DeleteAll => "Delete all local chat history",
                    }));
                    if matches!(action, Action::Enable) {
                        ui.label(t("Prompts and replies may contain sensitive data. Local history is not encrypted. Never included in maps or exports."));
                        ui.label(t("Existing session messages are not copied. Create a named thread to save future completed exchanges."));
                    }
                    if matches!(action, Action::Retain) { ui.label(format!("{} {}", self.ai.chat.history.retention.unwrap_or(30), t("days"))); }
                    ui.horizontal_wrapped(|ui| {
                        confirm = ui.add_enabled(!self.ai.chat.is_busy() && !self.ai.chat.history.composing, egui::Button::new(t("Confirm"))).clicked() && allowed;
                        cancel = ui.button(t("Cancel")).clicked() && allowed;
                    });
                });
            if confirm {
                match action {
                    Action::Enable => self.enable_chat_history(key),
                    Action::Disable => self.disable_chat_history(key),
                    Action::Delete => self.delete_chat_thread(key),
                    Action::Retain => {
                        if let Some(mut next) = self.ai.chat.history.store.clone() {
                            next.retention_days = self.ai.chat.history.retention.unwrap_or(30);
                            next.expire(now());
                            if self.chat_commit(next) {
                                self.reconcile_chat_retention();
                            }
                        }
                    }
                    Action::DeleteAll => {
                        if let Some(mut next) = self.ai.chat.history.store.clone() {
                            next.documents.clear();
                            if self.chat_commit(next) {
                                let documents = std::mem::take(&mut self.ai.chat.history.documents);
                                self.ai.chat.history.detached.extend(documents);
                            }
                        }
                    }
                }
                self.ai.chat.history.confirm = None;
                self.ai.chat.history.owner = None;
            }
            if cancel || (response.close_requested && allowed) {
                self.ai.chat.history.confirm = None;
                self.ai.chat.history.owner = None;
            }
        }
    }

    fn reconcile_chat_retention(&mut self) {
        // Expiration removes disk threads, not unsaved in-memory transcripts.
        // Detach affected documents rather than silently losing their drafts.
        let Some(store) = &self.ai.chat.history.store else {
            return;
        };
        let keys: Vec<_> = self
            .ai
            .chat
            .history
            .documents
            .iter()
            .filter_map(|(key, local)| {
                let active = self.ai.chat.threads.get(key);
                let persisted = store
                    .documents
                    .iter()
                    .find(|doc| doc.binding == local.binding);
                let removed = active.is_some_and(|active| {
                    !active.thread_id.is_empty()
                        && persisted.is_none_or(|doc| {
                            !doc.threads
                                .iter()
                                .any(|thread| thread.id == active.thread_id)
                        })
                });
                (removed
                    || local.parked.keys().any(|id| {
                        persisted
                            .is_none_or(|doc| !doc.threads.iter().any(|thread| &thread.id == id))
                    }))
                .then(|| key.clone())
            })
            .collect();
        for key in keys {
            let local = self.ai.chat.history.documents.remove(&key).unwrap();
            self.ai.chat.history.detached.insert(key, local);
        }
    }
}
