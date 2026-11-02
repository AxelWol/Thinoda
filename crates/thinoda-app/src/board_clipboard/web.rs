//! Browser clipboard transport. Menu operations use the asynchronous
//! Clipboard API and retain their document guard until completion.

use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use js_sys::{Array, Function, Object, Promise, Reflect, Uint8Array};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::{JsFuture, spawn_local};

use super::{Completion, DocumentGuard, MAX_HTML_BYTES, PreparedCopy, Snapshot};

thread_local! {
    // Kept only to make the current DOM event available while eframe runs its
    // synchronous copy/cut logic. event_phase() prevents reuse after dispatch.
    static ACTIVE_COPY_CUT: RefCell<Option<web_sys::ClipboardEvent>> = const { RefCell::new(None) };
    static CAPTURE_INSTALLED: RefCell<bool> = const { RefCell::new(false) };
}

pub(super) fn install_event_capture() -> Result<(), String> {
    if CAPTURE_INSTALLED.with(|installed| *installed.borrow()) {
        return Ok(());
    }
    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or("browser document is unavailable")?;
    for kind in ["copy", "cut"] {
        let capture = Closure::<dyn FnMut(web_sys::ClipboardEvent)>::new(
            move |event: web_sys::ClipboardEvent| {
                if event.is_trusted() {
                    ACTIVE_COPY_CUT.with(|active| *active.borrow_mut() = Some(event));
                }
            },
        );
        document
            .add_event_listener_with_callback_and_bool(kind, capture.as_ref().unchecked_ref(), true)
            .map_err(js_error)?;
        capture.forget();
    }
    CAPTURE_INSTALLED.with(|installed| *installed.borrow_mut() = true);
    Ok(())
}

fn write_active_event(prepared: &PreparedCopy) -> Option<Result<(), String>> {
    let expected = if prepared.cut.is_some() {
        "cut"
    } else {
        "copy"
    };
    let event = ACTIVE_COPY_CUT.with(|active| active.borrow_mut().take())?;
    if event.type_() != expected || event.event_phase() == 0 || !event.is_trusted() {
        return None;
    }
    let data = event.clipboard_data()?;
    Some(
        data.set_data("text/plain", &prepared.text)
            .and_then(|()| data.set_data("text/html", &prepared.html))
            .map(|()| event.prevent_default())
            .map_err(js_error),
    )
}

#[derive(Default)]
pub(super) struct Transport {
    completed: Rc<RefCell<VecDeque<Completion>>>,
}

impl Transport {
    pub(super) fn write(
        &mut self,
        ctx: &egui::Context,
        generation: u64,
        prepared: PreparedCopy,
    ) -> Option<Completion> {
        if let Some(result) = write_active_event(&prepared) {
            return Some(Completion::Write {
                generation,
                prepared,
                result,
            });
        }
        let promise = clipboard().and_then(|clipboard| {
            make_item(&prepared).and_then(|item| {
                let items = Array::of1(&item);
                call_promise(&clipboard, "write", Some(&items))
            })
        });
        let promise = match promise {
            Ok(promise) => promise,
            Err(error) => {
                return Some(Completion::Write {
                    generation,
                    prepared,
                    result: Err(error),
                });
            }
        };
        let completed = self.completed.clone();
        let ctx = ctx.clone();
        spawn_local(async move {
            let result = JsFuture::from(promise).await.map(|_| ()).map_err(js_error);
            completed.borrow_mut().push_back(Completion::Write {
                generation,
                prepared,
                result,
            });
            ctx.request_repaint();
        });
        None
    }

    pub(super) fn read(
        &mut self,
        ctx: &egui::Context,
        guard: DocumentGuard,
        fallback: Option<String>,
    ) -> Option<Completion> {
        let clipboard = match clipboard() {
            Ok(clipboard) => clipboard,
            Err(error) => {
                return Some(Completion::Read {
                    guard,
                    fallback,
                    result: Err(error),
                });
            }
        };
        let read = call_promise(&clipboard, "read", None);
        let text_only = read.is_err();
        let promise = match read.or_else(|_| call_promise(&clipboard, "readText", None)) {
            Ok(promise) => promise,
            Err(error) => {
                return Some(Completion::Read {
                    guard,
                    fallback,
                    result: Err(error),
                });
            }
        };
        let completed = self.completed.clone();
        let ctx = ctx.clone();
        spawn_local(async move {
            let result = if text_only {
                read_text_promise(promise).await
            } else {
                match read_items(promise).await {
                    Err(_) if fallback.is_none() => {
                        match call_promise(&clipboard, "readText", None) {
                            Ok(promise) => read_text_promise(promise).await,
                            Err(error) => Err(error),
                        }
                    }
                    other => other,
                }
            };
            completed.borrow_mut().push_back(Completion::Read {
                guard,
                fallback,
                result,
            });
            ctx.request_repaint();
        });
        None
    }

    pub(super) fn poll(&mut self) -> Vec<Completion> {
        self.completed.borrow_mut().drain(..).collect()
    }
}

fn clipboard() -> Result<JsValue, String> {
    let window = web_sys::window().ok_or("browser window is unavailable")?;
    if !window.is_secure_context() {
        return Err("the browser clipboard requires a secure context".into());
    }
    let clipboard = Reflect::get(window.navigator().as_ref(), &JsValue::from_str("clipboard"))
        .map_err(js_error)?;
    if clipboard.is_null() || clipboard.is_undefined() {
        return Err("the browser clipboard is unavailable".into());
    }
    Ok(clipboard)
}

fn call_promise(
    target: &JsValue,
    name: &str,
    argument: Option<&JsValue>,
) -> Result<Promise, String> {
    let function = Reflect::get(target, &JsValue::from_str(name))
        .map_err(js_error)?
        .dyn_into::<Function>()
        .map_err(|_| format!("browser clipboard {name} is unavailable"))?;
    let value = match argument {
        Some(argument) => function.call1(target, argument),
        None => function.call0(target),
    }
    .map_err(js_error)?;
    value
        .dyn_into::<Promise>()
        .map_err(|_| format!("browser clipboard {name} did not return a promise"))
}

fn make_blob(mime: &str, value: &str) -> Result<web_sys::Blob, String> {
    let parts = Array::of1(&Uint8Array::from(value.as_bytes()));
    let options = web_sys::BlobPropertyBag::new();
    options.set_type(mime);
    web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options).map_err(js_error)
}

fn make_item(prepared: &PreparedCopy) -> Result<web_sys::ClipboardItem, String> {
    let properties = Object::new();
    for (mime, value) in [
        ("text/plain", prepared.text.as_str()),
        ("text/html", prepared.html.as_str()),
    ] {
        let blob = make_blob(mime, value)?;
        Reflect::set(&properties, &JsValue::from_str(mime), &blob).map_err(js_error)?;
    }
    web_sys::ClipboardItem::new_with_record_from_str_to_blob_promise(&properties).map_err(js_error)
}

async fn read_items(promise: Promise) -> Result<Snapshot, String> {
    let result = JsFuture::from(promise).await.map_err(js_error)?;
    let items = Array::from(&result);
    for value in items.iter() {
        let item = value
            .dyn_into::<web_sys::ClipboardItem>()
            .map_err(|_| "browser returned an invalid clipboard item")?;
        let types = item.types();
        let has_type = |name: &str| {
            types
                .iter()
                .any(|value| value.as_string().as_deref() == Some(name))
        };
        if !has_type("text/plain") {
            continue;
        }
        let text = read_blob(&item, "text/plain", None).await?;
        let html = if has_type("text/html") {
            Some(read_blob(&item, "text/html", Some(MAX_HTML_BYTES)).await?)
        } else {
            None
        };
        return Ok(Snapshot { text, html });
    }
    Err("clipboard does not contain text".into())
}

async fn read_blob(
    item: &web_sys::ClipboardItem,
    mime: &str,
    limit: Option<usize>,
) -> Result<String, String> {
    let blob = JsFuture::from(item.get_type(mime))
        .await
        .map_err(js_error)?
        .dyn_into::<web_sys::Blob>()
        .map_err(|_| "browser returned invalid clipboard data")?;
    if limit.is_some_and(|limit| blob.size() > limit as f64) {
        return Err("clipboard HTML exceeds the size limit".into());
    }
    JsFuture::from(blob.text())
        .await
        .map_err(js_error)?
        .as_string()
        .ok_or_else(|| "browser returned invalid clipboard text".into())
}

async fn read_text_promise(promise: Promise) -> Result<Snapshot, String> {
    let text = JsFuture::from(promise)
        .await
        .map_err(js_error)?
        .as_string()
        .ok_or("browser returned invalid clipboard text")?;
    Ok(Snapshot { text, html: None })
}

fn js_error(error: JsValue) -> String {
    error
        .as_string()
        .or_else(|| {
            Reflect::get(&error, &JsValue::from_str("message"))
                .ok()
                .and_then(|message| message.as_string())
        })
        .unwrap_or_else(|| "browser clipboard operation failed".into())
}
