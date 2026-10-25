//! Bounded, explicit-model OpenAI-compatible embeddings.
//!
//! No model discovery, fallback, normalization, caching, or automatic splitting is
//! performed. Callers split larger collections into batches and retain model identity.

use std::fmt;
use std::sync::Arc;

use serde::Deserialize as DeriveDeserialize;
use serde::de::{Deserialize, Deserializer, SeqAccess, Visitor};
use serde_json::json;

use crate::stream::{Shared, spawn_job};
use crate::{AiConfig, AiError, AiStreamEvent, AiStreamJob, Client, Result};

/// Maximum number of texts in one request.
pub const MAX_INPUTS: usize = 128;
/// Maximum UTF-8 bytes in one text.
pub const MAX_TEXT_BYTES: usize = 16 * 1024;
/// Maximum UTF-8 bytes across a batch.
pub const MAX_INPUT_BYTES: usize = 256 * 1024;
/// Maximum UTF-8 bytes in the explicitly supplied model id.
pub const MAX_MODEL_BYTES: usize = 256;
/// Maximum components in each nonempty vector.
pub const MAX_DIMENSIONS: usize = 4096;
/// Maximum response body, including HTTP error bodies.
pub const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
/// Maximum returned numeric storage (128 * 4096 * 4 bytes).
pub const MAX_VECTOR_BYTES: usize = MAX_INPUTS * MAX_DIMENSIONS * size_of::<f32>();

fn invalid(message: impl Into<String>) -> AiError {
    AiError::Parse(format!("embeddings: {}", message.into()))
}

fn validate(model: &str, inputs: &[String]) -> Result<()> {
    if model.trim().is_empty() || model.len() > MAX_MODEL_BYTES {
        return Err(invalid(
            "an explicit model id of 1..=256 UTF-8 bytes is required",
        ));
    }
    if inputs.is_empty() || inputs.len() > MAX_INPUTS {
        return Err(invalid("batch must contain 1..=128 texts"));
    }
    let mut bytes = 0;
    for text in inputs {
        if text.trim().is_empty() || text.len() > MAX_TEXT_BYTES {
            return Err(invalid(
                "each text must be nonblank and at most 16384 UTF-8 bytes",
            ));
        }
        bytes += text.len();
        if bytes > MAX_INPUT_BYTES {
            return Err(invalid("batch exceeds 262144 UTF-8 bytes"));
        }
    }
    Ok(())
}

fn body(model: &str, inputs: &[String]) -> serde_json::Value {
    json!({"model": model, "input": inputs, "encoding_format": "float"})
}

// Do not deserialize untrusted arrays into an unbounded Vec or JSON Value.
struct Bounded<T, const N: usize>(Vec<T>);

impl<'de, T: Deserialize<'de>, const N: usize> Deserialize<'de> for Bounded<T, N> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct BoundedVisitor<T, const N: usize>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>, const N: usize> Visitor<'de> for BoundedVisitor<T, N> {
            type Value = Bounded<T, N>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "an array with at most {N} entries")
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                loop {
                    if values.len() == N {
                        if seq.next_element::<serde::de::IgnoredAny>()?.is_some() {
                            return Err(serde::de::Error::custom(format!(
                                "array exceeds {N} entries"
                            )));
                        }
                        break;
                    }
                    let Some(value) = seq.next_element()? else {
                        break;
                    };
                    values.push(value);
                }
                Ok(Bounded(values))
            }
        }
        deserializer.deserialize_seq(BoundedVisitor::<T, N>(std::marker::PhantomData))
    }
}

#[derive(DeriveDeserialize)]
struct Entry {
    index: usize,
    embedding: Bounded<f32, MAX_DIMENSIONS>,
}

#[derive(DeriveDeserialize)]
struct Response {
    data: Bounded<Entry, MAX_INPUTS>,
}

fn parse(status: u16, body: &[u8], count: usize) -> Result<Vec<Vec<f32>>> {
    if body.len() > MAX_RESPONSE_BYTES {
        return Err(invalid("response exceeds 2097152 bytes"));
    }
    if !(200..300).contains(&status) {
        // Bound server error text independently, and never materialize arbitrary JSON trees.
        #[derive(DeriveDeserialize)]
        struct ErrorMessage<'a> {
            #[serde(borrow)]
            message: std::borrow::Cow<'a, str>,
        }
        #[derive(DeriveDeserialize)]
        struct ErrorBody<'a> {
            #[serde(borrow)]
            error: ErrorMessage<'a>,
        }
        let message = serde_json::from_slice::<ErrorBody<'_>>(body)
            .ok()
            .map(|v| v.error.message)
            .unwrap_or_else(|| "endpoint rejected embeddings (unsupported model/API, authorization, or server error)".into());
        return Err(AiError::Status(status, message.chars().take(300).collect()));
    }
    let response: Response =
        serde_json::from_slice(body).map_err(|e| invalid(format!("invalid response: {e}")))?;
    if response.data.0.len() != count {
        return Err(invalid(format!(
            "expected {count} vectors, received {}",
            response.data.0.len()
        )));
    }
    let mut ordered: Vec<Option<Vec<f32>>> = (0..count).map(|_| None).collect();
    let mut dimensions = None;
    for entry in response.data.0 {
        let vector = entry.embedding.0;
        if vector.is_empty() {
            return Err(invalid("vector dimensions must be nonzero"));
        }
        if dimensions.is_some_and(|n| n != vector.len()) {
            return Err(invalid("inconsistent vector dimensions"));
        }
        dimensions = Some(vector.len());
        if vector.iter().any(|n| !n.is_finite()) {
            return Err(invalid("vector components must be finite f32 numbers"));
        }
        let slot = ordered
            .get_mut(entry.index)
            .ok_or_else(|| invalid("vector index is out of range"))?;
        if slot.replace(vector).is_some() {
            return Err(invalid("duplicate vector index"));
        }
    }
    ordered
        .into_iter()
        .map(|v| v.ok_or_else(|| invalid("missing vector index")))
        .collect()
}

fn append(body: &mut Vec<u8>, chunk: &[u8]) -> Result<()> {
    if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
        return Err(invalid("response exceeds 2097152 bytes"));
    }
    body.extend_from_slice(chunk);
    Ok(())
}

impl Client {
    /// Submit one bounded text batch to `/embeddings`, returning vectors in input order.
    ///
    /// `model` must be explicit; `AiConfig::model` is never consulted. See this
    /// module's constants for hard limits. Invalid input sends no request.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn embeddings(&self, model: &str, inputs: &[String]) -> Result<Vec<Vec<f32>>> {
        use std::io::Read;
        validate(model, inputs)?;
        let mut request = self.agent.post(self.url("embeddings"));
        if let Some(key) = self.api_key() {
            request = request.header("Authorization", format!("Bearer {key}"));
        }
        let mut response = request
            .send_json(body(model, inputs))
            .map_err(|e| AiError::Http(format!("embeddings request failed: {e}")))?;
        let status = response.status().as_u16();
        let mut reader = response.body_mut().as_reader();
        let mut body = Vec::new();
        let mut chunk = [0; 8192];
        loop {
            let size = reader
                .read(&mut chunk)
                .map_err(|e| AiError::Http(format!("embeddings response read failed: {e}")))?;
            if size == 0 {
                break;
            }
            append(&mut body, &chunk[..size])?;
        }
        parse(status, &body, inputs.len())
    }

    /// Browser equivalent of the blocking native method, with identical limits.
    #[cfg(target_arch = "wasm32")]
    pub async fn embeddings(&self, model: &str, inputs: &[String]) -> Result<Vec<Vec<f32>>> {
        validate(model, inputs)?;
        // Shared transport state uses Arc on both targets; browser jobs stay on one thread.
        #[allow(clippy::arc_with_non_send_sync)]
        let shared = Arc::new(Shared::<Vec<Vec<f32>>>::new());
        request(self.config(), model, inputs, shared).await
    }
}

/// A cancellable embeddings batch. Poll returns exactly one result; dropping
/// cancels unfinished work. Cancellation aborts native I/O/browser fetch and
/// suppresses late completion. Cancelling an already finished job is a no-op.
pub struct AiEmbeddingJob {
    inner: AiStreamJob<Vec<Vec<f32>>>,
}

impl AiEmbeddingJob {
    /// Start one batch using an explicitly supplied embedding model.
    pub fn spawn(cfg: AiConfig, model: String, inputs: Vec<String>) -> Self {
        Self {
            inner: spawn_job(cfg, move |cfg, shared| async move {
                validate(&model, &inputs)?;
                request(&cfg, &model, &inputs, shared).await
            }),
        }
    }

    /// Take the terminal result without blocking. Subsequent polls return `None`.
    pub fn poll(&self) -> Option<Result<Vec<Vec<f32>>>> {
        match self.inner.poll()? {
            AiStreamEvent::Finished(result) => Some(result),
            AiStreamEvent::Delta(_) => unreachable!("embedding jobs do not produce text deltas"),
        }
    }

    /// Abort unfinished work and queue `AiError::Cancelled` immediately.
    pub fn cancel(&self) {
        self.inner.cancel();
    }

    /// Whether the terminal result has been queued (possibly already consumed).
    pub fn is_finished(&self) -> bool {
        self.inner.is_finished()
    }
}

impl Drop for AiEmbeddingJob {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn request(
    cfg: &AiConfig,
    model: &str,
    inputs: &[String],
    shared: Arc<Shared<Vec<Vec<f32>>>>,
) -> Result<Vec<Vec<f32>>> {
    use crate::stream::{authorize, native_client};
    if shared.is_cancelled() {
        return Err(AiError::Cancelled);
    }
    let client = native_client(cfg)?;
    let mut response = authorize(
        client.post(crate::join_url(&cfg.base_url, "embeddings")),
        cfg,
    )
    .json(&body(model, inputs))
    .send()
    .await
    .map_err(|e| AiError::Http(format!("embeddings request failed: {e}")))?;
    let status = response.status().as_u16();
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| AiError::Http(format!("embeddings response read failed: {e}")))?
    {
        if shared.is_cancelled() {
            return Err(AiError::Cancelled);
        }
        append(&mut body, &chunk)?;
    }
    parse(status, &body, inputs.len())
}

#[cfg(target_arch = "wasm32")]
async fn request(
    cfg: &AiConfig,
    model: &str,
    inputs: &[String],
    shared: Arc<Shared<Vec<Vec<f32>>>>,
) -> Result<Vec<Vec<f32>>> {
    use crate::stream::{read_web_body, web_response_raw};
    struct RequestGuard(Arc<Shared<Vec<Vec<f32>>>>);
    impl Drop for RequestGuard {
        fn drop(&mut self) {
            self.0.abort_web_request();
        }
    }
    let _guard = RequestGuard(shared.clone());
    let response = web_response_raw(
        cfg,
        "embeddings",
        "POST",
        Some(&body(model, inputs)),
        &shared,
    )
    .await?;
    let status = response.status();
    let mut body = Vec::new();
    read_web_body(response, &shared, |chunk| {
        append(&mut body, chunk)?;
        Ok(false)
    })
    .await?;
    parse(status, &body, inputs.len())
}
