//! Preserve browser pen pressure and genuinely optional touch force.

use std::{cell::RefCell, rc::Rc};

use eframe::wasm_bindgen::{JsCast, closure::Closure};
use egui::{Context, PointerButton, Pos2, TouchDeviceId, TouchId, TouchPhase};
use thinoda_app::ThinodaApp;
use thinoda_canvas::pen_input::{
    self, MAX_POINTER_RECORDS, PenInputMatcher, PenSample, PointerPhase, PointerRecord,
};
use thinoda_canvas::touch_input::{self, MAX_TOUCH_RECORDS, TouchForceRecord, TouchPointerRecord};
use web_sys::{
    Document, Event, EventTarget, HtmlCanvasElement, MouseEvent, PointerEvent, TouchEvent,
};

pub(crate) struct WebApp {
    app: ThinodaApp,
    pen: Option<PenBridge>,
    touch: Option<TouchForceBridge>,
}

impl WebApp {
    pub fn new(app: ThinodaApp, canvas: &HtmlCanvasElement, ctx: &Context) -> Self {
        let pen = match PenBridge::install(canvas, ctx) {
            Ok(bridge) => Some(bridge),
            Err(error) => {
                log::warn!("Browser pen pressure capture is unavailable: {error}");
                None
            }
        };
        let touch = match TouchForceBridge::install(canvas, ctx) {
            Ok(bridge) => Some(bridge),
            Err(error) => {
                log::warn!("Browser touch-force capture is unavailable: {error}");
                None
            }
        };
        Self { app, pen, touch }
    }
}

impl eframe::App for WebApp {
    fn raw_input_hook(&mut self, ctx: &Context, input: &mut egui::RawInput) {
        eframe::App::raw_input_hook(&mut self.app, ctx, input);
        if let Some(touch) = &mut self.touch {
            touch.prepare_frame(ctx, input);
        }
        if let Some(pen) = &mut self.pen {
            pen.prepare_frame(ctx, input);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        eframe::App::ui(&mut self.app, ui, frame);
    }

    fn on_exit(&mut self) {
        eframe::App::on_exit(&mut self.app);
    }
}

struct Listener {
    target: EventTarget,
    name: &'static str,
    callback: Option<Closure<dyn FnMut(Event)>>,
}

impl Listener {
    fn new(
        target: &EventTarget,
        name: &'static str,
        callback: impl FnMut(Event) + 'static,
    ) -> Result<Self, String> {
        let callback = Closure::<dyn FnMut(Event)>::new(callback);
        target
            .add_event_listener_with_callback_and_bool(
                name,
                callback.as_ref().unchecked_ref(),
                true,
            )
            .map_err(|error| format!("could not listen for {name}: {error:?}"))?;
        Ok(Self {
            target: target.clone(),
            name,
            callback: Some(callback),
        })
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        let Some(callback) = self.callback.take() else {
            return;
        };
        if let Err(error) = self.target.remove_event_listener_with_callback_and_bool(
            self.name,
            callback.as_ref().unchecked_ref(),
            true,
        ) {
            log::warn!(
                "Could not remove browser input listener {}: {error:?}",
                self.name
            );
            callback.forget();
        }
    }
}

#[derive(Default)]
struct PendingTouchForces {
    records: Vec<TouchForceRecord>,
    overflowed: bool,
    pointers: Vec<TouchPointerRecord>,
    pointer_overflowed: bool,
}

struct TouchForceBridge {
    pending: Rc<RefCell<PendingTouchForces>>,
    _listeners: Vec<Listener>,
}

impl TouchForceBridge {
    fn install(canvas: &HtmlCanvasElement, ctx: &Context) -> Result<Self, String> {
        let document = web_sys::window()
            .and_then(|window| window.document())
            .ok_or("browser document is unavailable")?;
        let target: EventTarget = document.clone().into();
        let pending = Rc::new(RefCell::new(PendingTouchForces::default()));
        let mut listeners = Vec::new();
        for (name, phase) in [
            ("touchstart", TouchPhase::Start),
            ("touchmove", TouchPhase::Move),
            ("touchend", TouchPhase::End),
            ("touchcancel", TouchPhase::Cancel),
        ] {
            let pending = pending.clone();
            let canvas = canvas.clone();
            let ctx = ctx.clone();
            listeners.push(Listener::new(&target, name, move |event| {
                if event.target().as_ref() != Some(canvas.unchecked_ref::<EventTarget>()) {
                    return;
                }
                let Some(event) = event.dyn_ref::<TouchEvent>() else {
                    return;
                };
                let touches = event.changed_touches();
                let mut pending = pending.borrow_mut();
                for index in 0..touches.length() {
                    let Some(touch) = touches.item(index) else {
                        continue;
                    };
                    if pending.overflowed {
                        break;
                    }
                    if pending.records.len() >= MAX_TOUCH_RECORDS {
                        pending.records.clear();
                        pending.overflowed = true;
                        break;
                    }
                    let force_missing = match js_sys::Reflect::get(touch.as_ref(), &"force".into())
                    {
                        Ok(value) => value.is_undefined(),
                        Err(error) => {
                            log::warn!("Could not inspect browser touch force: {error:?}");
                            false
                        }
                    };
                    pending.records.push(TouchForceRecord {
                        device: TouchDeviceId(0),
                        id: TouchId::from(touch.identifier()),
                        phase,
                        position: position_from_client(
                            &canvas,
                            &ctx,
                            touch.client_x(),
                            touch.client_y(),
                        ),
                        force_missing,
                    });
                }
            })?);
        }
        for name in ["pointerdown", "pointerup"] {
            let pending = pending.clone();
            let document = document.clone();
            let canvas = canvas.clone();
            let ctx = ctx.clone();
            listeners.push(Listener::new(&target, name, move |event| {
                let Some(event) = event.dyn_ref::<PointerEvent>() else {
                    return;
                };
                let Some(button) = pointer_button(event.button()) else {
                    return;
                };
                if (name == "pointerdown"
                    && event.target().as_ref() != Some(canvas.unchecked_ref::<EventTarget>()))
                    || (name == "pointerup" && !interested(&document, &canvas, &ctx, event))
                {
                    return;
                }
                let mut pending = pending.borrow_mut();
                if pending.pointer_overflowed {
                    return;
                }
                if pending.pointers.len() >= MAX_TOUCH_RECORDS {
                    pending.pointers.clear();
                    pending.pointer_overflowed = true;
                    return;
                }
                pending.pointers.push(TouchPointerRecord {
                    phase: if name == "pointerdown" {
                        PointerPhase::Press(button)
                    } else {
                        PointerPhase::Release(button)
                    },
                    position: position(&canvas, &ctx, event),
                    is_touch: event.pointer_type() == "touch",
                });
            })?);
        }
        Ok(Self {
            pending,
            _listeners: listeners,
        })
    }

    fn prepare_frame(&mut self, ctx: &Context, input: &mut egui::RawInput) {
        let (records, overflowed, pointers, pointer_overflowed) = {
            let mut pending = self.pending.borrow_mut();
            (
                std::mem::take(&mut pending.records),
                std::mem::take(&mut pending.overflowed),
                std::mem::take(&mut pending.pointers),
                std::mem::take(&mut pending.pointer_overflowed),
            )
        };
        if let Err(issue) =
            touch_input::normalize_missing_force(&mut input.events, &records, overflowed)
        {
            log::warn!("Browser touch force could not be correlated safely: {issue:?}");
        }
        if let Some(issue) = touch_input::publish_pointer_frame(
            ctx,
            input.viewport_id,
            &input.events,
            &pointers,
            pointer_overflowed,
            &records,
            overflowed,
        ) {
            log::warn!("Browser touch pointer could not be correlated safely: {issue:?}");
        }
    }
}

struct PendingMove {
    sample: PenSample,
    x: i32,
    y: i32,
    buttons: u16,
    timestamp: f64,
    target: Option<EventTarget>,
}

#[derive(Default)]
struct PendingInput {
    records: Vec<PointerRecord>,
    active_pen: Option<i32>,
    pending_move: Option<PendingMove>,
    canceled: Option<i32>,
    overflowed: bool,
}

impl PendingInput {
    fn push(&mut self, record: PointerRecord) {
        if self.overflowed {
            return;
        }
        if self.records.len() >= MAX_POINTER_RECORDS {
            self.records.clear();
            self.overflowed = true;
        } else {
            self.records.push(record);
        }
    }

    fn cancel_active(&mut self) -> bool {
        self.pending_move = None;
        if let Some(id) = self.active_pen.take() {
            self.canceled = Some(id);
            true
        } else {
            false
        }
    }
}

struct PenBridge {
    pending: Rc<RefCell<PendingInput>>,
    matcher: PenInputMatcher,
    _listeners: Vec<Listener>,
}

impl PenBridge {
    fn install(canvas: &HtmlCanvasElement, ctx: &Context) -> Result<Self, String> {
        let window = web_sys::window().ok_or("browser window is unavailable")?;
        let document = window.document().ok_or("browser document is unavailable")?;
        let target: EventTarget = document.clone().into();
        let pending = Rc::new(RefCell::new(PendingInput::default()));
        let mut listeners = Vec::new();

        for name in [
            "pointerdown",
            "pointerup",
            "pointermove",
            "pointercancel",
            "mousemove",
        ] {
            let pending = pending.clone();
            let canvas = canvas.clone();
            let document = document.clone();
            let ctx = ctx.clone();
            listeners.push(Listener::new(&target, name, move |event| {
                if name == "mousemove" {
                    if let Some(event) = event.dyn_ref::<MouseEvent>() {
                        mouse_move(&pending, &document, &canvas, &ctx, event);
                    }
                } else if let Some(event) = event.dyn_ref::<PointerEvent>() {
                    pointer_event(&pending, &document, &canvas, &ctx, name, event);
                }
            })?);
        }

        let blur_pending = pending.clone();
        let blur_ctx = ctx.clone();
        listeners.push(Listener::new(&window.into(), "blur", move |_| {
            if blur_pending.borrow_mut().cancel_active() {
                blur_ctx.request_repaint();
            }
        })?);

        Ok(Self {
            pending,
            matcher: PenInputMatcher::default(),
            _listeners: listeners,
        })
    }

    fn prepare_frame(&mut self, ctx: &Context, input: &egui::RawInput) {
        let (records, canceled, overflowed) = {
            let mut pending = self.pending.borrow_mut();
            pending.pending_move = None;
            (
                std::mem::take(&mut pending.records),
                pending.canceled.take(),
                std::mem::take(&mut pending.overflowed),
            )
        };
        let touch = touch_input::peek_pointer_frame(ctx, input.viewport_id, &input.events);
        let frame = self.matcher.match_frame_with_touch(
            &input.events,
            &records,
            canceled,
            overflowed,
            &touch,
        );
        if let Some(issue) = frame.issue {
            log::warn!("Browser pen input could not be correlated safely: {issue:?}");
        }
        pen_input::publish(ctx, input.viewport_id, &input.events, frame);
    }
}

fn pen_sample(event: &PointerEvent) -> Option<PenSample> {
    (event.is_trusted() && event.is_primary() && event.pointer_type() == "pen").then(|| PenSample {
        id: event.pointer_id(),
        pressure: event.pressure(),
    })
}

fn pointer_event(
    pending: &Rc<RefCell<PendingInput>>,
    document: &Document,
    canvas: &HtmlCanvasElement,
    ctx: &Context,
    name: &str,
    event: &PointerEvent,
) {
    if name == "pointercancel" {
        let canceled = {
            let mut pending = pending.borrow_mut();
            pending.active_pen == Some(event.pointer_id()) && pending.cancel_active()
        };
        if canceled {
            ctx.request_repaint();
        }
        return;
    }

    if name == "pointermove" {
        let mut pending = pending.borrow_mut();
        pending.pending_move = if pending.active_pen.is_some() && event.buttons() & 1 != 0 {
            pen_sample(event).map(|sample| PendingMove {
                sample,
                x: event.client_x(),
                y: event.client_y(),
                buttons: event.buttons(),
                timestamp: event.time_stamp(),
                target: event.target(),
            })
        } else {
            None
        };
        return;
    }

    let Some(button) = pointer_button(event.button()) else {
        return;
    };
    let sample = pen_sample(event);
    let mut pending = pending.borrow_mut();
    pending.pending_move = None;
    if name == "pointerdown" {
        if event.target().as_ref() != Some(canvas.unchecked_ref::<EventTarget>()) {
            return;
        }
        if button == PointerButton::Primary
            && let Some(sample) = sample
            && pending.active_pen.is_none()
        {
            pending.active_pen = Some(sample.id);
        }
        if pending.active_pen.is_none() {
            return;
        }
        pending.push(PointerRecord {
            phase: PointerPhase::Press(button),
            position: position(canvas, ctx, event),
            pen: sample,
        });
    } else {
        if pending.active_pen.is_none() {
            return;
        }
        if interested(document, canvas, ctx, event) {
            pending.push(PointerRecord {
                phase: PointerPhase::Release(button),
                position: position(canvas, ctx, event),
                pen: sample,
            });
        } else if pending.active_pen == Some(event.pointer_id()) {
            pending.canceled = pending.active_pen;
            ctx.request_repaint();
        }
        if pending.active_pen == Some(event.pointer_id()) {
            pending.active_pen = None;
        }
    }
}

fn mouse_move(
    pending: &Rc<RefCell<PendingInput>>,
    document: &Document,
    canvas: &HtmlCanvasElement,
    ctx: &Context,
    event: &MouseEvent,
) {
    let mut pending = pending.borrow_mut();
    let candidate = pending.pending_move.take();
    if pending.active_pen.is_none() || !interested(document, canvas, ctx, event) {
        return;
    }
    // Compatibility mouse moves carry no pointer ID. Only an immediately
    // preceding trusted pen move with the same browser event identity is usable.
    let sample = candidate
        .filter(|candidate| {
            event.is_trusted()
                && candidate.x == event.client_x()
                && candidate.y == event.client_y()
                && candidate.buttons == event.buttons()
                && candidate.timestamp == event.time_stamp()
                && candidate.target == event.target()
        })
        .map(|candidate| candidate.sample);
    pending.push(PointerRecord {
        phase: PointerPhase::Move,
        position: position(canvas, ctx, event),
        pen: sample,
    });
}

fn pointer_button(button: i16) -> Option<PointerButton> {
    match button {
        0 => Some(PointerButton::Primary),
        1 => Some(PointerButton::Middle),
        2 => Some(PointerButton::Secondary),
        3 => Some(PointerButton::Extra1),
        4 => Some(PointerButton::Extra2),
        _ => None,
    }
}

fn interested(
    document: &Document,
    canvas: &HtmlCanvasElement,
    ctx: &Context,
    event: &MouseEvent,
) -> bool {
    document
        .element_from_point(event.client_x() as f32, event.client_y() as f32)
        .is_some_and(|element| element == *canvas.unchecked_ref::<web_sys::Element>())
        || ctx.input(|input| input.pointer.any_down() || input.any_touches())
}

fn position(canvas: &HtmlCanvasElement, ctx: &Context, event: &MouseEvent) -> Pos2 {
    position_from_client(canvas, ctx, event.client_x(), event.client_y())
}

fn position_from_client(canvas: &HtmlCanvasElement, ctx: &Context, x: i32, y: i32) -> Pos2 {
    let rect = canvas.get_bounding_client_rect();
    let style =
        web_sys::window().and_then(|window| window.get_computed_style(canvas).ok().flatten());
    let padding = |name| {
        style
            .as_ref()
            .and_then(|style| style.get_property_value(name).ok())
            .and_then(|value| value.trim_end_matches("px").parse::<f32>().ok())
            .unwrap_or_default()
    };
    egui::pos2(
        (x as f32 - (rect.left() as f32 + padding("padding-left"))) / ctx.zoom_factor(),
        (y as f32 - (rect.top() as f32 + padding("padding-top"))) / ctx.zoom_factor(),
    )
}
