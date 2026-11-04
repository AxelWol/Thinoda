//! An isolated, synthetic feasibility probe, not an accessibility adapter for Thinoda.

#[cfg(any(target_arch = "wasm32", test))]
mod probe {
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };

    use egui::{
        Context, Event, FullOutput, Plugin, RawInput,
        accesskit::{Action, ActionRequest, NodeId, Role, TreeId, TreeUpdate},
    };

    const MAX_PENDING: usize = 8;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Snapshot {
        label: String,
        enabled: bool,
    }

    #[derive(Default)]
    struct State {
        candidate: Option<NodeId>,
        active: Option<NodeId>,
        pending: VecDeque<NodeId>,
        snapshot: Option<Snapshot>,
    }

    impl State {
        fn nominate(&mut self, candidate: Option<NodeId>) {
            if self.candidate != candidate {
                self.active = None;
                self.pending.clear();
            }
            self.candidate = candidate;
        }

        fn observe(&mut self, update: Option<&TreeUpdate>) {
            let selected = update
                .filter(|tree| tree.tree_id == TreeId::ROOT && tree.tree.is_some())
                .and_then(|tree| {
                    tree.nodes
                        .iter()
                        .find(|(id, _)| Some(*id) == self.candidate)
                })
                .and_then(|(id, node)| {
                    let label = node.label()?;
                    if node.role() != Role::Button
                        || node.is_hidden()
                        || !matches!(
                            label,
                            "Synthetic action 0" | "Synthetic action 1" | "Synthetic action 2"
                        )
                        || (!node.is_disabled() && !node.supports_action(Action::Click))
                    {
                        return None;
                    }
                    Some((
                        *id,
                        Snapshot {
                            label: label.to_owned(),
                            enabled: !node.is_disabled(),
                        },
                    ))
                });

            self.active = selected
                .as_ref()
                .and_then(|(id, snapshot)| snapshot.enabled.then_some(*id));
            self.pending.retain(|id| Some(*id) == self.active);
            self.snapshot = selected.map(|(_, snapshot)| snapshot);
        }

        fn queue_click(&mut self) -> bool {
            match self.active {
                Some(id) if self.pending.len() < MAX_PENDING => {
                    self.pending.push_back(id);
                    true
                }
                _ => false,
            }
        }

        // Egui's button response coalesces same-frame click requests. Deliver at
        // most one per frame and revalidate the target against each new tree.
        fn dispatch_one(&mut self, input: &mut RawInput) -> bool {
            while let Some(target_node) = self.pending.pop_front() {
                if Some(target_node) == self.active {
                    input
                        .events
                        .push(Event::AccessKitActionRequest(ActionRequest {
                            action: Action::Click,
                            target_tree: TreeId::ROOT,
                            target_node,
                            data: None,
                        }));
                    break;
                }
            }
            !self.pending.is_empty()
        }
    }

    struct ProbePlugin {
        state: Arc<Mutex<State>>,
        #[cfg(target_arch = "wasm32")]
        published: Option<Option<Snapshot>>,
        #[cfg(target_arch = "wasm32")]
        handler_installed: bool,
    }

    impl ProbePlugin {
        fn new(state: Arc<Mutex<State>>) -> Self {
            Self {
                state,
                #[cfg(target_arch = "wasm32")]
                published: None,
                #[cfg(target_arch = "wasm32")]
                handler_installed: false,
            }
        }
    }

    impl Plugin for ProbePlugin {
        fn debug_name(&self) -> &'static str {
            "isolated-accesskit-probe"
        }

        fn setup(&mut self, ctx: &Context) {
            ctx.enable_accesskit();
            #[cfg(target_arch = "wasm32")]
            {
                self.handler_installed = web::install_click_handler(ctx, self.state.clone());
            }
        }

        fn input_hook(&mut self, ctx: &Context, input: &mut RawInput) {
            #[cfg(target_arch = "wasm32")]
            if !web::handler_is_current() {
                self.state.lock().unwrap().observe(None);
                return;
            }
            let more = self.state.lock().unwrap().dispatch_one(input);
            if more {
                ctx.request_repaint();
            }
        }

        fn output_hook(&mut self, _ctx: &Context, output: &mut FullOutput) {
            let mut state = self.state.lock().unwrap();
            #[cfg(target_arch = "wasm32")]
            let update = (self.handler_installed && web::handler_is_current())
                .then_some(output.platform_output.accesskit_update.as_ref())
                .flatten();
            #[cfg(not(target_arch = "wasm32"))]
            let update = output.platform_output.accesskit_update.as_ref();
            state.observe(update);
            #[cfg(target_arch = "wasm32")]
            {
                let snapshot = state.snapshot.clone();
                drop(state);
                if self.published.as_ref() != Some(&snapshot) {
                    web::publish(snapshot.as_ref());
                    self.published = Some(snapshot);
                }
            }
        }

        #[cfg(target_arch = "wasm32")]
        fn on_exit(&mut self, _ctx: &Context) {
            self.state.lock().unwrap().observe(None);
            web::publish(None);
            web::remove_click_handler();
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub(super) mod web {
        use super::{ProbePlugin, Snapshot, State};
        use std::{cell::RefCell, sync::Arc};

        use eframe::wasm_bindgen::{JsCast, closure::Closure};
        use egui::Context;

        type ClickHandler = (web_sys::Element, Closure<dyn FnMut(web_sys::Event)>);

        thread_local! {
            static CLICK_HANDLER: RefCell<Option<ClickHandler>> = RefCell::new(None);
        }

        fn element(id: &str) -> Option<web_sys::Element> {
            web_sys::window()?.document()?.get_element_by_id(id)
        }

        pub(super) fn install_click_handler(
            ctx: &Context,
            state: Arc<std::sync::Mutex<State>>,
        ) -> bool {
            let Some(button) = element("probe-action") else {
                return false;
            };
            let ctx = ctx.clone();
            let handler = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
                if handler_is_current() && state.lock().unwrap().queue_click() {
                    ctx.request_repaint();
                }
            });
            if button
                .add_event_listener_with_callback("click", handler.as_ref().unchecked_ref())
                .is_ok()
            {
                CLICK_HANDLER.with(|slot| *slot.borrow_mut() = Some((button, handler)));
                true
            } else {
                false
            }
        }

        pub(super) fn handler_is_current() -> bool {
            CLICK_HANDLER.with(|slot| {
                slot.borrow().as_ref().is_some_and(|(button, _)| {
                    button.is_connected()
                        && element("probe-action")
                            .is_some_and(|current| button.is_same_node(Some(&current)))
                })
            })
        }

        pub(super) fn remove_click_handler() {
            CLICK_HANDLER.with(|slot| {
                if let Some((button, handler)) = slot.borrow_mut().take() {
                    let _ = button.remove_event_listener_with_callback(
                        "click",
                        handler.as_ref().unchecked_ref(),
                    );
                }
            });
        }

        pub(super) fn publish(snapshot: Option<&Snapshot>) {
            let registered =
                CLICK_HANDLER.with(|slot| slot.borrow().as_ref().map(|(button, _)| button.clone()));
            let Some(button) = registered.or_else(|| element("probe-action")) else {
                return;
            };
            match snapshot {
                Some(snapshot) => {
                    button.set_text_content(Some(&snapshot.label));
                    if snapshot.enabled {
                        let _ = button.remove_attribute("disabled");
                    } else {
                        let _ = button.set_attribute("disabled", "");
                    }
                    let _ = button.remove_attribute("hidden");
                }
                None => {
                    let _ = button.set_attribute("disabled", "");
                    let _ = button.set_attribute("hidden", "");
                    button.set_text_content(None);
                }
            }
        }

        struct App {
            state: Arc<std::sync::Mutex<State>>,
            count: u32,
            text: String,
        }

        impl eframe::App for App {
            fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
                ui.label("Synthetic text input");
                let text = ui.text_edit_singleline(&mut self.text);
                let button = ui.add_enabled(
                    self.count < 2,
                    egui::Button::new(format!("Synthetic action {}", self.count)),
                );
                self.state
                    .lock()
                    .unwrap()
                    .nominate(Some(button.id.accesskit_id()));
                if button.clicked() {
                    self.count += 1;
                    ui.ctx().request_repaint();
                }

                if let Some(debug) = element("probe-debug") {
                    let center = text.rect.center();
                    let _ = debug.set_attribute("data-input-x", &center.x.to_string());
                    let _ = debug.set_attribute("data-input-y", &center.y.to_string());
                    let _ = debug.set_attribute("data-count", &self.count.to_string());
                    let _ = debug
                        .set_attribute("data-text-length", &self.text.chars().count().to_string());
                    let _ = debug.set_attribute(
                        "data-text-probe",
                        match self.text.as_str() {
                            "" => "empty",
                            "cafe" => "first",
                            "cafe猫" => "continued",
                            _ => "other",
                        },
                    );
                    if text.clicked() {
                        let _ = debug.set_attribute(
                            "data-last-input-click",
                            if text.double_clicked() {
                                "double"
                            } else {
                                "single"
                            },
                        );
                    }
                    let _ = debug.set_attribute(
                        "data-host-focused",
                        if ui.input(|input| input.focused) {
                            "true"
                        } else {
                            "false"
                        },
                    );
                }
            }
        }

        pub(crate) fn start() {
            wasm_bindgen_futures::spawn_local(async {
                let Some(canvas) = element("probe-canvas")
                    .and_then(|element| element.dyn_into::<web_sys::HtmlCanvasElement>().ok())
                else {
                    return;
                };
                let state = Arc::new(std::sync::Mutex::new(State::default()));
                let result = eframe::WebRunner::new()
                    .start(
                        canvas,
                        eframe::WebOptions::default(),
                        Box::new(move |cc| {
                            cc.egui_ctx.add_plugin(ProbePlugin::new(state.clone()));
                            Ok(Box::new(App {
                                state,
                                count: 0,
                                text: String::new(),
                            }))
                        }),
                    )
                    .await;
                if let Err(error) = result
                    && let Some(debug) = element("probe-debug")
                {
                    let _ = debug.set_attribute("data-start-error", &format!("{error:?}"));
                }
            });
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use egui::accesskit::{Node, TreeInfo};

        fn tree(id: NodeId, node: Node) -> TreeUpdate {
            TreeUpdate {
                nodes: vec![(id, node)],
                tree: Some(TreeInfo::new(id)),
                tree_id: TreeId::ROOT,
                focus: id,
            }
        }

        fn button(label: &str) -> Node {
            let mut node = Node::new(Role::Button);
            node.set_label(label);
            node.add_action(Action::Click);
            node
        }

        #[test]
        fn only_the_nominated_synthetic_control_is_published_and_authorized() {
            let id = NodeId(42);
            let mut state = State::default();
            state.nominate(Some(id));
            state.observe(Some(&tree(id, button("Synthetic action 0"))));
            assert_eq!(state.snapshot.as_ref().unwrap().label, "Synthetic action 0");
            assert!(state.queue_click());

            state.observe(Some(&tree(id, button("A private document title"))));
            assert!(state.snapshot.is_none());
            assert!(state.pending.is_empty());
            assert!(!state.queue_click());

            let mut hidden = button("Synthetic action 0");
            hidden.set_hidden();
            state.observe(Some(&tree(id, hidden)));
            assert!(state.snapshot.is_none());

            let mut wrong_role = Node::new(Role::TextInput);
            wrong_role.set_label("Synthetic action 0");
            wrong_role.add_action(Action::Click);
            state.observe(Some(&tree(id, wrong_role)));
            assert!(state.snapshot.is_none());

            let mut no_action = Node::new(Role::Button);
            no_action.set_label("Synthetic action 0");
            state.observe(Some(&tree(id, no_action)));
            assert!(state.snapshot.is_none());

            let mut disabled = button("Synthetic action 2");
            disabled.set_disabled();
            state.observe(Some(&tree(id, disabled)));
            assert!(!state.snapshot.as_ref().unwrap().enabled);
            assert!(!state.queue_click());

            state.observe(Some(&tree(NodeId(43), button("Synthetic action 0"))));
            assert!(state.snapshot.is_none());
            let mut incomplete = tree(id, button("Synthetic action 0"));
            incomplete.tree = None;
            state.observe(Some(&incomplete));
            assert!(state.snapshot.is_none());
            state.observe(None);
            assert!(state.snapshot.is_none());
        }

        #[test]
        fn egui_output_and_queued_accesskit_clicks_round_trip() {
            let state = Arc::new(Mutex::new(State::default()));
            let ctx = Context::default();
            ctx.add_plugin(ProbePlugin::new(state.clone()));
            let mut count = 0;
            let frame = |count: &mut u32| {
                let output = ctx.run_ui(RawInput::default(), |ui| {
                    let response = ui.add_enabled(
                        *count < 2,
                        egui::Button::new(format!("Synthetic action {count}")),
                    );
                    state
                        .lock()
                        .unwrap()
                        .nominate(Some(response.id.accesskit_id()));
                    if response.clicked() {
                        *count += 1;
                    }
                });
                output.drop_without_applying_deltas();
            };

            frame(&mut count);
            assert_eq!(
                state.lock().unwrap().snapshot.as_ref().unwrap().label,
                "Synthetic action 0"
            );
            assert!(state.lock().unwrap().queue_click());
            assert!(state.lock().unwrap().queue_click());
            frame(&mut count);
            assert_eq!(count, 1);
            frame(&mut count);
            assert_eq!(count, 2);
            frame(&mut count);
            let mut state = state.lock().unwrap();
            assert_eq!(state.snapshot.as_ref().unwrap().label, "Synthetic action 2");
            assert!(!state.snapshot.as_ref().unwrap().enabled);
            assert!(!state.queue_click());
        }

        #[test]
        fn replacing_a_target_revokes_pending_actions() {
            let id = NodeId(1);
            let mut state = State::default();
            state.nominate(Some(id));
            state.observe(Some(&tree(id, button("Synthetic action 0"))));
            for _ in 0..MAX_PENDING {
                assert!(state.queue_click());
            }
            assert!(!state.queue_click());
            state.nominate(Some(NodeId(2)));
            assert!(state.pending.is_empty());
            assert!(!state.queue_click());
            let mut input = RawInput::default();
            assert!(!state.dispatch_one(&mut input));
            assert!(input.events.is_empty());
        }

        #[test]
        fn a_dom_action_does_not_break_eguis_pointer_double_click_sequence() {
            fn frame(
                ctx: &Context,
                state: &Arc<Mutex<State>>,
                input: RawInput,
                text: &mut String,
                count: &mut u32,
            ) -> (egui::Pos2, bool) {
                let mut result = (egui::Pos2::ZERO, false);
                let output = ctx.run_ui(input, |ui| {
                    let response = ui.text_edit_singleline(text);
                    result = (response.rect.center(), response.double_clicked());
                    let button = ui.button("Synthetic action 0");
                    state
                        .lock()
                        .unwrap()
                        .nominate(Some(button.id.accesskit_id()));
                    if button.clicked() {
                        *count += 1;
                    }
                });
                output.drop_without_applying_deltas();
                result
            }

            fn pointer_click(time: f64, pos: egui::Pos2) -> RawInput {
                RawInput {
                    time: Some(time),
                    events: vec![
                        Event::PointerMoved(pos),
                        Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: egui::Modifiers::NONE,
                        },
                        Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed: false,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                }
            }

            let state = Arc::new(Mutex::new(State::default()));
            let ctx = Context::default();
            ctx.add_plugin(ProbePlugin::new(state.clone()));
            let mut text = "cafe".to_owned();
            let mut count = 0;
            let (pos, _) = frame(&ctx, &state, RawInput::default(), &mut text, &mut count);
            assert!(!frame(&ctx, &state, pointer_click(1.0, pos), &mut text, &mut count).1);
            assert!(state.lock().unwrap().queue_click());
            let unfocused = RawInput {
                time: Some(1.1),
                focused: false,
                events: vec![Event::WindowFocused(false)],
                ..Default::default()
            };
            frame(&ctx, &state, unfocused, &mut text, &mut count);
            assert_eq!(count, 1);
            let mut refocused = pointer_click(1.2, pos);
            refocused.events.insert(0, Event::WindowFocused(true));
            assert!(frame(&ctx, &state, refocused, &mut text, &mut count).1);
            let insert = RawInput {
                time: Some(1.3),
                events: vec![Event::Text("猫".to_owned())],
                ..Default::default()
            };
            frame(&ctx, &state, insert, &mut text, &mut count);
            assert_eq!(text, "猫");
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {
    probe::web::start();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("This isolated accessibility probe runs in a browser.");
}
