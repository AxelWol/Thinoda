// Origin-private document storage. IndexedDB is the atomic revision/pointer
// coordinator; immutable OPFS blobs are preferred, with IndexedDB byte storage
// when OPFS cannot be initialized. Never fall back after a committed read fails.
//
// Schema v2: `documents` holds metadata only ({id, size, revision, backend,
// blob}) with a `blob` index, so listing never loads document bytes. IndexedDB
// bytes live in `contents` keyed by an immutable content key. A blob (OPFS file
// or contents row) is shared by `.bak`, and is reclaimed once no record names it.
// Blob names are never reused, so an unreferenced blob stays unreferenced.
const DB_NAME = "thinoda.documents.v1";
const DB_VERSION = 2;
const STORE = "documents";
const CONTENTS = "contents";
const BLOB_INDEX = "blob";
const OPFS_DIR = "thinoda-documents-v1";
const PREFIX = "thinoda.vfs:";
let ready;
const handles = new Map();
// Other tabs learn about committed document IDs (metadata only, no bytes).
const channel = globalThis.BroadcastChannel ? new BroadcastChannel("thinoda.documents") : null;

function announce(changes) {
    try { channel?.postMessage(changes); } catch { /* Other tabs refresh on reads. */ }
}

// `callback(id, present)` for commits made by other tabs of this origin.
export function watchDocuments(callback) {
    if (!channel) return;
    channel.onmessage = event => {
        if (!Array.isArray(event.data)) return;
        for (const change of event.data) {
            if (Array.isArray(change) && typeof change[0] === "string" && typeof change[1] === "boolean") {
                callback(change[0], change[1]);
            }
        }
    };
}

function failure(code, message, actual = null, reason = null) {
    return Object.assign(new Error(message), { code, actual, reason });
}

function check(cancelled) {
    if (cancelled?.()) throw failure("cancelled", "Storage operation cancelled");
}

function validateLimit(limit) {
    if (limit !== null && (!Number.isSafeInteger(limit) || limit < 0)) {
        throw failure("io", "Invalid document read limit");
    }
}

function checkSize(size, limit) {
    if (limit === null) return;
    if (!Number.isSafeInteger(size) || size < 0) {
        throw failure("io", "Invalid document size");
    }
    if (size > limit) throw failure("too_large", "File exceeds its read size limit");
}

async function fileBytes(file, limit) {
    checkSize(file.size, limit);
    const bytes = new Uint8Array(await file.arrayBuffer());
    checkSize(bytes.length, limit);
    if (limit !== null && bytes.length !== file.size) {
        throw failure("io", "File changed during read");
    }
    return bytes;
}

function validDocumentId(id, directory = false) {
    return typeof id === "string" &&
        (id.startsWith("/Documents/") || (directory && id === "/Documents")) &&
        !id.includes("\\") && !id.includes("\0") &&
        !id.split("/").slice(1).some(p => !p || p === "." || p === "..");
}

function documentId(id, directory = false) {
    if (!validDocumentId(id, directory)) {
        throw failure("io", "Invalid browser document identifier");
    }
    return id;
}

// Web Locks coordinate tabs: exclusive per-document writes, exclusive blob
// creation/deletion and shared blob reads. Without Web Locks, deletion is still
// reference-safe and readers retry against the current record.
function withLock(name, mode, action, ifAvailable = false) {
    if (!globalThis.navigator?.locks?.request) return action(true);
    return navigator.locks.request(name, ifAvailable ? { mode, ifAvailable } : { mode },
        lock => action(lock !== null));
}

const blobLock = blob => `thinoda.documents.blob:${blob}`;

function upgrade(request, oldVersion) {
    const db = request.result;
    const tx = request.transaction;
    const documents = oldVersion < 1
        ? db.createObjectStore(STORE, { keyPath: "id" })
        : tx.objectStore(STORE);
    const contents = db.createObjectStore(CONTENTS, { keyPath: "blob" });
    documents.createIndex(BLOB_INDEX, BLOB_INDEX, { unique: false });
    if (oldVersion < 1) return;
    // v1 stored IndexedDB-backend bytes inline. Move them into `contents` in
    // the same versionchange transaction; failure aborts and keeps v1 intact.
    documents.openCursor().onsuccess = event => {
        const cursor = event.target.result;
        if (!cursor) return;
        const record = cursor.value;
        if (record.backend === "indexeddb" && record.bytes !== undefined) {
            const blob = `bytes-${crypto.randomUUID()}`;
            contents.put({ blob, bytes: record.bytes });
            const { bytes: _, ...metadata } = record;
            cursor.update({ ...metadata, blob });
        }
        cursor.continue();
    };
}

function openDatabase() {
    return new Promise((resolve, reject) => {
        if (!globalThis.indexedDB) {
            reject(failure("unsupported", "IndexedDB is unavailable", null, "indexeddb_unavailable"));
            return;
        }
        const request = indexedDB.open(DB_NAME, DB_VERSION);
        let failed = false;
        request.onupgradeneeded = event => {
            try {
                upgrade(request, event.oldVersion);
            } catch (error) {
                failed = true;
                request.transaction?.abort();
                reject(error);
            }
        };
        request.onerror = () => reject(request.error);
        request.onblocked = () => {
            failed = true;
            reject(failure("io", "Another tab is blocking document storage initialization",
                null, "blocked"));
        };
        request.onsuccess = () => {
            if (failed) {
                request.result.close();
                return;
            }
            const db = request.result;
            db.onversionchange = () => {
                db.close();
                ready = undefined;
            };
            resolve(db);
        };
    });
}

async function opfsDirectory(create) {
    const root = await navigator.storage.getDirectory();
    return root.getDirectoryHandle(OPFS_DIR, { create });
}

async function initialize() {
    const db = await openDatabase();
    let opfs = null;
    if (globalThis.navigator?.storage?.getDirectory) {
        try {
            opfs = await opfsDirectory(true);
            // Check actual write/close support, not merely API presence.
            const name = `probe-${crypto.randomUUID()}`;
            const file = await opfs.getFileHandle(name, { create: true });
            const writer = await file.createWritable();
            await writer.close();
            await opfs.removeEntry(name);
        } catch {
            opfs = null;
        }
    }
    return { db, opfs };
}

function storage() {
    if (!ready) {
        ready = initialize().catch(error => {
            ready = undefined;
            throw error;
        });
    }
    return ready;
}

function transaction(db, stores, mode, body) {
    return new Promise((resolve, reject) => {
        let tx;
        if (mode === "readwrite") {
            try {
                tx = db.transaction(stores, mode, { durability: "strict" });
            } catch (error) {
                // Old browsers lack the options overload. Standard transactions
                // still provide atomicity; power-loss durability is browser-defined.
                if (!(error instanceof TypeError)) throw error;
                tx = db.transaction(stores, mode);
            }
        } else {
            tx = db.transaction(stores, mode);
        }
        const state = { reason: undefined, result: undefined };
        tx.oncomplete = () => resolve(state.result);
        tx.onabort = () => reject(state.reason || tx.error || failure("io", "Document transaction aborted"));
        tx.onerror = () => {}; // onabort owns the rejection
        const fail = error => {
            state.reason = error;
            try { tx.abort(); } catch { /* Already finished. */ }
        };
        try {
            body(tx, state, fail);
        } catch (error) {
            fail(error);
        }
    });
}

function query(db, key) {
    return transaction(db, STORE, "readonly", (tx, state) => {
        const request = tx.objectStore(STORE).get(key);
        request.onsuccess = () => { state.result = request.result; };
    });
}

function references(db, blob) {
    return transaction(db, STORE, "readonly", (tx, state) => {
        const request = tx.objectStore(STORE).index(BLOB_INDEX).count(blob);
        request.onsuccess = () => { state.result = request.result; };
    });
}

function equal(a, b) {
    return a.length === b.length && a.every((byte, index) => byte === b[index]);
}

async function revision(bytes) {
    if (!globalThis.crypto?.subtle) {
        throw failure("unsupported", "Document storage requires a secure context");
    }
    const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
    return "sha256:" + Array.from(digest, b => b.toString(16).padStart(2, "0")).join("");
}

async function verify(record, bytes) {
    if (bytes.length !== record.size || await revision(bytes) !== record.revision) {
        throw failure("io", "Durable document verification failed", null, "invalid_durable");
    }
    return bytes;
}

// Reads the current record and its bytes. IndexedDB bytes are read in the same
// transaction as the record. An OPFS blob is read under a shared blob lock; if
// it was reclaimed after the record lookup, the record was superseded, so the
// read retries against the newer committed record.
async function readCurrent(state, id, limit = null) {
    for (let attempt = 0; attempt < 8; attempt++) {
        const found = await transaction(state.db, [STORE, CONTENTS], "readonly", (tx, out) => {
            const request = tx.objectStore(STORE).get(id);
            request.onsuccess = () => {
                const record = request.result;
                out.result = { record };
                if (record && limit !== null) {
                    if (!Number.isSafeInteger(record.size) || record.size < 0) {
                        out.result.invalidSize = true;
                        return;
                    }
                    if (record.size > limit) {
                        out.result.tooLarge = true;
                        return;
                    }
                }
                if (record?.backend === "indexeddb") {
                    const content = tx.objectStore(CONTENTS).get(record.blob);
                    content.onsuccess = () => { out.result.bytes = content.result?.bytes; };
                }
            };
        });
        const record = found.record;
        if (!record) return null;
        if (found.invalidSize) throw failure("io", "Invalid durable document size", null, "invalid_durable");
        if (found.tooLarge) throw failure("too_large", "File exceeds its read size limit");
        if (record.backend === "indexeddb") {
            if (found.bytes === undefined) {
                throw failure("io", "Durable document content is missing", null, "invalid_durable");
            }
            checkSize(found.bytes.byteLength, limit);
            return { record, bytes: await verify(record, new Uint8Array(found.bytes)) };
        }
        if (record.backend !== "opfs") {
            throw failure("io", "Invalid durable document backend", null, "invalid_durable");
        }
        let bytes;
        try {
            // The recorded backend is authoritative, even when OPFS probing failed.
            bytes = await withLock(blobLock(record.blob), "shared", async () => {
                const dir = await opfsDirectory(false);
                const handle = await dir.getFileHandle(record.blob);
                return fileBytes(await handle.getFile(), limit);
            });
        } catch (error) {
            if (error?.name !== "NotFoundError" && error?.name !== "NotReadableError") throw error;
            const latest = await query(state.db, id);
            if (latest && latest.blob !== record.blob) continue;
            throw failure("io", "Durable document blob is missing", null, "invalid_durable");
        }
        return { record, bytes: await verify(record, bytes) };
    }
    throw failure("io", "Durable document kept changing during read; retry");
}

async function readDocumentWithLimit(id, cancelled, limit) {
    validateLimit(limit);
    documentId(id);
    check(cancelled);
    const state = await storage();
    const current = await readCurrent(state, id, limit);
    if (!current) throw failure("notfound", "Browser document not found");
    check(cancelled);
    return { bytes: current.bytes, revision: current.record.revision };
}

export async function readDocument(id, cancelled) {
    return readDocumentWithLimit(id, cancelled, null);
}

export async function readDocumentBounded(id, cancelled, limit) {
    return readDocumentWithLimit(id, cancelled, limit);
}

// Current committed revision without reading blob bytes; null when missing.
export async function documentRevision(id, cancelled) {
    documentId(id);
    check(cancelled);
    const state = await storage();
    const record = await query(state.db, id);
    check(cancelled);
    return record?.revision ?? null;
}

export async function listDocuments(location, cancelled) {
    documentId(location, true);
    check(cancelled);
    const state = await storage();
    const prefix = `${location}/`;
    // Metadata-only records under the prefix; no document bytes are loaded.
    const records = await transaction(state.db, STORE, "readonly", (tx, out) => {
        const range = IDBKeyRange.bound(prefix, prefix + "\uffff", false, true);
        const request = tx.objectStore(STORE).getAll(range);
        request.onsuccess = () => { out.result = request.result; };
    });
    check(cancelled);
    const children = new Map();
    for (const record of records) {
        if (!record.id.startsWith(prefix)) continue;
        const rest = record.id.slice(prefix.length);
        const slash = rest.indexOf("/");
        const name = slash < 0 ? rest : rest.slice(0, slash);
        const id = prefix + name;
        if (slash >= 0) {
            children.set(id, { id, name, size: 0, revision: null, is_dir: true });
        } else if (!children.has(id)) {
            children.set(id, {
                id, name, size: record.size, revision: record.revision, is_dir: false
            });
        }
    }
    return JSON.stringify([...children.values()].sort((a, b) => a.id.localeCompare(b.id)));
}

// No await is allowed inside the readwrite transaction: get/CAS/put/GC remain
// active and serialized across tabs. Success means transaction.oncomplete.
// Resolves with the revision and the OPFS blobs that became unreferenced.
function commit(db, record, content, expected, cancelled, backup) {
    return transaction(db, [STORE, CONTENTS], "readwrite", (tx, out, fail) => {
        const documents = tx.objectStore(STORE);
        const contents = tx.objectStore(CONTENTS);
        const guard = step => (...args) => {
            try { step(...args); } catch (error) { fail(error); }
        };
        const request = documents.get(record.id);
        const bakRequest = documents.get(record.id + ".bak");
        bakRequest.onsuccess = guard(() => {
            check(cancelled);
            const previous = request.result;
            const actual = previous?.revision ?? null;
            if ((expected === "missing" && actual !== null) ||
                (expected !== "any" && expected !== "missing" &&
                    (!expected.startsWith("match:") || expected.slice(6) !== actual))) {
                throw failure("conflict", "Document revision changed", actual);
            }
            const keep = backup ? previous : undefined;
            const ids = keep ? [record.id, record.id + ".bak"] : [record.id];
            // A file cannot replace an implicit directory or vice versa.
            const scan = documents.getAllKeys();
            scan.onsuccess = guard(() => {
                check(cancelled);
                if (scan.result.some(id => ids.some(own => id !== own &&
                    (id.startsWith(own + "/") || own.startsWith(id + "/"))))) {
                    throw failure("io", "Document path conflicts with a directory", null, "path_conflict");
                }
                if (content) contents.put(content);
                if (previous?.legacyOriginal !== undefined && previous.revision === record.revision) {
                    record.legacyOriginal = previous.legacyOriginal;
                }
                // `.bak` shares the previous immutable blob and is committed
                // atomically with the replacement.
                if (keep) documents.put({ ...keep, id: record.id + ".bak" });
                documents.put(record);
                const candidates = new Map();
                for (const old of [previous, keep ? bakRequest.result : undefined]) {
                    if (old?.blob && old.blob !== record.blob) candidates.set(old.blob, old.backend);
                }
                const garbage = [];
                out.result = { revision: record.revision, backup: Boolean(keep), garbage };
                for (const [blob, backend] of candidates) {
                    const count = documents.index(BLOB_INDEX).count(blob);
                    count.onsuccess = guard(() => {
                        if (count.result !== 0) return;
                        // IndexedDB bytes are reclaimed atomically with the commit.
                        if (backend === "indexeddb") contents.delete(blob);
                        else if (backend === "opfs") garbage.push(blob);
                    });
                }
            });
        });
    });
}

// Deletes an OPFS blob only if no record references it, under its exclusive
// lock so no reader that found it before the swap is still reading. Returns
// whether it was deleted (false when locked and `ifAvailable`).
async function reclaim(state, blob, ifAvailable = false) {
    return withLock(blobLock(blob), "exclusive", async acquired => {
        if (!acquired || await references(state.db, blob) !== 0) return false;
        try {
            await (state.opfs ?? await opfsDirectory(false)).removeEntry(blob);
        } catch (error) {
            if (error?.name !== "NotFoundError") throw error;
        }
        return true;
    }, ifAvailable);
}

function committed(id, result) {
    announce(result.backup ? [[id, true], [id + ".bak", true]] : [[id, true]]);
    return { revision: result.revision, backup: result.backup };
}

// Resolves `{revision, backup}`; `backup` reports whether `<id>.bak` was written.
export async function writeDocument(id, input, expected, cancelled, backup = false) {
    documentId(id);
    check(cancelled);
    // Copy input before yielding: callers cannot mutate an in-flight write.
    const bytes = new Uint8Array(input);
    const state = await storage();
    const rev = await revision(bytes);
    check(cancelled);
    return withLock(`thinoda.documents.doc:${id}`, "exclusive", async () => {
        if (!state.opfs) {
            const blob = `bytes-${crypto.randomUUID()}`;
            const record = { id, size: bytes.length, revision: rev, backend: "indexeddb", blob };
            const result = await commit(state.db, record, { blob, bytes }, expected, cancelled,
                backup === true);
            return committed(id, result);
        }
        const blob = `blob-${crypto.randomUUID()}`;
        // Holding the new blob's lock until commit keeps the garbage sweep away
        // from an in-flight, not-yet-referenced blob.
        const result = await withLock(blobLock(blob), "exclusive", async () => {
            let committed = false;
            try {
                const handle = await state.opfs.getFileHandle(blob, { create: true });
                const writer = await handle.createWritable();
                try {
                    await writer.write(bytes);
                    check(cancelled);
                    await writer.close();
                } catch (error) {
                    try { await writer.abort(); } catch { /* Original error wins. */ }
                    throw error;
                }
                const verified = new Uint8Array(await (await handle.getFile()).arrayBuffer());
                if (!equal(bytes, verified)) throw failure("io", "OPFS write verification failed");
                check(cancelled);
                const record = { id, size: bytes.length, revision: rev, backend: "opfs", blob };
                const done = await commit(state.db, record, null, expected, cancelled,
                    backup === true);
                committed = true;
                return done;
            } finally {
                if (!committed) {
                    try { await state.opfs.removeEntry(blob); } catch { /* Swept later. */ }
                }
            }
        });
        // The write already succeeded; reclamation failure only delays GC.
        for (const old of result.garbage) {
            try { await reclaim(state, old); } catch (error) {
                console.warn("Thinoda could not reclaim a superseded document blob", error);
            }
        }
        // Do not return Cancelled after commit: the write has already succeeded.
        return committed(id, result);
    });
}

// Removes unreferenced OPFS blobs left by interrupted writes or reclamation.
// Requires Web Locks to tell crashed writes from in-flight writes in other
// tabs; without them nothing is swept. Returns the number of removed blobs.
export async function collectGarbage() {
    const state = await storage();
    if (!globalThis.navigator?.locks?.request || !globalThis.navigator?.storage?.getDirectory) {
        return 0;
    }
    let dir;
    try {
        dir = await opfsDirectory(false);
    } catch (error) {
        if (error?.name === "NotFoundError") return 0;
        throw error;
    }
    const names = [];
    for await (const [name, handle] of dir.entries()) {
        if (handle.kind === "file" && name.startsWith("blob-")) names.push(name);
    }
    let removed = 0;
    for (const name of names) {
        if (await reclaim({ ...state, opfs: dir }, name, true)) removed++;
    }
    return removed;
}

function legacyEntries() {
    // Access denial must be reported, not misinterpreted as an empty migration.
    const store = globalThis.localStorage;
    if (!store) throw failure("io", "Legacy browser storage is unavailable");
    const entries = [];
    for (let i = 0; i < store.length; i++) {
        const key = store.key(i);
        if (key === PREFIX + "/Documents" || key?.startsWith(PREFIX + "/Documents/")) {
            entries.push([key, store.getItem(key)]);
        }
    }
    return entries.sort(([a], [b]) => a.localeCompare(b));
}

// JSON.parse discards duplicate fields and rounds numeric spelling. The legacy
// Rust envelope decoder rejects duplicate known fields and non-integer tokens.
// Scan only top-level keys after JSON.parse has validated the syntax, ignoring
// nested/unknown values just as serde does. Escaped keys are decoded as well.
function validateEnvelope(raw) {
    let depth = 0;
    const seen = new Set();
    for (let i = 0; i < raw.length; i++) {
        const c = raw[i];
        if (c === "{" || c === "[") depth++;
        else if (c === "}" || c === "]") depth--;
        else if (c === '"') {
            let end = i + 1;
            for (; end < raw.length; end++) {
                if (raw[end] === "\\") end++;
                else if (raw[end] === '"') break;
            }
            const key = JSON.parse(raw.slice(i, end + 1));
            let next = end + 1;
            while (/\s/.test(raw[next] ?? "")) next++;
            if (depth === 1 && raw[next] === ":" && ["m", "l", "d"].includes(key)) {
                if (seen.has(key)) throw new Error("duplicate envelope field");
                seen.add(key);
                if (key !== "d") {
                    const token = raw.slice(next + 1).split(/[,}]/, 1)[0].trim();
                    if (!/^(0|[1-9][0-9]*)$/.test(token)) throw new Error("non-integer envelope field");
                }
            }
            i = end;
        }
    }
}

function decodeLegacy(key, raw) {
    try {
        const id = documentId(key.slice(PREFIX.length));
        const entry = JSON.parse(raw);
        validateEnvelope(raw);
        if (!entry || !Number.isSafeInteger(entry.m) || entry.m < 0 ||
            !Number.isSafeInteger(entry.l) || entry.l < 0 || typeof entry.d !== "string") {
            throw new Error("envelope");
        }
        // atob alone accepts whitespace/noncanonical input; match Rust's strict
        // STANDARD base64 decoder, including padding and discarded-bit checks.
        const binary = atob(entry.d);
        if (btoa(binary) !== entry.d || binary.length !== entry.l) throw new Error("base64");
        return { id, bytes: Uint8Array.from(binary, c => c.charCodeAt(0)) };
    } catch {
        throw failure("io", "Malformed legacy /Documents entry; original data was retained",
            null, "invalid_legacy");
    }
}

async function conflictIdForRevision(id, contentRevision) {
    const pathHash = (await revision(new TextEncoder().encode(id))).slice(7);
    const contentHash = contentRevision.slice(7);
    return `/Documents/.thinoda-legacy/${pathHash}/${contentHash}/${id.split("/").at(-1)}`;
}

function recordLegacyConflict(db, copy, original, expectedRevision) {
    return transaction(db, STORE, "readwrite", (tx, _, fail) => {
        const documents = tx.objectStore(STORE);
        const request = documents.get(copy);
        request.onsuccess = () => {
            try {
                const record = request.result;
                if (!record || record.revision !== expectedRevision ||
                    (record.legacyOriginal !== undefined && record.legacyOriginal !== original)) {
                    throw failure("io", "Legacy conflict copy verification failed", null, "invalid_durable");
                }
                if (record.legacyOriginal !== original) {
                    documents.put({ ...record, legacyOriginal: original });
                }
            } catch (error) {
                fail(error);
            }
        };
    });
}

async function preserveLegacy(entry, state) {
    const expectedRevision = await revision(entry.bytes);
    const copy = await conflictIdForRevision(entry.id, expectedRevision);
    const existing = await readMigrating(state, copy, entry);
    if (existing) {
        if (!equal(existing.bytes, entry.bytes)) {
            throw failure("io", "Legacy conflict copy verification failed", null, "invalid_durable");
        }
    } else {
        try {
            await writeDocument(copy, entry.bytes, "missing");
        } catch (error) {
            if (error?.code !== "conflict") throw error;
        }
        const verified = await readDocument(copy);
        if (!equal(verified.bytes, entry.bytes)) {
            throw failure("io", "Migration verification failed", null, "invalid_durable");
        }
    }
    // Keep the mapping with the durable copy, even if an older tab later
    // changes or removes the legacy key. The CAS prevents annotating a
    // different document after the verified read.
    await recordLegacyConflict(state.db, copy, entry.id, expectedRevision);
    return { copied: true, original: entry.id, preserved: copy };
}

async function readMigrating(state, id, legacy) {
    try {
        return await readCurrent(state, id);
    } catch (error) {
        if (error?.reason !== "invalid_durable") throw error;
        const record = await query(state.db, id);
        // A retained legacy copy is not necessarily the latest saved version.
        // Repair only the exact committed bytes, never roll back to an older save.
        if (!record || record.size !== legacy.bytes.length ||
            record.revision !== await revision(legacy.bytes) ||
            (id !== legacy.id && record.legacyOriginal !== undefined &&
                record.legacyOriginal !== legacy.id)) {
            throw error;
        }
        try {
            await writeDocument(id, legacy.bytes, `match:${record.revision}`);
        } catch (writeError) {
            if (writeError?.code !== "conflict") throw writeError;
            // Another tab saved first. Read its committed version, not our copy.
        }
        const restored = await readCurrent(state, id);
        console.warn("Thinoda rechecked a browser document after attempting exact-copy recovery");
        return restored;
    }
}

async function migrateOne(entry, retries = 3) {
    if (retries === 0) {
        throw failure("io", "Durable documents kept changing during migration; retry initialization",
            null, "migration_changed");
    }
    const state = await storage();
    const current = await readMigrating(state, entry.id, entry);
    if (current) {
        if (equal(current.bytes, entry.bytes)) return { copied: false };
        return preserveLegacy(entry, state);
    }
    try {
        await writeDocument(entry.id, entry.bytes, "missing");
    } catch (error) {
        if (error?.code === "io" && error.reason === "path_conflict") {
            return preserveLegacy(entry, state);
        }
        if (error?.code !== "conflict") throw error;
        // A concurrent writer won. Re-evaluate without overwriting its revision.
        return migrateOne(entry, retries - 1);
    }
    const verified = await readDocument(entry.id);
    if (!equal(verified.bytes, entry.bytes)) {
        // Another tab may have saved since commit; preserve the legacy copy too.
        return migrateOne(entry, retries - 1);
    }
    return { copied: false };
}

export async function initializeDocuments() {
    await storage();
    // Validate the complete snapshot before migrating any entries.
    const snapshot = legacyEntries();
    const decoded = snapshot.map(([key, raw]) => decodeLegacy(key, raw));
    for (const entry of decoded) {
        await migrateOne(entry);
    }
    // Legacy writers may still exist. Detect edits during migration and require
    // a retry; never remove legacy keys or mark a stale snapshot complete.
    const after = legacyEntries();
    if (JSON.stringify(snapshot) !== JSON.stringify(after)) {
        throw failure("io", "Legacy documents changed during migration; retry initialization",
            null, "migration_changed");
    }
    const state = await storage();
    // Best-effort reclamation of blobs orphaned by earlier interrupted writes.
    try { await collectGarbage(); } catch (error) {
        console.warn("Thinoda could not sweep unreferenced document blobs", error);
    }
    // Metadata only: the app gets both its existence index and every preserved
    // conflict, including copies from earlier legacy snapshots.
    const indexed = await transaction(state.db, STORE, "readonly", (tx, out) => {
        const documents = tx.objectStore(STORE);
        const prefix = "/Documents/.thinoda-legacy/";
        const keys = documents.getAllKeys();
        const copies = documents.getAll(IDBKeyRange.bound(prefix, prefix + "\uffff", false, true));
        out.result = {};
        keys.onsuccess = () => { out.result.documents = keys.result; };
        copies.onsuccess = () => { out.result.copies = copies.result; };
    });
    const conflicts = [];
    for (const record of indexed.copies) {
        if (record.legacyOriginal === undefined) continue;
        if (!validDocumentId(record.legacyOriginal) ||
            typeof record.revision !== "string" ||
            !/^sha256:[0-9a-f]{64}$/.test(record.revision) ||
            record.id !== await conflictIdForRevision(record.legacyOriginal, record.revision)) {
            throw failure("io", "Legacy conflict copy verification failed", null, "invalid_durable");
        }
        conflicts.push({ original: record.legacyOriginal, preserved: record.id });
    }
    conflicts.sort((a, b) => a.original.localeCompare(b.original) || a.preserved.localeCompare(b.preserved));
    return JSON.stringify({
        backend: state.opfs ? "opfs" : "indexeddb",
        verified: decoded.length,
        conflicts,
        documents: indexed.documents
    });
}

export async function storageEstimate() {
    if (!globalThis.navigator?.storage?.estimate) {
        throw failure("unsupported", "Storage estimates are unavailable");
    }
    const estimate = await navigator.storage.estimate();
    const persisted = navigator.storage.persisted ? await navigator.storage.persisted() : null;
    return JSON.stringify({
        usage: estimate.usage ?? null, quota: estimate.quota ?? null, persisted
    });
}

// Call only from an explicit user request. Initialization never asks for consent.
export async function requestPersistence() {
    if (!globalThis.navigator?.storage?.persist) {
        throw failure("unsupported", "Persistent storage requests are unavailable");
    }
    if (navigator.storage.persisted && await navigator.storage.persisted()) return true;
    return navigator.storage.persist();
}

export function fileSystemAccessSupported() {
    return globalThis.isSecureContext === true && typeof globalThis.showOpenFilePicker === "function";
}

// Invoke directly from a user gesture, before awaiting any storage operations.
// Register handles only; callers read and stage each file separately.
export async function pickFileHandles(multiple) {
    if (!fileSystemAccessSupported()) {
        throw failure("unsupported", "Writable file picking is unavailable; use import and download");
    }
    let picked;
    try {
        picked = await globalThis.showOpenFilePicker({ multiple: multiple === true });
    } catch (error) {
        if (error.name === "AbortError") throw failure("cancelled", "File picker cancelled");
        throw error;
    }
    if (!Array.isArray(picked) || picked.length === 0) {
        throw failure("cancelled", "File picker cancelled");
    }
    return picked.map(handle => {
        try {
            if (!handle || typeof handle.createWritable !== "function" ||
                typeof handle.getFile !== "function" || typeof handle.name !== "string") {
                throw failure("unsupported", "The picked file does not support saving");
            }
            const session = crypto.randomUUID();
            handles.set(session, { handle, name: handle.name, limit: null, verifiedSize: null });
            return { session, name: handle.name };
        } catch (error) {
            return { error };
        }
    });
}

export async function readPickedFile(session, cancelled, limit = null) {
    validateLimit(limit);
    check(cancelled);
    const picked = handles.get(session);
    if (!picked) throw failure("io", "Picked-file session expired; pick the file again");
    const file = await picked.handle.getFile();
    if (file.name !== picked.name) throw failure("io", "Picked-file name changed during read");
    const bytes = await fileBytes(file, limit);
    const rev = await revision(bytes);
    check(cancelled);
    picked.limit = limit;
    picked.verifiedSize = bytes.length;
    return { bytes, revision: rev };
}

async function savePicked(session, input, expected, cancelled) {
    const picked = handles.get(session);
    if (!picked) throw failure("io", "Picked-file session expired; pick the file again");
    const { handle } = picked;
    check(cancelled);
    // Explicit Save may prompt for write permission. Never ask at startup/open.
    if (await handle.queryPermission({ mode: "readwrite" }) !== "granted" &&
        await handle.requestPermission({ mode: "readwrite" }) !== "granted") {
        throw failure("io", "Permission to save the picked file was denied");
    }
    const bytes = new Uint8Array(input);
    // Reread only up to the import limit or the last verified file size.
    // The new output can be larger, but must not expand the pre-save bound.
    const readLimit = picked.limit === null
        ? null : Math.max(picked.limit, picked.verifiedSize ?? 0);
    const afterLimit = readLimit === null ? null : Math.max(readLimit, bytes.length);
    const before = await fileBytes(await handle.getFile(), readLimit);
    const actual = await revision(before);
    if (expected !== "any" && (!expected.startsWith("match:") || expected.slice(6) !== actual)) {
        throw failure("conflict", "Picked file changed since it was read", actual);
    }
    const writer = await handle.createWritable({ keepExistingData: false });
    try {
        await writer.write(bytes);
        // Recheck immediately before close (commit). OS edits cannot be locked
        // by this API, so this is detection, not a filesystem-level CAS promise.
        const fresh = await fileBytes(await handle.getFile(), readLimit);
        if (!equal(before, fresh)) {
            throw failure("conflict", "Picked file changed during save", await revision(fresh));
        }
        check(cancelled);
        await writer.close();
    } catch (error) {
        try { await writer.abort(); } catch { /* Original error wins. */ }
        throw error;
    }
    const after = await fileBytes(await handle.getFile(), afterLimit);
    if (!equal(bytes, after)) throw failure("io", "Picked-file save verification failed");
    const savedRevision = await revision(after);
    picked.verifiedSize = after.length;
    return savedRevision;
}

export async function writePickedFile(session, input, expected, cancelled) {
    const bytes = new Uint8Array(input);
    if (!globalThis.navigator?.locks?.request) {
        // No fake atomicity or unlocked multi-tab write fallback.
        throw failure("unsupported", "Cooperative picked-file save locking is unavailable");
    }
    // All picked-file saves on this origin share a lock, including two sessions
    // referring to the same file. External applications do not participate.
    return navigator.locks.request("thinoda-picked-file-save", () =>
        savePicked(session, bytes, expected, cancelled));
}

export function forgetPickedFile(session) {
    handles.delete(session);
}
