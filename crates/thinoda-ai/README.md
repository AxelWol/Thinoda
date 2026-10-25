_Last updated: 2026-10-25 13:14:37 UTC (non-binding repository metadata)._ <!-- docs-stamp -->

# Embeddings transport API

`thinoda-ai` supports explicitly configured OpenAI-compatible `/embeddings`
endpoints. This crate supplies transport, not app settings, semantic search,
clustering, model discovery, caching, or automatic document mutation.

```rust,no_run
use thinoda_ai::{AiConfig, AiEmbeddingJob, Client};

let config = AiConfig {
    base_url: "http://127.0.0.1:1234/v1".into(),
    ..AiConfig::default()
};
let inputs = vec!["First topic".to_owned(), "Second topic".to_owned()];
// Native blocking API (use `.await` instead in a browser build):
let vectors = Client::new(config.clone())
    .embeddings("explicit-embedding-model-id", &inputs)?;

// Identical native/browser polling API:
let job = AiEmbeddingJob::spawn(config, "explicit-embedding-model-id".into(), inputs);
// Poll from the application's regular update loop; never spin on the UI thread.
if let Some(result) = job.poll() {
    let vectors = result?;
}
// If the operation becomes obsolete:
job.cancel();
# Ok::<(), thinoda_ai::AiError>(())
```

## Stable interface

- `Client::embeddings(&self, model: &str, inputs: &[String]) -> Result<Vec<Vec<f32>>>`
  is blocking on native, `async` on wasm.
- `AiEmbeddingJob::spawn(config: AiConfig, model: String, inputs: Vec<String>) -> Self`
  starts one bounded batch off the UI thread.
- `poll(&self) -> Option<Result<Vec<Vec<f32>>>>` consumes exactly one terminal
  result; later polls return `None`.
- `cancel(&self)` aborts unfinished native I/O or browser fetch and immediately
  queues `Err(AiError::Cancelled)`. Late completion cannot replace that result.
  If completion has already been queued, cancellation does nothing.
- `is_finished(&self) -> bool` indicates a queued or consumed terminal result.
  Dropping an unfinished job also cancels it.

`model` is required explicitly and sent unchanged. `AiConfig::model` is a chat
setting and is **never** used as an embedding fallback. No `/models` request,
chat-model-name heuristic, retry with another model, or endpoint substitution
occurs. Requests send `model`, `input` (a text array), and
`encoding_format: "float"`. Base URL joining, configured bearer authentication
(including the existing environment fallback), connection timeout (5 seconds),
and overall timeout (`timeout_secs`, at least 1 second) follow existing client
behavior. Browser endpoints must allow CORS.

## Fixed bounds and validation

Public constants live in `thinoda_ai::embeddings`.

| Constant | Limit per operation |
|---|---|
| `MAX_MODEL_BYTES` | 256 UTF-8 bytes; model must not be blank |
| `MAX_INPUTS` | 128 texts; at least one required |
| `MAX_TEXT_BYTES` | 16 KiB UTF-8 per text; text must not be blank |
| `MAX_INPUT_BYTES` | 256 KiB UTF-8 across all texts |
| `MAX_DIMENSIONS` | 4096 components per vector; nonzero required |
| `MAX_RESPONSE_BYTES` | 2 MiB raw response, including HTTP error bodies |
| `MAX_VECTOR_BYTES` | 2 MiB total returned `f32` component storage |

The caller splits larger collections into bounded batches. The transport sends
exactly one request per accepted batch and returns no partial/fallback vectors.
The response must contain exactly one `data` entry per input, with unique integer
`index` values covering `0..inputs.len()`. Vectors are returned in input order
even when response entries are reordered. All vectors must have the same nonzero
dimension and only finite, representable `f32` numeric components. Base64
embeddings and malformed/missing indices or components are rejected.

Input limits are checked before request serialization. Response bytes are
bounded while reading, before JSON parsing. Typed array decoding caps both entry
and component allocations rather than building an unrestricted JSON tree.
At most `MAX_INPUTS * MAX_DIMENSIONS` floats are retained; ordering moves vectors
without copying their components. Request serialization, bounded response bytes,
parser bookkeeping, and output storage are separate bounded allocations; the
2 MiB vector bound is not a claim about total process RSS or HTTP/browser
internals. Callers own their input/configuration memory and limit concurrent jobs.

Errors use the existing `AiError`: `Parse` for input/protocol/size failures,
`Status(code, message)` for HTTP rejection (including unsupported endpoint/model),
`Http` for connection/read failures, and `Cancelled`. Server JSON error messages
are truncated to 300 characters; absent/unrecognized messages use a useful generic
endpoint rejection message. A response exceeding the byte bound is a size error
even if its HTTP status is unsuccessful.

## Local verification

Tests use only ephemeral loopback listeners, with no model downloads or external
test services. Set `THINODA_AI_SKIP_LIVE=1` when running the crate's existing test
suite. Use an isolated `CARGO_TARGET_DIR` when running alongside other sessions.

```sh
THINODA_AI_SKIP_LIVE=1 cargo test -p thinoda-ai
cargo clippy -p thinoda-ai --all-targets -- -D warnings
cargo check -p thinoda-ai --target wasm32-unknown-unknown
```

## Guided transformation tasks

`AiTask::Transform { action: TransformAction, source: String, instruction: String }`
uses the same typed streaming/terminal result API as existing map tasks.
`source` is caller-bounded JSON map reference data; the app sends selected subtree
titles and hierarchy only. The app bounds context to 500 topics/64 KiB and
instructions to 8192 bytes and requires explicit dispatch.

`Rewrite` produces `OutputKind::Title`/`AiOutput::Text`; it rejects empty,
multiline or over-1024-byte titles. `Grow` produces `Subtopics`/`Outline`.
`Translate`, `Reorganize` and `Refine` produce `Sheet`/`Outline`, requiring a
central Markdown heading. Their application policy is a new outline-only sheet,
not destructive replacement or a claim that metadata was translated.
Transformation output is bounded to 64 KiB/500 lines before outline parsing.
Only a completed valid stream returns a typed result; caller-owned review/Apply
and source/configuration validation remain mandatory.
