// Browser crash-recovery storage. This is deliberately separate from the
// synchronous localStorage VFS used for settings and ordinary documents.
//
// A Web Lock protects each session for its lifetime. Every IndexedDB mutation
// also checks a session token in the *same transaction* as the mutation. That
// fences an old page whose in-flight transaction outlives its Web Lock.
// Per-document revisions and durable tombstones prevent an older queued write
// from overwriting a newer write or resurrecting a removed snapshot.

const DB_NAME = "thinoda.recovery.v1";
// Version 2 records can contain pending discard decisions and receipts. A
// version 1 client must not silently ignore those decisions after upgrade.
const DB_VERSION = 2;
const LOCK_PREFIX = "thinoda.recovery.v1:";
const LEGACY_LEASE_ID = "legacy-localstorage";
const DB_MARKER = "thinoda.recovery.database.v1";
const OPERATION_TIMEOUT_MS = 15_000;
const STARTUP_TIMEOUT_MS = 30_000;
// Discovery must not synchronously materialize an unlimited number of
// snapshots. Exceeding any limit is an explicit incomplete scan, never an
// empty/successful recovery result. The originals remain in IndexedDB.
const MAX_DISCOVERY_SESSIONS = 256;
const MAX_DISCOVERY_DOCUMENTS = 512;
const MAX_DISCOVERY_DOCUMENTS_PER_SESSION = 128;
const MAX_DISCOVERY_SNAPSHOT_BYTES = 32 * 1024 * 1024;
const MAX_DISCOVERY_BYTES = 64 * 1024 * 1024;
// Rust strings cannot represent unpaired UTF-16 surrogates. Reject them
// before crossing the JS/Wasm bridge rather than silently replacing them.
const MAX_LEGACY_BRIDGE_CODE_UNITS = 32 * 1024 * 1024;
let database = null;
let databaseProblem = "";
let opening = null;
let createdDatabase = false;
let evictionDetected = false;
const leases = new Map();
const startupLocks = new Set();
const releasingLocks = new Map();
let startupGeneration = 0;
let startupHidden = false;
let resuming = null;
const accessibilityActions = [];
let accessibilityModel = "";
let accessibilityReviewExpanded = false;
let storagePersistent = false;
// This is an IndexedDB durability *hint*, not a guarantee against a device or
// browser failure. Browsers that ignore the option must not look equivalent to
// browsers that acknowledge it.
let storageStrictDurability = null;
let nextPersistenceCheck = 0;
let checkingPersistence = false;

function problem(error) {
    const name = error && typeof error.name === "string" ? error.name : "";
    if (name === "QuotaExceededError") {
        return "Browser recovery storage is full. Save or download your work and free site storage.";
    }
    if (name === "SecurityError" || name === "InvalidStateError") {
        return "Browser recovery storage is unavailable or access was denied. Save or download your work.";
    }
    return error && typeof error.message === "string"
        ? error.message
        : "Browser recovery storage failed. Save or download your work.";
}

function refreshPersistenceStatus() {
    if (checkingPersistence || Date.now() < nextPersistenceCheck) return;
    nextPersistenceCheck = Date.now() + 30_000;
    checkingPersistence = true;
    let timer;
    Promise.race([
        Promise.resolve().then(() => navigator.storage?.persisted?.() ?? false),
        new Promise(resolve => { timer = setTimeout(() => resolve(false), 3_000); }),
    ]).then(value => {
        storagePersistent = value === true;
    }, () => {
        storagePersistent = false;
    }).finally(() => {
        clearTimeout(timer);
        checkingPersistence = false;
    });
}

function storageError(message) {
    return new Error(message);
}

function unknownOutcome(message) {
    const error = new Error(message);
    error.name = "RecoveryOutcomeUnknown";
    return error;
}

function clock() {
    return globalThis.performance?.now?.() ?? Date.now();
}

function deadlineOrDefault(deadline) {
    return typeof deadline === "number" && Number.isFinite(deadline)
        ? deadline
        : clock() + OPERATION_TIMEOUT_MS;
}

function remaining(deadline) {
    return Math.max(0, Math.min(OPERATION_TIMEOUT_MS, deadline - clock()));
}

function ensureTime(deadline) {
    if (clock() >= deadline) {
        throw storageError("Browser recovery startup timed out. Crash recovery is unavailable; save or download your work.");
    }
}

function ensureStartup(deadline, generation) {
    ensureTime(deadline);
    if (startupHidden || generation !== startupGeneration) {
        throw storageError("Browser recovery discovery was deferred when the page was hidden. Retry after returning.");
    }
}

// All async startup phases share this deadline. Synchronous JavaScript/Rust
// work and a throttled browser event loop cannot be preempted by a timer.
export function recoveryStartupDeadline() {
    return clock() + STARTUP_TIMEOUT_MS;
}

export function recoveryStartupGeneration() {
    return startupGeneration;
}

export function recoveryStartupHasTime(deadline, generation) {
    return !startupHidden && generation === startupGeneration && clock() < deadline;
}

// A completed result may be delivered after its deadline, but never after a
// pagehide invalidated its generation. The deadline only limits discovery.
export function recoveryStartupIsCurrent(generation) {
    return !startupHidden && generation === startupGeneration;
}

export function recoveryYield() {
    return new Promise(resolve => setTimeout(resolve, 0));
}

// Canvas text alone is not reliably exposed to browser screen readers. Keep
// the same status in a focusable live region, with keyboard-accessible actions.
export function recoveryAccessibilityStatus(phase, message, retryLabel, reviewLabel, canRetry, canReview, modelJson = "{}") {
    if (!globalThis.document?.body) return;
    let panel = document.getElementById("thinoda-recovery-accessibility");
    if (!panel) {
        const style = document.createElement("style");
        style.textContent = `
            #thinoda-recovery-accessibility { position:fixed; left:-10000px; top:auto;
                width:1px; height:1px; overflow:hidden; z-index:2147483647; }
            #thinoda-recovery-accessibility:focus-within { left:1rem; top:1rem;
                width:auto; height:auto; max-width:min(36rem,calc(100vw - 2rem));
                max-height:calc(100vh - 2rem); overflow:auto;
                padding:1rem; background:#1f2430; color:#fff; font:16px/1.5 system-ui,sans-serif;
                border:2px solid #8ab4f8; border-radius:.5rem; }
            #thinoda-recovery-accessibility button { font:inherit; min-height:2.75rem;
                margin:.5rem .5rem 0 0; }
            #thinoda-recovery-accessibility :focus-visible { outline:3px solid #8ab4f8; outline-offset:3px; }
        `;
        document.head.append(style);
        panel = document.createElement("aside");
        panel.id = "thinoda-recovery-accessibility";
        accessibilityModel = "";
        const status = document.createElement("div");
        status.setAttribute("role", "status");
        status.setAttribute("aria-live", "polite");
        status.setAttribute("aria-atomic", "true");
        status.tabIndex = 0;
        const retry = document.createElement("button");
        retry.type = "button";
        retry.dataset.action = "retry";
        retry.addEventListener("click", () => { accessibilityActions.push("retry"); });
        const review = document.createElement("button");
        review.type = "button";
        review.dataset.action = "review";
        review.setAttribute("aria-expanded", "false");
        review.addEventListener("click", () => {
            accessibilityReviewExpanded = !accessibilityReviewExpanded;
            review.setAttribute("aria-expanded", String(accessibilityReviewExpanded));
            panel.querySelector('[data-section="review"]').hidden = !accessibilityReviewExpanded;
            if (accessibilityReviewExpanded) accessibilityActions.push("review");
        });
        const rescue = document.createElement("button");
        rescue.type = "button";
        rescue.dataset.action = "rescue";
        rescue.addEventListener("click", () => {
            try {
                const model = JSON.parse(accessibilityModel || "{}");
                recoveryOpenRescue(JSON.stringify(model.rescue || {}));
            } catch (error) {
                status.textContent = problem(error);
            }
        });
        const reviewSection = document.createElement("section");
        reviewSection.dataset.section = "review";
        reviewSection.hidden = true;
        const rescueSection = document.createElement("section");
        rescueSection.dataset.section = "rescue";
        panel.append(status, retry, review, rescue, reviewSection, rescueSection);
        document.body.append(panel);
    }
    panel.dataset.phase = phase;
    const status = panel.querySelector('[role="status"]');
    if (status.textContent !== message) status.textContent = message;
    const retry = panel.querySelector('[data-action="retry"]');
    const review = panel.querySelector('[data-action="review"]');
    const rescue = panel.querySelector('[data-action="rescue"]');
    const reviewSection = panel.querySelector('[data-section="review"]');
    let model;
    try { model = JSON.parse(modelJson); } catch { model = {}; }
    const canRescue = model.canRescue === true;
    const focusWillHide = (!canRetry && document.activeElement === retry)
        || (!canReview && (document.activeElement === review || reviewSection.contains(document.activeElement)))
        || (!canRescue && document.activeElement === rescue);
    if (retry.textContent !== retryLabel) retry.textContent = retryLabel;
    if (review.textContent !== reviewLabel) review.textContent = reviewLabel;
    retry.hidden = !canRetry;
    review.hidden = !canReview;
    rescue.hidden = !canRescue;
    rescue.textContent = model.rescue?.labels?.title || "Rescue original recovery data";
    if (!canReview) accessibilityReviewExpanded = false;
    review.setAttribute("aria-expanded", String(accessibilityReviewExpanded));
    reviewSection.hidden = !canReview || !accessibilityReviewExpanded;
    if (focusWillHide) status.focus({ preventScroll: true });
    if (accessibilityModel !== modelJson) {
        accessibilityModel = modelJson;
        const focused = panel.contains(document.activeElement) ? document.activeElement?.dataset?.focus : null;
        const labels = model.labels || {};
        const canDecide = model.canDecide !== false;
        const button = (label, action, focus, accessibleName = label) => {
            const element = document.createElement("button");
            element.type = "button";
            element.textContent = label;
            element.setAttribute("aria-label", accessibleName);
            element.dataset.focus = focus;
            element.addEventListener("click", () => {
                accessibilityActions.push(JSON.stringify(action));
            });
            return element;
        };
        reviewSection.replaceChildren();
        if ((model.sessions || []).length) {
            const retained = document.createElement("p");
            retained.textContent = labels.retained || "Original recovery snapshots use browser storage until discarded.";
            reviewSection.append(retained);
        }
        for (const session of model.sessions || []) {
            const group = document.createElement("div");
            for (const doc of session.docs || []) {
                const title = document.createElement("p");
                title.textContent = `${labels.original || "Original recovery snapshot"}: ${doc.title}`;
                group.append(title);
                const downloadLabel = doc.uncertain || doc.requiresRecheck
                    ? (labels.cached || "Download cached copy")
                    : (labels.download || "Download a copy");
                group.append(button(downloadLabel,
                    { action: "download-orphan", session: session.id, key: doc.key },
                    `orphan:${session.id}:${doc.key}`,
                    `${downloadLabel}: ${doc.title}`));
                group.append(button(labels.raw || "Download raw recovery data",
                    { action: "download-orphan-raw", session: session.id, key: doc.key },
                    `raw:${session.id}:${doc.key}`,
                    `${labels.raw || "Download raw recovery data"}: ${doc.title}`));
                if (doc.uncertain) {
                    const uncertain = document.createElement("p");
                    uncertain.textContent = labels.uncertain
                        || "Discard is pending or its result is unknown. Verify and complete it, keep the original, or download the cached copy.";
                    const verify = button(labels.verify || "Verify and complete discard",
                        { action: "verify-discard", session: session.id, key: doc.key },
                        `verify:${session.id}:${doc.key}`);
                    verify.disabled = !canDecide;
                    const keep = button(labels.keep || "Keep original",
                        { action: "keep-discard", session: session.id, key: doc.key },
                        `keep:${session.id}:${doc.key}`);
                    keep.disabled = !canDecide;
                    group.append(uncertain, verify, keep);
                } else if (doc.requiresRecheck) {
                    const recheck = document.createElement("p");
                    recheck.textContent = labels.recheck
                        || "Cached recovery copy needs rechecking. Download it or retry.";
                    group.append(recheck);
                } else if (doc.canDiscard === false) {
                    const retained = document.createElement("p");
                    retained.textContent = labels.legacyRetained
                        || "Legacy snapshots are kept because older tabs cannot be excluded safely.";
                    group.append(retained);
                } else {
                    const discard = document.createElement("button");
                    discard.type = "button";
                    discard.textContent = labels.discard || "Discard this original";
                    discard.setAttribute("aria-label", `${discard.textContent}: ${doc.title}`);
                    discard.dataset.focus = `discard:${session.id}:${doc.key}`;
                    discard.disabled = !canDecide;
                    const confirm = button(labels.confirmDiscard || "Confirm discard",
                        { action: "discard-doc", session: session.id, key: doc.key },
                        `confirm:${session.id}:${doc.key}`,
                        `${labels.confirmDiscard || "Confirm discard"}: ${doc.title}`);
                    confirm.disabled = !canDecide;
                    const warning = document.createElement("p");
                    warning.textContent = labels.discardWarning
                        || "This requests discard. The original remains until you verify and complete it.";
                    const cancel = document.createElement("button");
                    cancel.type = "button";
                    cancel.textContent = labels.cancel || "Cancel";
                    cancel.setAttribute("aria-label", `${cancel.textContent}: ${doc.title}`);
                    cancel.dataset.focus = `cancel-discard:${session.id}:${doc.key}`;
                    warning.hidden = true;
                    confirm.hidden = true;
                    cancel.hidden = true;
                    discard.addEventListener("click", () => {
                        warning.hidden = false;
                        confirm.hidden = false;
                        cancel.hidden = false;
                        confirm.focus();
                    });
                    cancel.addEventListener("click", () => {
                        warning.hidden = true;
                        confirm.hidden = true;
                        cancel.hidden = true;
                        discard.focus();
                    });
                    group.append(discard, warning, confirm, cancel);
                }
            }
            const restore = button(labels.restore || "Restore",
                { action: "restore", session: session.id }, `restore:${session.id}`,
                `${labels.restore || "Restore"}: ${(session.docs || []).map(doc => doc.title).join(", ")}`);
            restore.disabled = !model.canRestore || !!session.copied || !!session.uncertain
                || !!session.requiresRecheck;
            if (session.copied) {
                const copied = document.createElement("p");
                copied.textContent = labels.copied || "A recovered copy is already open; the original is retained.";
                group.append(copied);
            }
            group.append(restore);
            reviewSection.append(group);
        }
        const restoreAll = button(labels.restoreAll || "Restore All", { action: "restore-all" }, "restore-all");
        restoreAll.disabled = !model.canRestore || !(model.sessions || [])
            .some(session => !session.copied && !session.uncertain && !session.requiresRecheck);
        reviewSection.append(restoreAll);
        const rescueSection = panel.querySelector('[data-section="rescue"]');
        rescueSection.replaceChildren();
        for (const doc of model.documents || []) {
            const group = document.createElement("div");
            const title = document.createElement("p");
            title.textContent = doc.title;
            group.append(title, button(labels.download || "Download a copy",
                { action: "download-doc", key: doc.key }, `document:${doc.key}`,
                `${labels.download || "Download a copy"}: ${doc.title}`));
            if (doc.drafts) {
                group.append(button(labels.draftDetails || "Download unfinished draft details",
                    { action: "download-drafts", key: doc.key }, `drafts:${doc.key}`));
                group.append(button(labels.committed || "Download workbook without unfinished drafts",
                    { action: "download-committed", key: doc.key }, `committed:${doc.key}`));
            }
            if (doc.ink) {
                const retryInk = button(labels.retryInk || "Retry ink",
                    { action: "retry-ink", key: doc.key }, `retry-ink:${doc.key}`);
                retryInk.disabled = !doc.canRetryInk;
                const discardInk = document.createElement("button");
                discardInk.type = "button";
                discardInk.textContent = labels.discardInk || "Discard ink";
                discardInk.dataset.focus = `discard-ink:${doc.key}`;
                const confirmInk = button(labels.confirmDiscard || "Confirm discard",
                    { action: "discard-ink", key: doc.key }, `confirm-ink:${doc.key}`);
                confirmInk.hidden = true;
                discardInk.addEventListener("click", () => {
                    confirmInk.hidden = false;
                    confirmInk.focus();
                });
                group.append(retryInk, discardInk, confirmInk);
            }
            rescueSection.append(group);
        }
        if (focused) {
            const next = Array.from(panel.querySelectorAll("[data-focus]"))
                .find(element => element.dataset.focus === focused && !element.hidden && !element.disabled);
            (next || status).focus({ preventScroll: true });
        }
    }
}

export function recoveryTakeAccessibilityAction() {
    return accessibilityActions.shift() || "";
}

// Rescue is deliberately separate from discovery: it opens existing storage
// read-only, needs no Web Lock, and never claims, deletes, or rewrites a
// source. Listing uses keys rather than materializing oversized records.
const RESCUE_PAGE_SIZE = 25;
const RESCUE_LEGACY_SCAN_SIZE = 64;
const RESCUE_STRING_CHUNK = 64 * 1024;
const RESCUE_MAX_GRAPH_VALUES = 100_000;
const RESCUE_MAX_GRAPH_DEPTH = 128;
const RESCUE_MAX_HEADER_BYTES = 4 * 1024 * 1024;
const RESCUE_MAGIC = "THINODA-RECOVERY-RECORD-1\n";

function rescueCheckCancellation(signal) {
    if (signal?.aborted) throw storageError("The rescue copy was cancelled; the original was not changed.");
}

// Do not depend on a browser's version of well-formed JSON.stringify: even an
// older implementation that emits lone surrogates must not let Blob replace
// them with U+FFFD. Escaping all surrogate code units is valid JSON.
function rescueJson(value) {
    return JSON.stringify(value).replace(/[\uD800-\uDFFF]/g, character =>
        `\\u${character.charCodeAt(0).toString(16).padStart(4, "0")}`);
}

function rescueLegacyStorage(prefix) {
    if (typeof prefix !== "string" || !prefix.startsWith("thinoda.vfs:")
        || !prefix.endsWith("/recovery/")) {
        throw storageError("The legacy recovery location is unavailable.");
    }
    return globalThis.localStorage;
}

// Only bridge strings that Rust can represent without loss. Malformed or
// oversized originals remain available through the JS-only rescue archive.
export function recoveryReadLegacyValue(key) {
    if (typeof key !== "string" || !key.startsWith("thinoda.vfs:")) {
        throw storageError("The legacy recovery location is unavailable.");
    }
    const value = globalThis.localStorage.getItem(key);
    if (value === null) return null;
    if (value.length > MAX_LEGACY_BRIDGE_CODE_UNITS) {
        throw storageError("A legacy recovery entry exceeds the safe preview limit. Its original remains available for rescue.");
    }
    let wellFormed = true;
    if (typeof value.isWellFormed === "function") {
        wellFormed = value.isWellFormed();
    } else {
        for (let index = 0; index < value.length; index++) {
            const unit = value.charCodeAt(index);
            if (unit >= 0xD800 && unit <= 0xDBFF) {
                const next = value.charCodeAt(index + 1);
                if (!(next >= 0xDC00 && next <= 0xDFFF)) {
                    wellFormed = false;
                    break;
                }
                index++;
            } else if (unit >= 0xDC00 && unit <= 0xDFFF) {
                wellFormed = false;
                break;
            }
        }
    }
    if (!wellFormed) {
        throw storageError("A legacy recovery entry contains unpaired UTF-16. Its original remains available for rescue.");
    }
    return value;
}

// localStorage has no cursor. Scan a bounded number of keys per explicit
// page; concurrent changes can reorder keys, so a refresh may be necessary.
export function recoveryRescueLegacyPage(prefix, start = 0) {
    const storage = rescueLegacyStorage(prefix);
    if (!Number.isSafeInteger(start) || start < 0) throw storageError("Invalid rescue page.");
    const count = storage.length;
    if (start > count) throw storageError("Legacy storage changed. Refresh the rescue list.");
    const end = Math.min(count, start + RESCUE_LEGACY_SCAN_SIZE);
    const keys = [];
    for (let index = start; index < end; index++) {
        const key = storage.key(index);
        if (key === null) throw storageError("Legacy storage changed. Refresh the rescue list.");
        if (key.startsWith(prefix)) keys.push(key);
    }
    if (storage.length !== count) throw storageError("Legacy storage changed. Refresh the rescue list.");
    return { keys, next: end < count ? end : null };
}

async function rescueStringParts(value, signal) {
    const parts = [];
    for (let offset = 0; offset < value.length; offset += RESCUE_STRING_CHUNK) {
        rescueCheckCancellation(signal);
        // JSON string lines preserve even unpaired UTF-16 surrogates. Joining
        // the parsed lines reconstructs exactly the captured storage value.
        parts.push(`${rescueJson(value.slice(offset, offset + RESCUE_STRING_CHUNK))}\n`);
        if ((offset / RESCUE_STRING_CHUNK) % 8 === 7) await recoveryYield();
    }
    rescueCheckCancellation(signal);
    return new Blob(parts, { type: "application/octet-stream" });
}

export async function recoveryRescueLegacyArchive(prefix, key, signal) {
    const storage = rescueLegacyStorage(prefix);
    if (typeof key !== "string" || !key.startsWith(prefix)) {
        throw storageError("The selected entry is not in legacy recovery storage.");
    }
    rescueCheckCancellation(signal);
    // getItem itself is synchronous and may materialize one large value. It is
    // performed only after the user selects a source, never during startup.
    const value = storage.getItem(key);
    if (value === null) throw storageError("The selected legacy entry disappeared. Refresh the list.");
    const chunks = await rescueStringParts(value, signal);
    const header = rescueJson({
        format: "thinoda-localstorage-rescue-1",
        key,
        utf16Length: value.length,
        instructions: "Parse each following JSON string line and concatenate them in order.",
    });
    rescueCheckCancellation(signal);
    return new Blob([`${header}\n`, chunks], { type: "application/octet-stream" });
}

// Open without a version and abort an upgrade: rescue must not create an
// empty database or require Web Locks when recovery initialization failed.
function rescueOpenExistingDatabase() {
    if (!globalThis.indexedDB) return Promise.reject(storageError("IndexedDB is unavailable."));
    return new Promise((resolve, reject) => {
        let settled = false;
        let absent = false;
        const timer = setTimeout(() => {
            if (!settled) {
                settled = true;
                reject(storageError("Opening recovery storage for rescue timed out."));
            }
        }, OPERATION_TIMEOUT_MS);
        let request;
        try {
            request = globalThis.indexedDB.open(DB_NAME);
        } catch (error) {
            clearTimeout(timer);
            reject(error);
            return;
        }
        request.onupgradeneeded = () => {
            absent = true;
            request.transaction.abort();
        };
        request.onblocked = () => {
            if (!settled) {
                settled = true;
                clearTimeout(timer);
                reject(storageError("Recovery storage is blocked by another page."));
            }
        };
        request.onerror = () => {
            if (!settled) {
                settled = true;
                clearTimeout(timer);
                reject(absent ? storageError("No existing browser recovery database was found.")
                    : request.error || storageError("Could not open recovery storage."));
            }
        };
        request.onsuccess = () => {
            const opened = request.result;
            if (settled) {
                opened.close();
                return;
            }
            settled = true;
            clearTimeout(timer);
            opened.onversionchange = () => opened.close();
            resolve(opened);
        };
    });
}

function rescueStoreName(store) {
    if (store !== "documents" && store !== "sessions") {
        throw storageError("Invalid browser recovery store.");
    }
    return store;
}

export async function recoveryRescueBrowserPage(store, after = null) {
    rescueStoreName(store);
    const range = after === null ? undefined : IDBKeyRange.lowerBound(after, true);
    const connection = await rescueOpenExistingDatabase();
    try {
        if (!connection.objectStoreNames.contains(store)) {
            throw storageError("The selected recovery store is missing.");
        }
        const tx = connection.transaction([store], "readonly");
        const done = completed(tx, { value: null });
        const request = tx.objectStore(store).openKeyCursor(range);
        const keys = [];
        let more = false;
        request.onsuccess = () => {
            const cursor = request.result;
            if (!cursor) return;
            if (keys.length === RESCUE_PAGE_SIZE) {
                more = true;
                return;
            }
            keys.push(cursor.primaryKey);
            cursor.continue();
        };
        await done;
        return { keys, next: more ? keys[keys.length - 1] : null };
    } finally {
        connection.close();
    }
}

// A record archive is a length-prefixed UTF-8 JSON graph followed by its
// binary parts. It preserves keys, repeated references, cycles, and binary
// bytes without interpreting the snapshot as a workbook. Unsupported browser
// objects fail explicitly; no partial archive is presented as complete.
async function rescueRecordArchive(store, key, record, signal) {
    const nodes = [];
    const seen = new Map();
    const sources = [];
    let values = 0;
    const encode = (value, depth = 0) => {
        if (++values > RESCUE_MAX_GRAPH_VALUES || depth > RESCUE_MAX_GRAPH_DEPTH) {
            throw storageError("This record is too complex to export losslessly. Its original was retained.");
        }
        if (value === null) return { type: "null" };
        if (value === undefined) return { type: "undefined" };
        if (typeof value === "boolean") return { type: "boolean", value };
        if (typeof value === "number") {
            const encoded = Number.isNaN(value) ? "NaN"
                : value === Infinity ? "Infinity"
                    : value === -Infinity ? "-Infinity"
                        : Object.is(value, -0) ? "-0" : value;
            return { type: "number", value: encoded };
        }
        if (typeof value === "bigint") return { type: "bigint", value: value.toString() };
        if (typeof value === "string") {
            if (value.length <= RESCUE_STRING_CHUNK) return { type: "string", value };
            const part = sources.length;
            sources.push({ encoding: "json-string-lines", value });
            return { type: "string-part", part, utf16Length: value.length };
        }
        if (typeof value !== "object") {
            throw storageError("This record contains an unsupported value. Its original was retained.");
        }
        if (seen.has(value)) return { ref: seen.get(value) };
        const index = nodes.length;
        seen.set(value, index);
        nodes.push(null);
        let node;
        if (value instanceof ArrayBuffer) {
            const part = sources.length;
            sources.push({ encoding: "raw", value });
            node = { type: "array-buffer", part, byteLength: value.byteLength };
        } else if (ArrayBuffer.isView(value)) {
            const type = value.constructor?.name;
            const allowed = [
                "DataView", "Int8Array", "Uint8Array", "Uint8ClampedArray", "Int16Array",
                "Uint16Array", "Int32Array", "Uint32Array", "Float32Array", "Float64Array",
                "BigInt64Array", "BigUint64Array",
            ];
            if (!allowed.includes(type)) throw storageError("This record has an unsupported binary view.");
            node = {
                type: "view",
                viewType: type,
                buffer: encode(value.buffer, depth + 1),
                byteOffset: value.byteOffset,
                byteLength: value.byteLength,
            };
        } else if (value instanceof Blob) {
            const part = sources.length;
            sources.push({ encoding: "raw", value });
            node = { type: "blob", part, byteLength: value.size, mediaType: value.type };
            if (typeof File !== "undefined" && value instanceof File) {
                node.type = "file";
                node.name = encode(value.name, depth + 1);
                node.lastModified = value.lastModified;
            }
        } else if (value instanceof Date) {
            node = { type: "date", time: encode(value.getTime(), depth + 1) };
        } else if (value instanceof RegExp) {
            node = {
                type: "regexp",
                source: encode(value.source, depth + 1),
                flags: value.flags,
                lastIndex: value.lastIndex,
            };
        } else if (value instanceof Map) {
            node = {
                type: "map",
                entries: Array.from(value, ([k, v]) => [encode(k, depth + 1), encode(v, depth + 1)]),
            };
        } else if (value instanceof Set) {
            node = { type: "set", entries: Array.from(value, v => encode(v, depth + 1)) };
        } else if (Array.isArray(value) || Object.getPrototypeOf(value) === Object.prototype
            || Object.getPrototypeOf(value) === null) {
            const keys = Object.keys(value);
            const names = Object.getOwnPropertyNames(value);
            if ((Array.isArray(value) ? names.length !== keys.length + 1 : names.length !== keys.length)
                || Object.getOwnPropertySymbols(value).length) {
                throw storageError("This record has unsupported properties. Its original was retained.");
            }
            node = {
                type: Array.isArray(value) ? "array" : "object",
                ...(Array.isArray(value) ? { length: value.length } : {}),
                ...(!Array.isArray(value) && Object.getPrototypeOf(value) === null
                    ? { nullPrototype: true } : {}),
                properties: keys.map(name => [name, encode(value[name], depth + 1)]),
            };
        } else {
            throw storageError("This record contains an unsupported browser object. Its original was retained.");
        }
        nodes[index] = node;
        return { ref: index };
    };
    const encodedKey = encode(key);
    const encodedRecord = encode(record);
    const payloads = [];
    const parts = [];
    for (const source of sources) {
        rescueCheckCancellation(signal);
        const payload = source.encoding === "json-string-lines"
            ? await rescueStringParts(source.value, signal)
            : new Blob([source.value], { type: "application/octet-stream" });
        payloads.push(payload);
        parts.push({ encoding: source.encoding, byteLength: payload.size });
    }
    const header = rescueJson({
        format: "thinoda-indexeddb-rescue-1", store, key: encodedKey,
        record: encodedRecord, nodes, parts,
    });
    const headerBytes = new TextEncoder().encode(header);
    if (headerBytes.byteLength > RESCUE_MAX_HEADER_BYTES) {
        throw storageError("This record's metadata is too large to export losslessly. Its original was retained.");
    }
    rescueCheckCancellation(signal);
    return new Blob(
        [RESCUE_MAGIC, `${headerBytes.byteLength}\n`, headerBytes, ...payloads],
        { type: "application/octet-stream" },
    );
}

export async function recoveryRescueBrowserRecord(store, key, signal) {
    rescueStoreName(store);
    rescueCheckCancellation(signal);
    const connection = await rescueOpenExistingDatabase();
    let record;
    try {
        rescueCheckCancellation(signal);
        if (!connection.objectStoreNames.contains(store)) {
            throw storageError("The selected recovery store is missing.");
        }
        const tx = connection.transaction([store], "readonly");
        const done = completed(tx, { value: null });
        const request = tx.objectStore(store).get(key);
        request.onsuccess = () => { record = request.result; };
        await done;
    } finally {
        connection.close();
    }
    if (record === undefined) throw storageError("The selected recovery record disappeared. Refresh the list.");
    rescueCheckCancellation(signal);
    const bytes = record && typeof record === "object" ? record.bytes : null;
    const raw = bytes instanceof ArrayBuffer || ArrayBuffer.isView(bytes)
        ? new Blob([bytes], { type: "application/octet-stream" }) : null;
    try {
        const archive = await rescueRecordArchive(store, key, record, signal);
        return { archive, raw, warning: "" };
    } catch (error) {
        rescueCheckCancellation(signal);
        if (!raw) throw error;
        return { archive: null, raw, warning: problem(error) };
    }
}

function rescueKeyLabel(key) {
    if (typeof key === "string") {
        return JSON.stringify(key.slice(0, 160)) + (key.length > 160 ? "…" : "");
    }
    if (typeof key === "number") return String(key);
    if (key instanceof Date) return `Date (${key.toISOString()})`;
    if (Array.isArray(key)) return `Array key (${key.length} items)`;
    if (key instanceof ArrayBuffer || ArrayBuffer.isView(key)) {
        return `Binary key (${key.byteLength} bytes)`;
    }
    return "Recovery record";
}

// This panel is opened only by an explicit user action. A finished discovery
// never opens it, switches documents, or changes the editing gate.
export function recoveryOpenRescue(configJson = "{}") {
    if (!globalThis.document?.body) throw storageError("The recovery rescue panel is unavailable.");
    const alreadyOpen = document.getElementById("thinoda-recovery-rescue");
    if (alreadyOpen) {
        alreadyOpen.querySelector('[data-action="close"]')?.focus();
        return;
    }
    let config;
    try { config = JSON.parse(configJson); } catch { config = {}; }
    const labels = config.labels || {};
    const text = (name, fallback) => typeof labels[name] === "string" ? labels[name] : fallback;
    const legacyPrefix = typeof config.legacyPrefix === "string" ? config.legacyPrefix : "";
    const previouslyFocused = document.activeElement;
    const overlay = document.createElement("div");
    overlay.id = "thinoda-recovery-rescue";
    const style = document.createElement("style");
    style.textContent = `
        #thinoda-recovery-rescue { position:fixed; inset:0; z-index:2147483647;
            display:flex; align-items:center; justify-content:center;
            padding:1rem; background:rgba(0,0,0,.7); }
        #thinoda-recovery-rescue section { box-sizing:border-box; width:min(48rem,100%);
            max-height:calc(100vh - 2rem); overflow:auto; padding:1rem;
            background:#1f2430; color:#fff; border:2px solid #8ab4f8;
            border-radius:.5rem; font:16px/1.5 system-ui,sans-serif; }
        #thinoda-recovery-rescue button, #thinoda-recovery-rescue a { font:inherit;
            min-height:2.75rem; margin:.25rem .5rem .25rem 0; }
        #thinoda-recovery-rescue a { color:#bcd7ff; display:inline-block; }
        #thinoda-recovery-rescue :focus-visible { outline:3px solid #8ab4f8; outline-offset:3px; }
        #thinoda-recovery-rescue [data-section="records"] button { display:block;
            max-width:100%; overflow-wrap:anywhere; text-align:left; }
    `;
    const dialog = document.createElement("section");
    dialog.setAttribute("role", "dialog");
    dialog.setAttribute("aria-modal", "true");
    dialog.setAttribute("aria-labelledby", "thinoda-recovery-rescue-title");
    dialog.setAttribute("aria-describedby", "thinoda-recovery-rescue-description");
    const title = document.createElement("h2");
    title.id = "thinoda-recovery-rescue-title";
    title.textContent = text("title", "Rescue original recovery data");
    const description = document.createElement("p");
    description.id = "thinoda-recovery-rescue-description";
    description.textContent = text("description",
        "Read-only rescue. Downloads do not resolve recovery or change browser storage.");
    const privacy = document.createElement("p");
    privacy.textContent = text("privacy", "Raw recovery copies may contain private content.");
    const changed = document.createElement("p");
    changed.textContent = text("changed", "Other tabs may change these entries. Refresh the list to check again.");
    const makeButton = (label, action) => {
        const element = document.createElement("button");
        element.type = "button";
        element.textContent = label;
        element.dataset.action = action;
        return element;
    };
    const close = makeButton(text("close", "Close"), "close");
    const legacy = makeButton(text("legacy", "Legacy recovery entries"), "legacy");
    const documents = makeButton(text("documents", "Browser recovery documents"), "documents");
    const sessions = makeButton(text("sessions", "Browser recovery session records"), "sessions");
    legacy.disabled = !legacyPrefix;
    const legacyUnavailable = document.createElement("p");
    legacyUnavailable.hidden = !legacy.disabled;
    legacyUnavailable.textContent = `${text("legacy", "Legacy recovery entries")}: ${text(
        "failure", "Recovery data could not be read. The original was not changed.",
    )}`;
    const refresh = makeButton(text("refresh", "Refresh list"), "refresh");
    const next = makeButton(text("next", "Next entries"), "next");
    refresh.hidden = true;
    next.hidden = true;
    const status = document.createElement("p");
    status.setAttribute("role", "status");
    status.setAttribute("aria-live", "polite");
    status.textContent = text("choose", "Select a record to prepare a download.");
    const records = document.createElement("div");
    records.dataset.section = "records";
    const offers = document.createElement("div");
    offers.dataset.section = "downloads";
    const archiveNotice = document.createElement("p");
    archiveNotice.textContent = text("archive",
        "This archive is a rescue copy, not a workbook; keep the original browser data.");
    dialog.append(title, description, privacy, changed, close, legacy, legacyUnavailable, documents, sessions,
        refresh, next, status, records, offers, archiveNotice);
    overlay.append(style, dialog);
    document.body.append(overlay);

    let selection = "";
    let nextPage = null;
    let sequence = 0;
    let pending = null;
    let objectUrls = [];
    const clearOffers = () => {
        offers.replaceChildren();
        // A user may already have activated a download. Give the browser time
        // to consume its URL before releasing the Blob.
        for (const url of objectUrls) setTimeout(() => URL.revokeObjectURL(url), 60_000);
        objectUrls = [];
    };
    const cancelPending = () => {
        sequence++;
        pending?.abort();
        pending = null;
    };
    const closePanel = () => {
        cancelPending();
        clearOffers();
        overlay.remove();
        if (previouslyFocused?.isConnected && typeof previouslyFocused.focus === "function") {
            previouslyFocused.focus({ preventScroll: true });
        }
    };
    close.addEventListener("click", closePanel);
    dialog.addEventListener("keydown", event => {
        if (event.key === "Escape") {
            event.preventDefault();
            closePanel();
        } else if (event.key === "Tab") {
            const focusable = Array.from(dialog.querySelectorAll("button:not(:disabled), a[href]"))
                .filter(element => !element.hidden && element.getClientRects().length);
            if (!focusable.length) return;
            const first = focusable[0];
            const last = focusable[focusable.length - 1];
            if (event.shiftKey && document.activeElement === first) {
                event.preventDefault();
                last.focus();
            } else if (!event.shiftKey && document.activeElement === last) {
                event.preventDefault();
                first.focus();
            }
        }
    });

    const addDownload = (blob, label, name, action) => {
        const url = URL.createObjectURL(blob);
        objectUrls.push(url);
        const link = document.createElement("a");
        link.href = url;
        link.download = name;
        link.textContent = label;
        link.dataset.action = action;
        offers.append(link);
    };
    const prepare = async key => {
        cancelPending();
        const current = sequence;
        pending = new AbortController();
        const signal = pending.signal;
        clearOffers();
        status.textContent = text("preparing", "Preparing a rescue copy…");
        try {
            if (selection === "legacy") {
                const archive = await recoveryRescueLegacyArchive(legacyPrefix, key, signal);
                if (current !== sequence) return;
                addDownload(archive, text("download", "Download prepared rescue copy"),
                    "thinoda-legacy-recovery.ndjson", "download-archive");
                status.textContent = text("ready", "Rescue copy prepared. Use the download link.");
            } else {
                const result = await recoveryRescueBrowserRecord(selection, key, signal);
                if (current !== sequence) return;
                if (result.archive) {
                    addDownload(result.archive, text("download", "Download prepared rescue copy"),
                        "thinoda-recovery-record.tdr", "download-archive");
                }
                if (result.raw) {
                    addDownload(result.raw, text("raw", "Download raw recovery data"),
                        "thinoda-recovery-bytes.bin", "download-raw");
                }
                status.textContent = result.warning
                    ? `${text("failure", "Recovery data could not be read. The original was not changed.")} ${result.warning}`
                    : text("ready", "Rescue copy prepared. Use the download link.");
            }
        } catch (error) {
            if (current === sequence) {
                status.textContent = `${text("failure", "Recovery data could not be read. The original was not changed.")} ${problem(error)}`;
            }
        } finally {
            if (current === sequence) pending = null;
        }
    };
    const list = async (kind, after = null) => {
        cancelPending();
        const current = sequence;
        selection = kind;
        nextPage = null;
        clearOffers();
        records.replaceChildren();
        refresh.hidden = false;
        next.hidden = true;
        for (const button of [legacy, documents, sessions]) {
            button.setAttribute("aria-pressed", String(button.dataset.action === kind));
        }
        status.textContent = text("preparing", "Preparing a rescue copy…");
        try {
            const page = kind === "legacy"
                ? recoveryRescueLegacyPage(legacyPrefix, after === null ? 0 : after)
                : await recoveryRescueBrowserPage(kind, after);
            if (current !== sequence) return;
            nextPage = page.next;
            next.hidden = nextPage === null;
            for (const key of page.keys) {
                const record = makeButton(
                    `${text("prepare", "Prepare download")}: ${rescueKeyLabel(key)}`,
                    "prepare",
                );
                record.addEventListener("click", () => { void prepare(key); });
                records.append(record);
            }
            status.textContent = page.keys.length
                ? text("choose", "Select a record to prepare a download.")
                : text("empty", "No entries on this page.");
        } catch (error) {
            if (current === sequence) {
                status.textContent = `${text("failure", "Recovery data could not be read. The original was not changed.")} ${problem(error)}`;
            }
        }
    };
    legacy.addEventListener("click", () => { void list("legacy"); });
    documents.addEventListener("click", () => { void list("documents"); });
    sessions.addEventListener("click", () => { void list("sessions"); });
    refresh.addEventListener("click", () => { if (selection) void list(selection); });
    next.addEventListener("click", () => { if (selection && nextPage !== null) void list(selection, nextPage); });
    close.focus({ preventScroll: true });
}

// Let the app present its first frame before potentially expensive discovery
// and snapshot parsing. A hidden tab or a stalled animation frame gets a
// bounded fallback; this delay is not a claim that recovery is ready.
export function recoveryAfterFirstPaint() {
    return new Promise(resolve => {
        let settled = false;
        const done = () => {
            if (settled) return;
            settled = true;
            clearTimeout(timer);
            resolve();
        };
        const timer = setTimeout(done, 250);
        if (globalThis.document?.hidden || !globalThis.requestAnimationFrame) {
            setTimeout(done, 0);
        } else {
            requestAnimationFrame(() => requestAnimationFrame(done));
        }
    });
}

function openDatabase(deadline) {
    try { ensureTime(deadline); } catch (error) { return Promise.reject(error); }
    if (database) return Promise.resolve(database);
    if (opening) {
        // A caller joining an existing open still has its own startup budget.
        // Timing it out must not cancel another attempt or adopt a late result.
        return new Promise((resolve, reject) => {
            let settled = false;
            const timer = setTimeout(() => {
                if (!settled) {
                    settled = true;
                    reject(storageError("Opening browser recovery storage timed out. Crash recovery is unavailable."));
                }
            }, remaining(deadline));
            opening.then(value => {
                if (settled) return;
                settled = true;
                clearTimeout(timer);
                if (clock() >= deadline) {
                    reject(storageError("Opening browser recovery storage timed out. Crash recovery is unavailable."));
                } else {
                    resolve(value);
                }
            }, error => {
                if (settled) return;
                settled = true;
                clearTimeout(timer);
                reject(error);
            });
        });
    }
    if (!globalThis.indexedDB) {
        return Promise.reject(storageError("IndexedDB is unavailable. Crash recovery is disabled."));
    }
    if (!globalThis.navigator?.locks?.request) {
        return Promise.reject(storageError("Browser Web Locks are unavailable. Crash recovery is disabled to prevent conflicting tabs."));
    }
    opening = new Promise((resolve, reject) => {
        let settled = false;
        let wasCreated = false;
        const timer = setTimeout(() => {
            if (!settled) {
                settled = true;
                reject(storageError("Opening browser recovery storage timed out. Crash recovery is unavailable."));
            }
        }, remaining(deadline));
        let request;
        try {
            request = indexedDB.open(DB_NAME, DB_VERSION);
        } catch (error) {
            clearTimeout(timer);
            reject(error);
            return;
        }
        request.onupgradeneeded = event => {
            const db = request.result;
            wasCreated = !event || event.oldVersion === 0;
            if (!db.objectStoreNames.contains("sessions")) {
                db.createObjectStore("sessions", { keyPath: "id" });
            }
            if (!db.objectStoreNames.contains("documents")) {
                const docs = db.createObjectStore("documents", { keyPath: "id" });
                docs.createIndex("session", "session", { unique: false });
            }
        };
        request.onblocked = () => {
            if (!settled) {
                settled = true;
                clearTimeout(timer);
                reject(storageError("Browser recovery storage is blocked by another page."));
            }
        };
        request.onerror = () => {
            if (!settled) {
                settled = true;
                clearTimeout(timer);
                reject(request.error || storageError("Could not open browser recovery storage."));
            }
        };
        request.onsuccess = () => {
            if (settled) {
                request.result.close();
                return;
            }
            if (clock() >= deadline) {
                settled = true;
                clearTimeout(timer);
                request.result.close();
                reject(storageError("Opening browser recovery storage timed out. Crash recovery is unavailable."));
                return;
            }
            settled = true;
            clearTimeout(timer);
            const opened = request.result;
            database = opened;
            createdDatabase = wasCreated;
            databaseProblem = "";
            opened.onversionchange = () => {
                opened.close();
                if (database === opened) {
                    database = null;
                    databaseProblem = "Browser recovery storage changed or was removed. Save or download your work.";
                }
            };
            opened.onclose = () => {
                if (database === opened) {
                    database = null;
                    databaseProblem = "Browser recovery storage was closed or evicted. Save or download your work.";
                }
            };
            resolve(database);
        };
    }).finally(() => { opening = null; });
    return opening;
}

function transaction(stores, mode, deadline) {
    if (deadline !== undefined) ensureTime(deadline);
    if (!database) {
        throw storageError(databaseProblem || "Browser recovery storage is unavailable.");
    }
    let tx;
    if (mode === "readwrite") {
        // Strict durability asks the browser to flush the transaction before
        // acknowledging it. Older browsers may not understand this option.
        try {
            tx = database.transaction(stores, mode, { durability: "strict" });
            if (tx.durability !== "strict") storageStrictDurability = false;
            else if (storageStrictDurability === null) storageStrictDurability = true;
        } catch (error) {
            if (!(error instanceof TypeError)) throw error;
        }
        if (!tx) storageStrictDurability = false;
    }
    if (!tx) tx = database.transaction(stores, mode);
    return tx;
}

function completed(tx, failure, deadline) {
    return new Promise((resolve, reject) => {
        let settled = false;
        if (deadline !== undefined && remaining(deadline) === 0) {
            failure.value = storageError("Browser recovery startup timed out. Crash recovery is unavailable.");
            try { tx.abort(); } catch (_) { /* Already inactive. */ }
            reject(failure.value);
            return;
        }
        const timer = setTimeout(() => {
            if (settled) return;
            settled = true;
            try {
                tx.abort();
                failure.value = storageError("Browser recovery storage timed out; the transaction was aborted.");
            } catch (_) {
                // The transaction may have committed just before its delayed
                // completion event. Do not claim that it failed or succeeded.
                failure.value = unknownOutcome(
                    "The browser recovery transaction timed out after it may have committed. Verify its result before retrying.",
                );
            }
            reject(failure.value);
        }, deadline === undefined ? OPERATION_TIMEOUT_MS : remaining(deadline));
        tx.oncomplete = () => {
            if (settled) return;
            settled = true;
            clearTimeout(timer);
            if (deadline !== undefined && clock() >= deadline) {
                reject(storageError("Browser recovery startup timed out. Crash recovery is unavailable."));
                return;
            }
            resolve();
        };
        tx.onabort = () => {
            if (settled) return;
            settled = true;
            clearTimeout(timer);
            reject(failure.value || tx.error || storageError("Browser recovery transaction was aborted."));
        };
        // The abort event supplies the transaction's final failure.
        tx.onerror = () => {};
    });
}

function abort(tx, failure, message) {
    failure.value = storageError(message);
    try { tx.abort(); } catch (_) { /* Already aborted. */ }
}

function randomToken() {
    if (globalThis.crypto?.randomUUID) return crypto.randomUUID();
    if (!globalThis.crypto?.getRandomValues) {
        throw storageError("Secure browser session identifiers are unavailable.");
    }
    const bytes = crypto.getRandomValues(new Uint8Array(16));
    return Array.from(bytes, byte => byte.toString(16).padStart(2, "0")).join("");
}

function releaseLock(lock) {
    if (!lock || lock.releasing) return;
    lock.releasing = true;
    // Resolving the callback's promise does not synchronously release a Web
    // Lock. Wait for the request itself before this page tries to reacquire
    // the same name, or it can mistake its own closing lock for another tab.
    const completion = Promise.resolve(lock.releaseCompletion()).then(() => {}, () => {});
    const previous = releasingLocks.get(lock.id);
    const pending = previous ? Promise.all([previous, completion]).then(() => {}) : completion;
    releasingLocks.set(lock.id, pending);
    pending.then(() => {
        if (releasingLocks.get(lock.id) === pending) releasingLocks.delete(lock.id);
    });
    lock.release();
}

async function waitForLockRelease(id, deadline) {
    while (releasingLocks.has(id)) {
        ensureTime(deadline);
        const pending = releasingLocks.get(id);
        let timer;
        try {
            await Promise.race([
                pending,
                new Promise((_, reject) => {
                    timer = setTimeout(
                        () => reject(storageError("Releasing a browser recovery lock timed out.")),
                        remaining(deadline),
                    );
                }),
            ]);
        } finally {
            clearTimeout(timer);
        }
        ensureTime(deadline);
    }
}

async function tryLock(id, deadline = clock() + OPERATION_TIMEOUT_MS) {
    await waitForLockRelease(id, deadline);
    return new Promise((resolve, reject) => {
        try { ensureTime(deadline); } catch (error) { reject(error); return; }
        let answered = false;
        const timer = setTimeout(() => {
            if (!answered) {
                answered = true;
                reject(storageError("Acquiring a browser recovery lock timed out."));
            }
        }, remaining(deadline));
        try {
            let request;
            request = navigator.locks.request(
                LOCK_PREFIX + id,
                { mode: "exclusive", ifAvailable: true },
                lock => {
                    if (answered) return;
                    clearTimeout(timer);
                    if (clock() >= deadline) {
                        answered = true;
                        reject(storageError("Acquiring a browser recovery lock timed out."));
                        return;
                    }
                    if (!lock) {
                        answered = true;
                        resolve(null);
                        return;
                    }
                    return new Promise(release => {
                        answered = true;
                        resolve({ id, release, releaseCompletion: () => request, releasing: false });
                    });
                },
            );
            request.catch(error => {
                if (!answered) {
                    answered = true;
                    clearTimeout(timer);
                    reject(error);
                }
            });
        } catch (error) {
            clearTimeout(timer);
            reject(error);
        }
    });
}

async function tryStartupLock(id, deadline, generation) {
    ensureStartup(deadline, generation);
    const lock = await tryLock(id, deadline);
    try {
        ensureStartup(deadline, generation);
    } catch (error) {
        releaseLock(lock);
        throw error;
    }
    if (lock) startupLocks.add(lock);
    return lock;
}

function adoptStartupLock(state) {
    startupLocks.delete(state.lock);
    leases.set(state.id, state);
}

function unlock(state) {
    state.active = false;
    const lock = state.lock;
    state.lock = null;
    releaseLock(lock);
}

function requireLease(id, kind, expectedToken) {
    const state = leases.get(id);
    if (!state || (kind && state.kind !== kind)) {
        throw storageError("The browser recovery session is no longer available.");
    }
    if (expectedToken !== undefined && state.token !== expectedToken) {
        throw storageError("This browser recovery claim is stale. Retry recovery.");
    }
    if (state.lost) {
        throw storageError("Another tab claimed this recovery session. Save or download your work.");
    }
    return state;
}

function docId(session, key) {
    return `${session}/${key}`;
}

function validRecoveryKey(key) {
    return typeof key === "string" && /^[A-Za-z0-9_-]{1,180}$/.test(key);
}

function validManifest(manifest) {
    return Array.isArray(manifest) &&
        manifest.every(entry => entry && validRecoveryKey(entry.key) &&
            Number.isSafeInteger(entry.revision) && entry.revision > 0) &&
        new Set(manifest.map(entry => entry.key)).size === manifest.length;
}

function validDiscardProof(proof) {
    if (!proof || typeof proof !== "object" || Array.isArray(proof)) return false;
    const names = Object.getOwnPropertyNames(proof);
    return names.length === 3 && Object.keys(proof).length === 3
        && Object.getOwnPropertySymbols(proof).length === 0
        && names.every(name => ["version", "id", "digest"].includes(name))
        && proof.version === 1 && validRecoveryKey(proof.id)
        && typeof proof.digest === "string" && /^[0-9a-f]{64}$/.test(proof.digest);
}

function validRecord(record, session) {
    return record && validRecoveryKey(record.key) &&
        record.id === docId(session, record.key) && record.session === session &&
        Number.isSafeInteger(record.revision) && record.revision > 0 &&
        typeof record.deleted === "boolean" &&
        (record.deleted ? record.bytes === null : record.bytes instanceof ArrayBuffer) &&
        (!Object.hasOwn(record, "discardIntent") ||
            (!record.deleted && validDiscardProof(record.discardIntent))) &&
        (!Object.hasOwn(record, "discardReceipt") ||
            (record.deleted && validDiscardProof(record.discardReceipt)));
}

async function createSession(id, token, deadline) {
    const tx = transaction(["sessions"], "readwrite", deadline);
    const failure = { value: null };
    const done = completed(tx, failure, deadline);
    tx.objectStore("sessions").add({ id, token, created: Date.now(), manifest: [] });
    await done;
}

async function claimSession(id, token, deadline, remainingDocuments, remainingBytes, allowEmptyFence) {
    const tx = transaction(["sessions", "documents"], "readwrite", deadline);
    const failure = { value: null };
    const done = completed(tx, failure, deadline);
    const sessions = tx.objectStore("sessions");
    const request = sessions.get(id);
    let records = null;
    let manifest = [];
    let manifestValid = false;
    let incomplete = false;
    let scanned = 0;
    let bytesRead = 0;
    request.onsuccess = () => {
        const row = request.result;
        // A tab may have completed cleanup after the initial session listing.
        if (!row) return;
        if (Array.isArray(row.manifest)) {
            manifest = row.manifest;
            // Validate at most the configured number of entries synchronously.
            manifestValid = manifest.length <= MAX_DISCOVERY_DOCUMENTS_PER_SESSION &&
                validManifest(manifest);
        }
        records = [];
        const finishClaim = () => {
            const hasPreview = records.some(record => validRecord(record, id) && !record.deleted);
            const completeEmpty = allowEmptyFence && !incomplete && manifestValid && manifest.length === 0
                && records.every(record => validRecord(record, id) && record.deleted);
            // A claim returning a preview (or a fully checked empty session)
            // must fence late writers. An uninspectable session with no
            // preview stays gated and needs no metadata mutation at all.
            if (hasPreview || completeEmpty) {
                row.token = token;
                sessions.put(row);
            }
        };
        // A cursor lets the event loop run between records and stops before
        // collecting an unbounded number of byte arrays. IndexedDB itself may
        // still materialize one arbitrarily large legacy record before its
        // byteLength can be inspected.
        const read = tx.objectStore("documents").index("session").openCursor(IDBKeyRange.only(id));
        read.onsuccess = () => {
            const cursor = read.result;
            if (!cursor) {
                finishClaim();
                return;
            }
            if (scanned >= remainingDocuments || scanned >= MAX_DISCOVERY_DOCUMENTS_PER_SESSION) {
                incomplete = true;
                finishClaim();
                return;
            }
            scanned++;
            const record = cursor.value;
            const size = record?.bytes instanceof ArrayBuffer ? record.bytes.byteLength : 0;
            if (size > MAX_DISCOVERY_SNAPSHOT_BYTES || size > remainingBytes - bytesRead) {
                incomplete = true;
            } else {
                records.push(record);
                bytesRead += size;
            }
            cursor.continue();
        };
    };
    await done;
    return records ? { records, manifest, manifestValid, incomplete, scanned, bytesRead } : null;
}

// Session-index cursors alone cannot see a damaged document with a missing
// session field (or a document whose session row disappeared). Audit the
// primary store as well, or such an original could look like "no recovery".
// IndexedDB may still materialize one oversized record before exposing its
// length; the cursor stops as soon as the bounded audit cannot continue.
async function auditRecoveryRecords(deadline, generation, knownSessions) {
    const tx = transaction(["documents"], "readonly", deadline);
    const done = completed(tx, { value: null }, deadline);
    const request = tx.objectStore("documents").openCursor();
    let scanned = 0;
    let bytesRead = 0;
    let incomplete = false;
    let malformed = false;
    request.onsuccess = () => {
        const cursor = request.result;
        if (!cursor) return;
        if (scanned >= MAX_DISCOVERY_DOCUMENTS) {
            incomplete = true;
            return;
        }
        scanned++;
        const record = cursor.value;
        const size = record?.bytes instanceof ArrayBuffer ? record.bytes.byteLength : 0;
        if (size > MAX_DISCOVERY_SNAPSHOT_BYTES || size > MAX_DISCOVERY_BYTES - bytesRead) {
            incomplete = true;
            return;
        }
        bytesRead += size;
        if (!validRecoveryKey(record?.session) || !knownSessions.has(record.session)
            || !validRecord(record, record.session)) {
            malformed = true;
        }
        cursor.continue();
    };
    await done;
    ensureStartup(deadline, generation);
    return { incomplete, malformed };
}

async function checkSession(state, deadline) {
    const tx = transaction(["sessions"], "readonly", deadline);
    const failure = { value: null };
    const done = completed(tx, failure, deadline);
    const request = tx.objectStore("sessions").get(state.id);
    request.onsuccess = () => {
        if (!request.result || request.result.token !== state.token) {
            abort(tx, failure, "Another tab claimed this recovery session. Save or download your work.");
        }
    };
    await done;
}

// All byte mutations check the fencing token and revision within one
// read/write transaction. A failed transaction leaves the previous record.
async function commit(state, op) {
    const tx = transaction(["sessions", "documents"], "readwrite");
    op.transaction = tx;
    const failure = { value: null };
    const done = completed(tx, failure);
    const sessions = tx.objectStore("sessions");
    const documents = tx.objectStore("documents");
    const session = sessions.get(state.id);
    session.onsuccess = () => {
        if (!session.result || session.result.token !== state.token) {
            abort(tx, failure, "Recovery ownership was lost or storage was evicted. Save or download your work.");
            return;
        }
        const row = session.result;
        if (!validManifest(row.manifest)) {
            abort(tx, failure, "Recovery metadata is invalid or was evicted. Save or download your work.");
            return;
        }
        const id = docId(state.id, op.key);
        const current = documents.get(id);
        current.onsuccess = () => {
            const record = current.result;
            const listed = row.manifest.find(entry => entry.key === op.key);
            const validRecord = !record || (
                record.id === id && record.session === state.id && record.key === op.key &&
                Number.isSafeInteger(record.revision) && record.revision > 0 &&
                typeof record.deleted === "boolean" &&
                (record.deleted ? record.bytes === null : record.bytes instanceof ArrayBuffer)
            );
            if (!validRecord || (listed && (!record || record.deleted || listed.revision !== record.revision)) ||
                (record && !record.deleted && !listed)) {
                abort(tx, failure, "A committed recovery snapshot is missing or inconsistent. Save or download your work.");
                return;
            }
            if (record && record.revision > op.revision) {
                abort(tx, failure, "A newer recovery revision already exists.");
                return;
            }
            if (record && record.revision === op.revision) return;
            documents.put({
                id,
                session: state.id,
                key: op.key,
                revision: op.revision,
                deleted: op.kind === "delete",
                bytes: op.kind === "put" ? op.bytes : null,
            });
            row.manifest = row.manifest.filter(entry => entry.key !== op.key);
            if (op.kind === "put") row.manifest.push({ key: op.key, revision: op.revision });
            sessions.put(row);
        };
    };
    await done;
}

function report(state, error) {
    state.error = problem(error);
}

function pump(state) {
    if (!state.active || state.lost || state.closing || state.busy ||
        state.pending.size === 0) return;
    // Handoffs and explicit cleanup must get an answer even if an unrelated
    // retrying snapshot is blocked by a quota or permission error.
    const available = Array.from(state.pending.values())
        .filter(candidate => Date.now() >= candidate.retryAt);
    const op = available.find(candidate => !candidate.retry) || available[0];
    if (!op) return;
    state.busy = op;
    state.inflight = (async () => {
        let failure = null;
        try {
            await commit(state, op);
            const current = state.pending.get(op.key) === op;
            if (current) {
                state.pending.delete(op.key);
                const desired = state.desired.get(op.key);
                if (desired && desired.revision === op.revision) desired.done = true;
            } else {
                // A committed but superseded handoff is not the current copy.
                failure = storageError("The recovery operation was superseded.");
            }
            // A success must not hide another document's queued failure.
            if (!Array.from(state.pending.values()).some(candidate => candidate.lastError)) {
                state.error = "";
                state.reported = "";
            }
        } catch (error) {
            failure = error;
            const current = state.pending.get(op.key) === op;
            if (current && !op.retry) state.pending.delete(op.key);
            if (current) {
                report(state, error);
                op.lastError = state.error;
                op.failures = Math.min(op.failures + 1, 6);
                op.retryAt = Date.now() + Math.min(60_000, 1_000 * (2 ** op.failures));
            }
        } finally {
            // An awaited handoff must not report completion while this very
            // transaction still makes suspend/finish look pending.
            state.busy = null;
            state.inflight = null;
            pump(state);
        }
        if (failure) op.reject?.(failure);
        else op.resolve?.();
    })();
}

function enqueue(state, key, kind, bytes, retry, resolve, reject) {
    if (state.closing) throw storageError("The browser recovery session has finished.");
    if (state.lost) throw storageError("Recovery ownership was lost. Save or download your work.");
    if (state.revision >= Number.MAX_SAFE_INTEGER) {
        throw storageError("Browser recovery revision space was exhausted.");
    }
    const previous = state.pending.get(key);
    const superseded = state.busy?.key === key ? state.busy : null;
    if (previous && previous.reject && previous !== state.busy) {
        previous.reject(storageError("The recovery operation was superseded."));
    }
    const op = {
        key,
        kind,
        bytes,
        revision: ++state.revision,
        retry,
        resolve,
        reject,
        retryAt: 0,
        failures: 0,
        lastError: "",
    };
    state.pending.set(key, op);
    state.desired.set(key, { kind, revision: op.revision, done: false });
    // Cancel an older transaction as soon as a newer edit or deletion is
    // accepted. If the page is terminated before the replacement can commit,
    // the old in-flight write cannot appear after that edit/save.
    if (superseded?.transaction) {
        try { superseded.transaction.abort(); } catch (_) { /* Already committed. */ }
    }
    pump(state);
    return op;
}

function newLease(id, token, lock, kind) {
    return {
        id, token, lock, kind, active: true, lost: false, closing: false,
        revision: 0, pending: new Map(), desired: new Map(), busy: null,
        inflight: null, error: "", reported: "",
        retryPromise: null,
    };
}

// Returns persistence and eviction-detection status. A false persistence result
// is not a failed transaction, but must be shown as an eviction risk.
export async function recoveryInitialize(startupDeadline, attemptGeneration = startupGeneration) {
    const deadline = deadlineOrDefault(startupDeadline);
    const generation = attemptGeneration;
    ensureStartup(deadline, generation);
    // Checking existing permission may be slow; overlap it with database
    // opening and conservatively report "not persistent" if it times out.
    // Never request persistence automatically: the app's explicit settings
    // action owns consent for this origin-wide permission.
    const protection = (async () => {
        let timer;
        try {
            return await Promise.race([
                navigator.storage?.persisted?.().then(Boolean) ?? Promise.resolve(false),
                new Promise(resolve => {
                    timer = setTimeout(() => resolve(false), Math.min(3_000, remaining(deadline)));
                }),
            ]);
        } catch (_) {
            return false;
        } finally {
            clearTimeout(timer);
        }
    })();
    await openDatabase(deadline);
    ensureStartup(deadline, generation);
    let warning = "";
    try {
        const previous = localStorage.getItem(DB_MARKER);
        if (createdDatabase && previous === "1") {
            evictionDetected = true;
        }
        localStorage.setItem(DB_MARKER, "1");
    } catch (_) {
        warning = "Browser recovery cannot detect all storage eviction because localStorage is unavailable or full.";
    }
    createdDatabase = false;
    if (evictionDetected) {
        warning = [
            "Browser recovery storage was cleared or evicted; earlier snapshots may be missing.",
            warning,
        ].filter(Boolean).join(" ");
    }
    const persistent = await protection;
    ensureStartup(deadline, generation);
    storagePersistent = persistent;
    nextPersistenceCheck = Date.now() + 30_000;
    return JSON.stringify({ persistent, warning });
}

export async function recoveryCreate(id, startupDeadline, attemptGeneration = startupGeneration) {
    if (!validRecoveryKey(id) || id === LEGACY_LEASE_ID) {
        throw storageError("Invalid browser recovery session identifier.");
    }
    const deadline = deadlineOrDefault(startupDeadline);
    const generation = attemptGeneration;
    const lock = await tryStartupLock(id, deadline, generation);
    if (!lock) throw storageError("Could not claim the new browser recovery session.");
    try {
        const token = randomToken();
        await createSession(id, token, deadline);
        ensureStartup(deadline, generation);
        adoptStartupLock(newLease(id, token, lock, "writer"));
        return token;
    } catch (error) {
        startupLocks.delete(lock);
        releaseLock(lock);
        throw error;
    }
}

// Legacy localStorage locks alone are a non-atomic read/write protocol. A
// separate Web Lock prevents two current-version tabs from reviewing the same
// old snapshots at once. The non-atomic legacy heartbeat cannot exclude old
// tabs reliably, so it must never authorize deletion of their snapshots.
export async function recoveryClaimLegacy(startupDeadline, attemptGeneration = startupGeneration) {
    const deadline = deadlineOrDefault(startupDeadline);
    const generation = attemptGeneration;
    ensureStartup(deadline, generation);
    if (!globalThis.navigator?.locks?.request) {
        throw storageError("Browser Web Locks are unavailable. Previous recovery snapshots cannot be claimed safely.");
    }
    if (leases.has(LEGACY_LEASE_ID)) return false;
    const lock = await tryStartupLock(LEGACY_LEASE_ID, deadline, generation);
    if (!lock) return false;
    try {
        const token = randomToken();
        adoptStartupLock(newLease(LEGACY_LEASE_ID, token, lock, "legacy"));
        return token;
    } catch (error) {
        startupLocks.delete(lock);
        releaseLock(lock);
        throw error;
    }
}

// A bounded audit must report an incomplete result rather than trust a saved
// cursor: same-origin writers can change records without advancing a checkpoint.
export async function recoveryDiscover(startupDeadline, attemptGeneration = startupGeneration) {
    const deadline = deadlineOrDefault(startupDeadline);
    const generation = attemptGeneration;
    ensureStartup(deadline, generation);
    const tx = transaction(["sessions"], "readonly", deadline);
    const failure = { value: null };
    const done = completed(tx, failure, deadline);
    const request = tx.objectStore("sessions").getAllKeys(undefined, MAX_DISCOVERY_SESSIONS + 1);
    let sessions = [];
    request.onsuccess = () => { sessions = request.result; };
    await done;
    ensureStartup(deadline, generation);
    const warnings = new Set();
    if (sessions.length > MAX_DISCOVERY_SESSIONS) {
        warnings.add("Browser recovery discovery reached its safe preview limit. Some snapshots remain uninspected and were retained.");
        sessions = sessions.slice(0, MAX_DISCOVERY_SESSIONS);
    }
    const audit = await auditRecoveryRecords(deadline, generation, new Set(sessions));
    if (audit.incomplete) {
        warnings.add("Browser recovery discovery reached its safe preview limit. Some snapshots remain uninspected and were retained.");
    }
    if (audit.malformed) {
        warnings.add("Some browser recovery records are unindexed, malformed, or missing their session. The originals were retained for rescue.");
    }
    return claimListedSessions(sessions, deadline, generation, warnings, audit);
}

// Each claimed session changes its fencing token before returning bytes, so
// an old page cannot subsequently mutate that source.
async function claimListedSessions(sessions, deadline, generation, warnings, audit) {
    const result = [];
    let scanned = 0;
    let bytesRead = 0;
    const claimed = [];
    try {
        for (const id of sessions) {
            ensureStartup(deadline, generation);
            if (!validRecoveryKey(id) || id === LEGACY_LEASE_ID) {
                warnings.add("Some browser recovery metadata is invalid. The original data was retained.");
                continue;
            }
            const existing = leases.get(id);
            if (existing) {
                if (existing.kind !== "writer" || !existing.active || !existing.lock || existing.lost) {
                    // A previous attempt can still hold an orphan while its
                    // stale asynchronous result is being dropped. Skipping it
                    // as if discovery were complete would un-gate editing
                    // without returning that snapshot to the application.
                    warnings.add("Some browser recovery claims are still owned by an earlier attempt. Retry after ownership is resolved.");
                }
                continue;
            }
            if (scanned >= MAX_DISCOVERY_DOCUMENTS) {
                warnings.add("Browser recovery discovery reached its safe preview limit. Some snapshots remain uninspected and were retained.");
                break;
            }
            const lock = await tryStartupLock(id, deadline, generation);
            if (!lock) {
                warnings.add("Some browser recovery snapshots are in use in another tab. Close that tab and retry; the originals were retained.");
                continue;
            }
            let token;
            let claimedSession;
            try {
                token = randomToken();
                claimedSession = await claimSession(
                    id, token, deadline,
                    MAX_DISCOVERY_DOCUMENTS - scanned,
                    MAX_DISCOVERY_BYTES - bytesRead,
                    !audit.incomplete && !audit.malformed,
                );
                ensureStartup(deadline, generation);
            } catch (error) {
                startupLocks.delete(lock);
                releaseLock(lock);
                throw error;
            }
            if (!claimedSession) {
                startupLocks.delete(lock);
                releaseLock(lock);
                continue;
            }
            const state = newLease(id, token, lock, "orphan");
            adoptStartupLock(state);
            claimed.push([id, token]);
            const { records, manifest, manifestValid, incomplete } = claimedSession;
            scanned += claimedSession.scanned;
            bytesRead += claimedSession.bytesRead;
            if (incomplete) {
                warnings.add("Browser recovery discovery reached its safe preview limit. Some snapshots remain uninspected and were retained.");
            }
            const docs = records.filter(record => validRecord(record, id) && !record.deleted).map(record => ({
                key: record.key,
                revision: record.revision,
                bytes: new Uint8Array(record.bytes),
                pendingDiscard: Object.hasOwn(record, "discardIntent"),
            }));
            const malformed = records.some(record => !validRecord(record, id));
            const missing = incomplete || !manifestValid || malformed || manifest.some(entry =>
                !records.some(record => validRecord(record, id) && record.key === entry.key &&
                    !record.deleted && record.revision === entry.revision)) ||
                records.some(record => validRecord(record, id) && !record.deleted &&
                    !manifest.some(entry => entry.key === record.key && entry.revision === record.revision));
            if (missing) {
                warnings.add("Some browser recovery snapshots are missing, malformed, or were evicted. Remaining snapshots were retained.");
            }
            if (docs.length === 0) {
                // Discovery and retry never delete even empty/tombstoned or
                // damaged records. Cleanup needs a separate safe policy.
                recoveryRelease(id, token);
            } else {
                result.push({ id, lease: token, docs });
            }
        }
        ensureStartup(deadline, generation);
        return { sessions: result, warning: Array.from(warnings).join(" ") };
    } catch (error) {
        for (const [id, token] of claimed) recoveryRelease(id, token);
        throw error;
    }
}

// Copy immediately: the wasm view may be freed or its memory may grow before
// IndexedDB starts its transaction.
export function recoveryQueue(id, key, bytes) {
    const state = requireLease(id, "writer");
    if (!validRecoveryKey(key)) throw storageError("Invalid browser recovery document key.");
    return enqueue(state, key, "put", Uint8Array.from(bytes).buffer, true, null, null).revision;
}

// A handoff is not retried after failure: the caller keeps the original orphan.
export function recoveryPersist(id, key, bytes) {
    const state = requireLease(id, "writer");
    if (!validRecoveryKey(key)) throw storageError("Invalid browser recovery document key.");
    const copy = Uint8Array.from(bytes).buffer;
    return new Promise((resolve, reject) => {
        try { enqueue(state, key, "put", copy, false, resolve, reject); }
        catch (error) { reject(error); }
    });
}

export function recoveryRemove(id, key) {
    const state = requireLease(id, "writer");
    if (!validRecoveryKey(key)) throw storageError("Invalid browser recovery document key.");
    const desired = state.desired.get(key);
    if (desired?.kind === "delete") return desired.done ? "committed" : "pending";
    enqueue(state, key, "delete", null, true, null, null);
    return "pending";
}

export function recoveryStatus(id) {
    const state = leases.get(id);
    if (!state || state.kind !== "writer") {
        throw storageError("The browser recovery session is no longer available.");
    }
    if (databaseProblem && !state.error) state.error = databaseProblem;
    refreshPersistenceStatus();
    pump(state);
    return JSON.stringify({
        active: state.active,
        pending: state.pending.size > 0 || !!state.busy,
        error: state.error,
        persistent: storagePersistent,
        strict_durability: storageStrictDurability === true,
    });
}

// beforeunload can fire between UI frames. Query the actual writer queue
// instead of relying only on the app's last rendered dirty-state snapshot.
export function recoveryHasUnconfirmedWork() {
    for (const state of leases.values()) {
        if (state.kind === "writer" &&
            (databaseProblem || state.error || !state.active || state.pending.size > 0 || state.busy)) {
            return true;
        }
    }
    return false;
}

// A clean save may have superseded a formerly queued snapshot with a
// tombstone. If undo makes that old document watermark dirty again, callers
// need to queue it anew instead of waiting forever on an obsolete revision.
export function recoveryCurrent(id, key, revision) {
    const state = leases.get(id);
    const desired = state?.desired.get(key);
    return !!state && state.kind === "writer" && state.active && !state.lost &&
        !databaseProblem && desired?.kind === "put" &&
        desired.revision === revision;
}

// A watermark alone is not proof of persistence. Acknowledge only the exact
// latest revision after its own transaction completed under this live writer.
export function recoveryAcknowledged(id, key, revision) {
    return recoveryCurrent(id, key, revision) &&
        leases.get(id).desired.get(key).done === true;
}

export function recoveryTakeError(id) {
    const state = leases.get(id);
    if (!state || state.kind !== "writer") {
        throw storageError("The browser recovery session is no longer available.");
    }
    if (databaseProblem && !state.error) state.error = databaseProblem;
    if (!state.error || state.reported === state.error) return "";
    state.reported = state.error;
    return state.error;
}

export function recoveryFinish(id, expectedToken) {
    const state = requireLease(id, "writer", expectedToken);
    if (!state.active) throw storageError("The browser recovery session is suspended.");
    if (databaseProblem && !state.error) state.error = databaseProblem;
    pump(state);
    if (state.error) throw storageError(state.error);
    if (state.pending.size > 0 || state.busy) return "pending";
    // Releasing ownership must not discard committed snapshots: a handoff or
    // interrupted UI shutdown may still need them. Discovery also preserves
    // empty/tombstoned sessions. Pending writes and tombstones are never cancelled.
    state.closing = true;
    unlock(state);
    leases.delete(id);
    return "finished";
}

export function recoveryHeld(id, expectedToken) {
    const state = leases.get(id);
    return !!state && (expectedToken === undefined || state.token === expectedToken) &&
        state.active && !!state.lock && !state.lost;
}

export function recoveryRelease(id, expectedToken) {
    const state = leases.get(id);
    if (!state || (expectedToken !== undefined && state.token !== expectedToken)) return;
    state.closing = true;
    if (state.kind === "writer") {
        // A dropped writer must not keep an accepted mutation running after
        // its lock is handed to another page. Preserve the last committed
        // record; callers waiting for a dropped queued operation get an error.
        const busy = state.busy;
        if (busy?.transaction) {
            try {
                busy.transaction.abort();
            } catch (_) {
                // It may already have committed, even if its completion
                // callback has not run. Do not report a definite outcome.
                busy.reject?.(unknownOutcome(
                    "The recovery writer was released after its transaction may have committed.",
                ));
            }
        }
        for (const op of state.pending.values()) {
            if (op !== busy) op.reject?.(storageError("The recovery writer was released before the operation committed."));
        }
        state.pending.clear();
    }
    unlock(state);
    leases.delete(id);
}

// Invalidate an unfinished startup before pagehide. A lock already obtained
// for an in-flight startup transaction is released; token checks fence that
// transaction if another tab claims the session. A pending live writer keeps
// its leases until it can flush or the browser actually terminates the page.
export function recoveryPauseStartup(keepLiveWriter = false) {
    startupHidden = true;
    startupGeneration++;
    for (const lock of startupLocks) releaseLock(lock);
    startupLocks.clear();
    for (const state of leases.values()) {
        if (state.kind !== "writer") continue;
        // A dirty page may have edits that have not reached the snapshot
        // scheduler yet. A BFCache page must not expose that live session as
        // an orphan just because no IndexedDB write is currently pending.
        if (keepLiveWriter && state.active && state.lock && !state.lost) {
            return "pending";
        }
        if (databaseProblem && !state.error) state.error = databaseProblem;
        pump(state);
        if (state.error || state.pending.size > 0 || state.busy) {
            return "pending";
        }
    }
    recoverySuspendAll();
    return "suspended";
}

// Pagehide cannot await IndexedDB. Release synchronously; already committed
// snapshots remain, and transactional fencing rejects a late stale mutation.
export function recoverySuspendAll() {
    for (const state of leases.values()) unlock(state);
}

// Do not expose a stale snapshot as an orphan while this live page still has
// failed or uncommitted changes. A terminated page releases its locks itself.
export function recoverySuspend(id) {
    const state = leases.get(id);
    if (!state || state.kind !== "writer") {
        throw storageError("The browser recovery session is no longer available.");
    }
    if (databaseProblem && !state.error) state.error = databaseProblem;
    pump(state);
    if (state.error) throw storageError(state.error);
    if (!state.active) return "suspended";
    if (state.pending.size > 0 || state.busy) return "pending";
    recoverySuspendAll();
    return "suspended";
}

// Concurrent pageshow/retry callbacks share one attempt per generation.
// Otherwise one attempt can mistake this page's other in-flight lock for a
// conflicting tab and permanently mark its own writer lost.
export function recoveryResumeAll() {
    startupHidden = false;
    const generation = startupGeneration;
    if (resuming?.generation === generation) return resuming.promise;
    const promise = resumeGeneration(generation);
    const tracked = promise.finally(() => {
        if (resuming?.promise === tracked) resuming = null;
    });
    resuming = { generation, promise: tracked };
    return tracked;
}

// A BFCache page may resume only if nobody claimed its old session while it
// was suspended. Never apply its cached bytes after another owner intervened.
async function resumeGeneration(generation) {
    const deadline = clock() + STARTUP_TIMEOUT_MS;
    for (const state of leases.values()) {
        if (startupHidden || generation !== startupGeneration) return;
        if (state.active || state.lost) continue;
        let lock;
        try {
            lock = await tryStartupLock(state.id, deadline, generation);
            if (!lock) throw storageError("Another tab claimed this recovery session. Save or download your work.");
            // The owner may have dropped the lease while the lock request was
            // pending. Do not retain an unreferenced lock in that case.
            if (leases.get(state.id) !== state) {
                startupLocks.delete(lock);
                releaseLock(lock);
                continue;
            }
            state.lock = lock;
            if (state.kind !== "legacy") {
                // A BFCache page can return after the database connection was
                // closed. Reopen before checking its token; an absent session
                // still fails closed and is never silently recreated.
                if (!database) await openDatabase(deadline);
                ensureStartup(deadline, generation);
                await checkSession(state, deadline);
            }
            if (startupHidden || generation !== startupGeneration || leases.get(state.id) !== state) {
                startupLocks.delete(lock);
                releaseLock(lock);
                if (state.lock === lock) state.lock = null;
                continue;
            }
            startupLocks.delete(lock);
            state.active = true;
            state.error = "";
            state.reported = "";
            for (const op of state.pending.values()) op.retryAt = 0;
            pump(state);
        } catch (error) {
            if (lock) {
                startupLocks.delete(lock);
                releaseLock(lock);
            }
            if (state.lock === lock) state.lock = null;
            // A second pagehide cancels this resume rather than permanently
            // marking the still-fenced session lost. A later pageshow can retry.
            if (startupHidden || generation !== startupGeneration || leases.get(state.id) !== state) {
                continue;
            }
            // A timeout or temporarily denied API does not prove that another
            // tab claimed the session; leave it retryable. A token mismatch
            // or unavailable lock does prove that this owner must be fenced.
            state.lost = /another tab claimed/i.test(problem(error));
            report(state, error);
        }
    }
}

// Retry a temporarily blocked writer without replacing its session identity.
// If no snapshot is queued, a token-checked write probes whether the store has
// become writable; it does not claim that a large future write will fit.
export async function recoveryRetryWriter(id) {
    const state = requireLease(id, "writer");
    if (state.retryPromise) return state.retryPromise;
    state.retryPromise = (async () => {
        const generation = startupGeneration;
        const deadline = clock() + OPERATION_TIMEOUT_MS;
        try {
            if (startupHidden) throw storageError("Recovery cannot retry while this page is hidden.");
            if (!state.active) await recoveryResumeAll();
            if (!state.active || state.lost) {
                throw storageError(state.error || "Browser recovery could not resume.");
            }
            if (!database) await openDatabase(deadline);
            ensureStartup(deadline, generation);
            for (const op of state.pending.values()) op.retryAt = 0;
            if (state.pending.size > 0 || state.busy) {
                pump(state);
                return;
            }
            const tx = transaction(["sessions"], "readwrite", deadline);
            const failure = { value: null };
            const done = completed(tx, failure, deadline);
            const sessions = tx.objectStore("sessions");
            const request = sessions.get(id);
            request.onsuccess = () => {
                if (!request.result || request.result.token !== state.token) {
                    abort(tx, failure, "Recovery ownership was lost or storage was evicted.");
                } else {
                    sessions.put(request.result);
                }
            };
            await done;
            ensureStartup(deadline, generation);
            if (!state.active) throw storageError("Browser recovery was suspended during retry.");
            state.error = "";
            state.reported = "";
        } catch (error) {
            report(state, error);
            throw error;
        }
    })().finally(() => { state.retryPromise = null; });
    return state.retryPromise;
}

function copyExpectedSnapshot(bytes) {
    if (!ArrayBuffer.isView(bytes)) {
        throw storageError("The cached recovery snapshot is required to verify this operation.");
    }
    return new Uint8Array(bytes.buffer, bytes.byteOffset, bytes.byteLength).slice();
}

function matchesExpectedSnapshot(record, expected) {
    if (!(record?.bytes instanceof ArrayBuffer) || record.bytes.byteLength !== expected.byteLength) {
        return false;
    }
    const actual = new Uint8Array(record.bytes);
    for (let index = 0; index < actual.length; index++) {
        if (actual[index] !== expected[index]) return false;
    }
    return true;
}

async function snapshotDigest(expected) {
    if (!globalThis.crypto?.subtle?.digest) {
        throw storageError("Browser recovery cannot verify this snapshot. No recovery data was discarded.");
    }
    const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", expected));
    return Array.from(digest, byte => byte.toString(16).padStart(2, "0")).join("");
}

export async function recoveryDiscard(id, key, revision, expectedToken, expectedBytes) {
    const state = requireLease(id, "orphan", expectedToken);
    if (!state.active) throw storageError("The recovered session is not currently owned by this tab.");
    if (!validRecoveryKey(key) || !Number.isSafeInteger(revision) || revision <= 0) {
        throw storageError("The recovery snapshot identity is invalid; no data was discarded.");
    }
    const expected = copyExpectedSnapshot(expectedBytes);
    const digest = await snapshotDigest(expected);
    if (!state.active || state.lost || leases.get(id) !== state) {
        throw storageError("The recovery claim changed before the discard request.");
    }
    const intent = { version: 1, id: randomToken(), digest };
    const tx = transaction(["sessions", "documents"], "readwrite");
    const failure = { value: null };
    const done = completed(tx, failure);
    const sessions = tx.objectStore("sessions");
    const docs = tx.objectStore("documents");
    const session = sessions.get(id);
    session.onsuccess = () => {
        if (!session.result || session.result.token !== state.token) {
            abort(tx, failure, "Another tab claimed this recovery session.");
            return;
        }
        const row = session.result;
        if (!Array.isArray(row.manifest) ||
            row.manifest.length > MAX_DISCOVERY_DOCUMENTS_PER_SESSION ||
            !validManifest(row.manifest) ||
            !row.manifest.some(entry => entry.key === key && entry.revision === revision)) {
            abort(tx, failure, "Recovery metadata is invalid or inconsistent; no recovery data was discarded.");
            return;
        }
        const doc = docs.get(docId(id, key));
        doc.onsuccess = () => {
            if (!validRecord(doc.result, id) || doc.result.deleted) {
                abort(tx, failure, "The recovery snapshot is missing or was evicted; no recovery data was discarded.");
                return;
            }
            if (doc.result.revision !== revision || !matchesExpectedSnapshot(doc.result, expected)) {
                abort(tx, failure, "The recovery snapshot changed in another tab.");
                return;
            }
            if (Object.hasOwn(doc.result, "discardIntent")) {
                failure.value = unknownOutcome("This discard is already pending. Verify it before retrying.");
                tx.abort();
                return;
            }
            // First record only the user's intent. The original bytes and
            // manifest remain intact, including after a lost completion event
            // or reload. A separate explicit verification completes deletion.
            docs.put({ ...doc.result, discardIntent: intent });
        };
    };
    await done;
}

async function inspectDiscard(id, key, revision, expectedToken, expectedBytes, action) {
    const state = requireLease(id, "orphan", expectedToken);
    if (!state.active) throw storageError("The recovered session is not currently owned by this tab.");
    if (!validRecoveryKey(key) || !Number.isSafeInteger(revision) || revision <= 0) {
        throw storageError("The recovery snapshot identity is invalid; its state is unknown.");
    }
    const expected = copyExpectedSnapshot(expectedBytes);
    const digest = await snapshotDigest(expected);
    if (!state.active || state.lost || leases.get(id) !== state) {
        throw storageError("The recovery claim changed before the discard could be verified.");
    }
    const tx = transaction(
        ["sessions", "documents"], action !== "inspect" ? "readwrite" : "readonly",
    );
    const failure = { value: null };
    const done = completed(tx, failure);
    const session = tx.objectStore("sessions").get(id);
    let result = null;
    session.onsuccess = () => {
        if (!session.result || session.result.token !== state.token) {
            abort(tx, failure, "Another tab claimed this recovery session.");
            return;
        }
        const row = session.result;
        if (!Array.isArray(row.manifest)
            || row.manifest.length > MAX_DISCOVERY_DOCUMENTS_PER_SESSION
            || !validManifest(row.manifest)) {
            failure.value = unknownOutcome("Recovery metadata cannot verify the discard. The cached copy was retained.");
            tx.abort();
            return;
        }
        const listed = row.manifest.find(entry => entry.key === key);
        const doc = tx.objectStore("documents").get(docId(id, key));
        doc.onsuccess = () => {
            const record = doc.result;
            if (validRecord(record, id) && record.deleted && record.revision === revision
                && validDiscardProof(record.discardReceipt)
                && record.discardReceipt.digest === digest && !listed) {
                if (action === "cancel") {
                    if (row.manifest.length >= MAX_DISCOVERY_DOCUMENTS_PER_SESSION) {
                        failure.value = unknownOutcome(
                            "There is no room to retain this cached copy in the recovery manifest.",
                        );
                        tx.abort();
                        return;
                    }
                    // The user chose to keep a cached copy after a lost
                    // completion event. Restore only bytes proven to match
                    // this receipt, under the same fenced claim.
                    const { discardReceipt, ...original } = record;
                    tx.objectStore("documents").put({
                        ...original,
                        deleted: false,
                        bytes: expected.buffer,
                    });
                    row.manifest.push({ key, revision });
                    tx.objectStore("sessions").put(row);
                    result = "present";
                } else {
                    result = "absent";
                }
            } else if (validRecord(record, id) && !record.deleted && record.revision === revision
                && listed?.revision === revision && matchesExpectedSnapshot(record, expected)) {
                if (!Object.hasOwn(record, "discardIntent")) {
                    result = "present";
                } else if (record.discardIntent.digest !== digest) {
                    failure.value = unknownOutcome(
                        "The pending discard does not match the cached copy. The original was retained.",
                    );
                    tx.abort();
                } else if (action === "inspect") {
                    result = "pending";
                } else if (action === "cancel") {
                    const { discardIntent, ...original } = record;
                    tx.objectStore("documents").put(original);
                    result = "present";
                } else {
                    // This is invoked only by the explicit "verify and
                    // complete" action. Keep a content-bound receipt so an
                    // interrupted completion can be verified without mistaking
                    // eviction, a different snapshot, or an ordinary writer
                    // tombstone for the user's discard.
                    const { discardIntent, ...original } = record;
                    tx.objectStore("documents").put({
                        ...original,
                        deleted: true,
                        bytes: null,
                        discardReceipt: discardIntent,
                    });
                    row.manifest = row.manifest.filter(entry => entry.key !== key);
                    tx.objectStore("sessions").put(row);
                    result = "absent";
                }
            } else {
                failure.value = unknownOutcome(
                    "The discard cannot be verified from the current records. The cached copy was retained.",
                );
                tx.abort();
            }
        };
    };
    await done;
    if (!result) throw unknownOutcome("The recovery snapshot could not be verified.");
    return result;
}

// Inspection is read-only. A pending intent still has all original bytes.
export function recoveryInspect(id, key, revision, expectedToken, expectedBytes) {
    return inspectDiscard(id, key, revision, expectedToken, expectedBytes, "inspect");
}

// Called only after the user explicitly chooses to verify and complete the
// pending discard; it is idempotent when its own completion event was lost.
export function recoveryCompleteDiscard(id, key, revision, expectedToken, expectedBytes) {
    return inspectDiscard(id, key, revision, expectedToken, expectedBytes, "complete");
}

// Keeping the original clears a checked intent, or explicitly restores the
// cached bytes when a matching receipt proves that completion already ran.
// A missing record or an unrelated tombstone is never recreated.
export function recoveryCancelDiscard(id, key, revision, expectedToken, expectedBytes) {
    return inspectDiscard(id, key, revision, expectedToken, expectedBytes, "cancel");
}
