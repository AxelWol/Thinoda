// Real IndexedDB/Web Locks regression harness. Serve the repository root on an
// isolated local origin, then open this HTML in a browser. No npm dependencies.
const params = new URLSearchParams(location.search);
if (params.has("without-locks")) {
    Object.defineProperty(navigator, "locks", { value: undefined, configurable: true });
}
if (params.has("deny-locks")) {
    Object.defineProperty(navigator, "locks", {
        value: { request() { return Promise.reject(new DOMException("locking denied", "SecurityError")); } },
        configurable: true,
    });
}
if (params.has("stall-locks")) {
    const locks = navigator.locks;
    Object.defineProperty(navigator, "locks", {
        value: {
            request(...args) {
                return new Promise((resolve, reject) => {
                    setTimeout(() => locks.request(...args).then(resolve, reject), 200);
                });
            },
        },
        configurable: true,
    });
}
if (params.has("deny-idb")) {
    Object.defineProperty(globalThis, "indexedDB", {
        value: { open() { throw new DOMException("storage denied", "SecurityError"); } },
        configurable: true,
    });
}
if (params.has("stall-open")) {
    Object.defineProperty(globalThis, "indexedDB", {
        value: {
            open() {
                const request = {};
                setTimeout(() => {
                    request.result = { close() { globalThis.testLateDatabaseClosed = true; } };
                    request.onsuccess?.();
                }, 200);
                return request;
            },
        },
        configurable: true,
    });
}
if (params.has("simulate-eviction")) {
    Object.defineProperty(globalThis, "indexedDB", {
        value: {
            open() {
                const request = {};
                setTimeout(() => {
                    request.result = { objectStoreNames: { contains: () => true }, close() {} };
                    request.onupgradeneeded?.();
                    request.onsuccess?.();
                    globalThis.testEvictionOpenCompleted = true;
                }, 0);
                return request;
            },
        },
        configurable: true,
    });
    Object.defineProperty(navigator.storage, "persisted", {
        value: () => new Promise(resolve => setTimeout(() => resolve(false), 150)),
        configurable: true,
    });
}
if (params.has("observe-persistence")) {
    globalThis.testPersistenceRequests = 0;
    globalThis.testPersistenceGranted = false;
    Object.defineProperty(navigator.storage, "persisted", {
        value: () => Promise.resolve(globalThis.testPersistenceGranted), configurable: true,
    });
    Object.defineProperty(navigator.storage, "persist", {
        value: () => { globalThis.testPersistenceRequests++; return Promise.resolve(true); },
        configurable: true,
    });
}
const api = await import("../../thinoda-platform/src/recovery_db.js");
globalThis.recoveryTestApi = api;

if (params.has("child")) {
    parent.postMessage({ recoveryTestReady: true }, location.origin);
} else {
    const output = document.querySelector("#results");
    const lines = [];
    const write = text => {
        lines.push(text);
        output.textContent = lines.join("\n");
    };
    const assert = (condition, message) => {
        if (!condition) throw new Error(message);
    };
    const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
    const eventually = async (predicate, description) => {
        const deadline = Date.now() + 20_000;
        while (Date.now() < deadline) {
            const value = await predicate();
            if (value) return value;
            await sleep(50);
        }
        throw new Error(`Timed out: ${description}`);
    };
    const discover = testApi => testApi.recoveryDiscover(testApi.recoveryStartupDeadline());
    const idle = async id => eventually(() => {
        const state = JSON.parse(api.recoveryStatus(id));
        if (state.error) throw new Error(state.error);
        return !state.pending;
    }, "recovery transaction");
    const child = async query => {
        const frame = document.createElement("iframe");
        frame.hidden = true;
        frame.src = `${location.pathname}?child=1&${query || ""}`;
        document.body.append(frame);
        await eventually(() => frame.contentWindow?.recoveryTestApi, "test tab startup");
        return frame;
    };
    const db = () => new Promise((resolve, reject) => {
        const request = indexedDB.open("thinoda.recovery.v1");
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
    });
    const fixtureWrite = (connection, stores) => connection.transaction(stores, "readwrite");
    const readRecord = async (session, key) => {
        const connection = await db();
        try {
            return await new Promise((resolve, reject) => {
                const tx = connection.transaction(["documents"], "readonly");
                const request = tx.objectStore("documents").get(`${session}/${key}`);
                request.onsuccess = () => resolve(request.result);
                request.onerror = () => reject(request.error);
            });
        } finally {
            connection.close();
        }
    };
    const seedSession = async (id, records, manifest) => {
        const connection = await db();
        try {
            await new Promise((resolve, reject) => {
                const tx = fixtureWrite(connection, ["sessions", "documents"]);
                tx.objectStore("sessions").add({
                    id, token: "synthetic-abandoned-writer", created: Date.now(), manifest,
                });
                for (const record of records) tx.objectStore("documents").add(record);
                tx.oncomplete = resolve;
                tx.onabort = () => reject(tx.error);
            });
        } finally {
            connection.close();
        }
    };
    const seedBatch = async (sessions, documents) => {
        const connection = await db();
        try {
            await new Promise((resolve, reject) => {
                const tx = fixtureWrite(connection, ["sessions", "documents"]);
                for (const row of sessions) tx.objectStore("sessions").add(row);
                for (const record of documents) tx.objectStore("documents").add(record);
                tx.oncomplete = resolve;
                tx.onabort = () => reject(tx.error);
            });
        } finally {
            connection.close();
        }
    };
    const removeBatch = async (sessions, documents) => {
        const connection = await db();
        try {
            await new Promise((resolve, reject) => {
                const tx = fixtureWrite(connection, ["sessions", "documents"]);
                for (const id of sessions) tx.objectStore("sessions").delete(id);
                for (const id of documents) tx.objectStore("documents").delete(id);
                tx.oncomplete = resolve;
                tx.onabort = () => reject(tx.error);
            });
        } finally {
            connection.close();
        }
    };
    const readSessionDocuments = async id => {
        const connection = await db();
        try {
            return await new Promise((resolve, reject) => {
                const tx = connection.transaction(["documents"], "readonly");
                const request = tx.objectStore("documents").index("session").getAll(IDBKeyRange.only(id));
                request.onsuccess = () => resolve(request.result);
                request.onerror = () => reject(request.error);
            });
        } finally {
            connection.close();
        }
    };
    const removeFixture = async (id, keys) => {
        const connection = await db();
        try {
            await new Promise((resolve, reject) => {
                const tx = fixtureWrite(connection, ["sessions", "documents"]);
                tx.objectStore("sessions").delete(id);
                for (const key of keys) tx.objectStore("documents").delete(`${id}/${key}`);
                tx.oncomplete = resolve;
                tx.onabort = () => reject(tx.error);
            });
        } finally {
            connection.close();
        }
    };
    const removeSessionRecord = async session => {
        const connection = await db();
        try {
            await new Promise((resolve, reject) => {
                const tx = fixtureWrite(connection, ["sessions"]);
                tx.objectStore("sessions").delete(session);
                tx.oncomplete = resolve;
                tx.onabort = () => reject(tx.error);
            });
        } finally {
            connection.close();
        }
    };
    const changeSessionManifest = async session => {
        const connection = await db();
        try {
            await new Promise((resolve, reject) => {
                const tx = fixtureWrite(connection, ["sessions"]);
                const store = tx.objectStore("sessions");
                const request = store.get(session);
                request.onsuccess = () => {
                    const row = request.result;
                    row.manifest = null;
                    store.put(row);
                };
                tx.oncomplete = resolve;
                tx.onabort = () => reject(tx.error);
            });
        } finally {
            connection.close();
        }
    };
    const readSessionRecord = async session => {
        const connection = await db();
        try {
            return await new Promise((resolve, reject) => {
                const tx = connection.transaction(["sessions"], "readonly");
                const request = tx.objectStore("sessions").get(session);
                request.onsuccess = () => resolve(request.result);
                request.onerror = () => reject(request.error);
            });
        } finally {
            connection.close();
        }
    };
    const unpackRescueArchive = async blob => {
        const magic = "THINODA-RECOVERY-RECORD-1\n";
        const prefix = await blob.slice(0, 128).text();
        assert(prefix.startsWith(magic), "Recovery record archive has invalid magic");
        const end = prefix.indexOf("\n", magic.length);
        assert(end !== -1, "Recovery record archive has no header length");
        const length = Number(prefix.slice(magic.length, end));
        assert(Number.isSafeInteger(length) && length > 0, "Invalid recovery archive header length");
        const start = end + 1;
        const header = JSON.parse(await blob.slice(start, start + length).text());
        let offset = start + length;
        const parts = header.parts.map(part => {
            const result = blob.slice(offset, offset + part.byteLength);
            offset += part.byteLength;
            return result;
        });
        assert(offset === blob.size, "Recovery archive is truncated or has unexpected bytes");
        return { header, parts };
    };
    const archiveProperty = (header, reference, name) => {
        const node = header.nodes[reference.ref];
        const property = node?.properties?.find(([key]) => key === name);
        assert(property, `Missing archive property: ${name}`);
        return property[1];
    };
    const removeDocumentRecord = async (session, key) => {
        const connection = await db();
        try {
            await new Promise((resolve, reject) => {
                const tx = fixtureWrite(connection, ["documents"]);
                tx.objectStore("documents").delete(`${session}/${key}`);
                tx.oncomplete = resolve;
                tx.onabort = () => reject(tx.error);
            });
        } finally {
            connection.close();
        }
    };
    let testsRun = 0;
    const test = async (name, body) => {
        const filter = params.get("filter")?.toLowerCase();
        if (filter && !name.toLowerCase().includes(filter)) return;
        await body();
        testsRun++;
        write(`PASS ${name}`);
    };

    try {
        if (typeof indexedDB.databases === "function"
            && !(await indexedDB.databases()).some(entry => entry.name === "thinoda.recovery.v1")) {
            await test("rescue does not create a missing recovery database", async () => {
                let failed = false;
                try { await api.recoveryRescueBrowserPage("documents"); } catch (_) { failed = true; }
                assert(failed, "An absent database was reported as an empty successful list");
                assert(!(await indexedDB.databases()).some(entry => entry.name === "thinoda.recovery.v1"),
                    "Read-only rescue created a new recovery database");
            });
        }
        await api.recoveryInitialize();
        await test("recovery status and actions are exposed to browser accessibility", async () => {
            api.recoveryAccessibilityStatus(
                "pending", "Recovery is still being checked.", "Retry recovery", "Review recovery",
                true, true,
            );
            const panel = document.querySelector("#thinoda-recovery-accessibility");
            const status = panel?.querySelector('[role="status"]');
            assert(status?.getAttribute("aria-live") === "polite", "Recovery is not a live status");
            assert(status.tabIndex === 0 && status.textContent.includes("still being checked"),
                "Recovery status is not keyboard reachable");
            const retry = panel.querySelector('[data-action="retry"]');
            const review = panel.querySelector('[data-action="review"]');
            assert(!retry.hidden && !review.hidden, "Recovery actions are hidden");
            retry.click();
            assert(api.recoveryTakeAccessibilityAction() === "retry", "Retry action was not delivered");
            const model = JSON.stringify({
                canRestore: true,
                sessions: [{ id: "session", docs: [{ key: "orphan", title: "<sample>" }] }],
                documents: [{ key: "live", title: "Current", drafts: true, ink: true, canRetryInk: true }],
                labels: { confirmDiscard: "Confirm discard" },
            });
            api.recoveryAccessibilityStatus(
                "pending", "Recovery is still being checked.", "Retry recovery", "Review recovery",
                true, true, model,
            );
            review.click();
            assert(api.recoveryTakeAccessibilityAction() === "review", "Review action was not delivered");
            assert(review.getAttribute("aria-expanded") === "true", "Review state was not exposed");
            assert(!panel.querySelector("img"), "Recovery titles were interpreted as HTML");
            panel.querySelector('[data-focus="restore:session"]').click();
            assert(JSON.parse(api.recoveryTakeAccessibilityAction()).session === "session",
                "Accessible restore was not bound to its session");
            const confirm = panel.querySelector('[data-focus="confirm:session:orphan"]');
            assert(confirm.hidden, "Destructive confirmation was not initially hidden");
            panel.querySelector('[data-focus="discard:session:orphan"]').click();
            assert(!confirm.hidden, "Discard did not require a separate confirmation");
            panel.querySelector('[data-focus="cancel-discard:session:orphan"]').click();
            assert(confirm.hidden && !api.recoveryTakeAccessibilityAction(),
                "Canceling discard left it armed or queued a destructive action");
            panel.querySelector('[data-focus="discard:session:orphan"]').click();
            confirm.click();
            const discard = JSON.parse(api.recoveryTakeAccessibilityAction());
            assert(discard.action === "discard-doc" && discard.session === "session" && discard.key === "orphan",
                "Confirmed discard was not bound to exactly one source");
            panel.querySelector('[data-focus="drafts:live"]').click();
            assert(JSON.parse(api.recoveryTakeAccessibilityAction()).action === "download-drafts",
                "Draft rescue was not accessible");
            const copiedModel = JSON.parse(model);
            copiedModel.sessions[0].copied = true;
            api.recoveryAccessibilityStatus(
                "originals-retained", "Originals are retained.", "Retry", "Review", false, true,
                JSON.stringify(copiedModel),
            );
            assert(panel.querySelector('[data-focus="restore:session"]').disabled,
                "An already-open copy could be restored repeatedly");
            assert(panel.querySelector('[data-focus="restore-all"]').disabled,
                "Restore All remained enabled when all copies are open");
            assert(panel.textContent.includes("original is retained"),
                "The retained original was not clearly labeled");
            copiedModel.sessions[0].uncertain = true;
            copiedModel.sessions[0].docs[0].uncertain = true;
            api.recoveryAccessibilityStatus(
                "deferred", "Discard result unknown.", "Retry", "Review", false, true,
                JSON.stringify(copiedModel),
            );
            assert(!panel.querySelector('[data-focus="discard:session:orphan"]'),
                "An indeterminate discard could be retried destructively");
            panel.querySelector('[data-focus="verify:session:orphan"]').click();
            assert(JSON.parse(api.recoveryTakeAccessibilityAction()).action === "verify-discard",
                "Indeterminate discard could not be verified");
            panel.querySelector('[data-focus="keep:session:orphan"]').click();
            assert(JSON.parse(api.recoveryTakeAccessibilityAction()).action === "keep-discard",
                "A pending discard did not offer an accessible keep action");
            assert(panel.querySelector('[data-focus="orphan:session:orphan"]').textContent.includes("cached"),
                "Indeterminate source did not offer cached rescue");
            copiedModel.sessions[0].uncertain = false;
            copiedModel.sessions[0].docs[0].uncertain = false;
            copiedModel.sessions[0].docs[0].canDiscard = false;
            api.recoveryAccessibilityStatus(
                "originals-retained", "Legacy original retained.", "Retry", "Review", false, true,
                JSON.stringify(copiedModel),
            );
            assert(!panel.querySelector('[data-focus="discard:session:orphan"]'),
                "Legacy recovery offered a non-atomic discard");
            assert(panel.textContent.includes("older tabs cannot be excluded safely"),
                "The legacy retention restriction was not explained");
            api.recoveryAccessibilityStatus("ready", "Recovery is ready.", "Retry", "Review", false, false);
            assert(retry.hidden && review.hidden, "Unavailable actions remained exposed");
        });
        await test("late recovery status does not steal focus and exposes safe retry actions", async () => {
            const input = document.createElement("input");
            document.body.append(input);
            input.focus();
            const model = JSON.stringify({
                canRestore: false,
                canDecide: false,
                canRescue: true,
                rescue: { legacyPrefix: "thinoda.vfs:/thinoda/data/recovery/" },
                sessions: [{
                    id: "indexeddb:cached",
                    requiresRecheck: true,
                    docs: [
                        { key: "cached", title: "Cached", canDiscard: false, requiresRecheck: true },
                        { key: "fresh", title: "Fresh", canDiscard: true },
                    ],
                }],
                documents: [],
            });
            api.recoveryAccessibilityStatus(
                "discovery-pending", "Still checking.", "Retry", "Review", false, true, model,
            );
            const panel = document.querySelector("#thinoda-recovery-accessibility");
            assert(document.activeElement === input, "A late result stole focus from active work");
            assert(!document.querySelector("#thinoda-recovery-rescue"),
                "A late result opened the rescue dialog without a user action");
            panel.querySelector('[data-action="review"]').click();
            api.recoveryTakeAccessibilityAction();
            assert(panel.querySelector('[data-focus="restore:indexeddb:cached"]').disabled,
                "A cached session could be restored before it was rechecked");
            assert(!panel.querySelector('[data-focus="discard:indexeddb:cached:cached"]'),
                "A cached source offered destructive discard");
            assert(panel.querySelector('[data-focus="discard:indexeddb:cached:fresh"]').disabled,
                "An in-flight retry allowed a destructive decision");
            assert(panel.querySelector('[data-focus="raw:indexeddb:cached:cached"]'),
                "A cached source lost its raw download action");
            assert(panel.textContent.includes("needs rechecking"), "The cached state was not explained");
            const rescue = panel.querySelector('[data-action="rescue"]');
            rescue.focus();
            rescue.click();
            const dialog = document.querySelector('#thinoda-recovery-rescue [role="dialog"]');
            assert(dialog?.getAttribute("aria-modal") === "true", "The requested rescue dialog is not accessible");
            dialog.querySelector('[data-action="close"]').click();
            assert(!document.querySelector("#thinoda-recovery-rescue"), "Rescue did not close");
            assert(document.activeElement === rescue, "Closing rescue did not restore focus");
            input.remove();
            api.recoveryAccessibilityStatus("ready", "Recovery is ready.", "Retry", "Review", false, false);
        });
        await test("legacy rescue preserves malformed UTF-16 data and never changes storage", async () => {
            const prefix = "thinoda.vfs:/thinoda/data/recovery/";
            const key = `${prefix}${crypto.randomUUID()}/docs/unreadable-\ud800.json`;
            const unrelated = `thinoda.vfs:/thinoda/data/other/${crypto.randomUUID()}`;
            // The surrogate pair straddles the export chunk boundary; the
            // trailing unpaired surrogate must remain an exact code unit too.
            const value = `${"x".repeat(65_535)}\ud83d\ude80 invalid \ud800`;
            localStorage.setItem(key, value);
            localStorage.setItem(unrelated, "unrelated");
            try {
                let page = 0;
                const found = [];
                do {
                    const result = api.recoveryRescueLegacyPage(prefix, page);
                    found.push(...result.keys);
                    page = result.next;
                } while (page !== null);
                assert(found.includes(key) && !found.includes(unrelated), "Legacy rescue listed the wrong keys");
                const blob = await api.recoveryRescueLegacyArchive(prefix, key);
                const lines = (await blob.text()).split("\n");
                const header = JSON.parse(lines.shift());
                const copy = lines.filter(Boolean).map(line => JSON.parse(line)).join("");
                assert(header.key === key && header.utf16Length === value.length,
                    "Legacy rescue lost its exact key or length");
                assert(copy === value, "Legacy rescue changed the captured UTF-16 code units");
                assert(localStorage.getItem(key) === value && localStorage.getItem(unrelated) === "unrelated",
                    "Legacy rescue modified browser storage");
                const cancel = new AbortController();
                cancel.abort();
                let cancelled = false;
                try { await api.recoveryRescueLegacyArchive(prefix, key, cancel.signal); }
                catch (_) { cancelled = true; }
                assert(cancelled && localStorage.getItem(key) === value,
                    "Cancelled rescue changed the original or produced a copy");

                api.recoveryOpenRescue(JSON.stringify({ legacyPrefix: prefix }));
                const panel = document.querySelector("#thinoda-recovery-rescue");
                panel.querySelector('[data-action="legacy"]').click();
                const button = Array.from(panel.querySelectorAll('[data-action="prepare"]'))
                    .find(item => item.textContent.includes(key.slice(prefix.length, prefix.length + 36)));
                assert(button, "The rescue panel did not list the unreadable entry");
                button.click();
                const link = await eventually(
                    () => panel.querySelector('[data-action="download-archive"]'),
                    "preparing the legacy rescue link",
                );
                const offered = await (await fetch(link.href)).text();
                assert(offered.split("\n").filter(Boolean).slice(1).map(line => JSON.parse(line)).join("") === value,
                    "The visible rescue download was not the selected original");
                panel.querySelector('[data-action="close"]').click();
                assert(localStorage.getItem(key) === value, "Preparing a rescue download changed the original");
            } finally {
                document.querySelector('#thinoda-recovery-rescue [data-action="close"]')?.click();
                localStorage.removeItem(key);
                localStorage.removeItem(unrelated);
            }
        });
        await test("legacy discovery rejects strings that cannot cross the UTF-8 bridge losslessly", async () => {
            const prefix = "thinoda.vfs:/thinoda/data/recovery/";
            const key = `${prefix}${crypto.randomUUID()}/docs/unreadable.json`;
            const frame = await child();
            const fallback = frame.contentWindow.recoveryTestApi;
            Object.defineProperty(frame.contentWindow.String.prototype, "isWellFormed", {
                configurable: true, value: undefined,
            });
            try {
                const valid = '{"l":0,"d":"","note":"\ud83d\ude80"}';
                localStorage.setItem(key, valid);
                for (const reader of [api, fallback]) {
                    assert(reader.recoveryReadLegacyValue(key) === valid,
                        "A well-formed legacy value changed across the bridge");
                }
                for (const unit of ["\ud800", "\udc00", "\ud800x"]) {
                    const original = `{"l":0,"d":"","ignored":"${unit}"}`;
                    localStorage.setItem(key, original);
                    for (const reader of [api, fallback]) {
                        let rejected = false;
                        try { reader.recoveryReadLegacyValue(key); } catch (_) { rejected = true; }
                        assert(rejected, "Malformed UTF-16 could be silently converted for discovery");
                    }
                    const archive = await api.recoveryRescueLegacyArchive(prefix, key);
                    const copy = (await archive.text()).split("\n").filter(Boolean).slice(1)
                        .map(line => JSON.parse(line)).join("");
                    assert(copy === original && localStorage.getItem(key) === original,
                        "Rejecting an unsafe bridge conversion changed the original or its rescue copy");
                }
                localStorage.removeItem(key);
                assert(api.recoveryReadLegacyValue(key) === null, "A missing legacy value was invented");
            } finally {
                frame.remove();
                localStorage.removeItem(key);
            }
        });
        await test("oversized legacy rescue is bounded to explicit reads and retains every code unit", async () => {
            const frame = await child();
            const other = frame.contentWindow.recoveryTestApi;
            const prefix = "thinoda.vfs:/thinoda/data/recovery/";
            const key = `${prefix}oversized/docs/original.json`;
            const original = `${"q".repeat(32 * 1024 * 1024 + 1)}\ud800`;
            let writes = 0;
            const storage = {
                length: 1,
                key(index) { return index === 0 ? key : null; },
                getItem(candidate) { return candidate === key ? original : null; },
                setItem() { writes++; throw new Error("unexpected write"); },
                removeItem() { writes++; throw new Error("unexpected deletion"); },
            };
            Object.defineProperty(frame.contentWindow, "localStorage", { configurable: true, value: storage });
            try {
                let deferred = false;
                try { other.recoveryReadLegacyValue(key); } catch (_) { deferred = true; }
                assert(deferred && writes === 0, "Oversized legacy data crossed the preview bridge");
                assert(other.recoveryRescueLegacyPage(prefix).keys[0] === key, "Oversized key was not listed");
                const archive = await other.recoveryRescueLegacyArchive(prefix, key);
                const lines = (await archive.text()).split("\n");
                const header = JSON.parse(lines.shift());
                let offset = 0;
                for (const line of lines.filter(Boolean)) {
                    const chunk = JSON.parse(line);
                    assert(chunk === original.slice(offset, offset + chunk.length),
                        "Oversized legacy rescue changed a chunk");
                    offset += chunk.length;
                }
                assert(offset === original.length && header.utf16Length === original.length,
                    "Oversized legacy rescue truncated the value");
                assert(writes === 0 && storage.getItem(key) === original, "Legacy rescue changed its source");
                storage.getItem = () => { throw new DOMException("storage denied", "SecurityError"); };
                let unreadable = false;
                try { await other.recoveryRescueLegacyArchive(prefix, key); } catch (_) { unreadable = true; }
                assert(unreadable && writes === 0, "Unreadable legacy storage was mistaken for a successful rescue");
            } finally {
                frame.remove();
            }
        });
        await test("read-only rescue paginates malformed IndexedDB records without Web Locks", async () => {
            const id = crypto.randomUUID();
            const keys = Array.from({ length: 28 }, (_, index) => `rescue${String(index).padStart(2, "0")}`);
            const records = keys.map((key, index) => ({
                id: `${id}/${key}`, session: id, key, revision: 1, deleted: false,
                bytes: new Uint8Array([index, 0, 255]).buffer,
            }));
            // The malformed record is not in the session index, but its exact
            // primary key and bytes must remain available to read-only rescue.
            delete records[5].session;
            records[5].key = "different-key";
            const extra = { label: "retained", negativeZero: -0, missing: undefined };
            extra.self = extra;
            records[5].extra = extra;
            records[5].sameBytes = records[5].bytes;
            extra.view = new Uint8Array(records[5].bytes, 1, 2);
            extra.longText = `${"p".repeat(65_535)}\ud83d\ude80${"q".repeat(1_000)}\ud800`;
            extra.map = new Map([["not-a-number", NaN], ["infinity", Infinity]]);
            extra.set = new Set(["first", "second"]);
            extra.blob = new Blob([new Uint8Array([31, 32])], { type: "application/test" });
            extra.date = new Date("2025-01-02T03:04:05.000Z");
            records[6].unsupported = new Error("synthetic unsupported browser value");
            await seedSession(id, records, keys.map(key => ({ key, revision: 1 })));
            const beforeSession = await readSessionRecord(id);
            const frame = await child("without-locks=1");
            const other = frame.contentWindow.recoveryTestApi;
            try {
                let cursor = null;
                const found = [];
                do {
                    const page = await other.recoveryRescueBrowserPage("documents", cursor);
                    assert(page.keys.length <= 25, "Rescue loaded an unbounded page");
                    found.push(...page.keys.filter(key => typeof key === "string" && key.startsWith(`${id}/`)));
                    cursor = page.next;
                } while (cursor !== null);
                assert(found.length === keys.length, "Rescue omitted records or duplicated a page");
                assert(found.includes(`${id}/${keys[5]}`), "Malformed unindexed record was not listed");
                const result = await other.recoveryRescueBrowserRecord("documents", `${id}/${keys[5]}`);
                assert(result.archive && result.raw && !result.warning, "Malformed record was not exportable");
                const { header, parts } = await unpackRescueArchive(result.archive);
                assert(header.store === "documents" && header.key.value === `${id}/${keys[5]}`,
                    "Rescue archive lost the primary key");
                const bytesRef = archiveProperty(header, header.record, "bytes");
                const aliasRef = archiveProperty(header, header.record, "sameBytes");
                assert(bytesRef.ref === aliasRef.ref, "Rescue archive lost a shared reference");
                const bytesNode = header.nodes[bytesRef.ref];
                assert(bytesNode.type === "array-buffer", "Recovery bytes were not preserved as a buffer");
                assert(new Uint8Array(await parts[bytesNode.part].arrayBuffer()).join() === "5,0,255",
                    "Recovery archive changed raw record bytes");
                assert(new Uint8Array(await result.raw.arrayBuffer()).join() === "5,0,255",
                    "Raw rescue copy changed record bytes");
                const extraRef = archiveProperty(header, header.record, "extra");
                assert(archiveProperty(header, extraRef, "self").ref === extraRef.ref,
                    "Recovery archive did not preserve a cycle");
                assert(archiveProperty(header, extraRef, "negativeZero").value === "-0",
                    "Recovery archive changed negative zero");
                assert(archiveProperty(header, extraRef, "missing").type === "undefined",
                    "Recovery archive dropped an undefined value");
                const viewNode = header.nodes[archiveProperty(header, extraRef, "view").ref];
                assert(viewNode.type === "view" && viewNode.buffer.ref === bytesRef.ref
                    && viewNode.byteOffset === 1 && viewNode.byteLength === 2,
                "Recovery archive lost a binary view or its shared buffer");
                const longText = archiveProperty(header, extraRef, "longText");
                assert(longText.type === "string-part", "Long structured data was not chunked");
                const textLines = (await parts[longText.part].text()).split("\n").filter(Boolean);
                assert(textLines.map(line => JSON.parse(line)).join("") === extra.longText,
                    "Recovery archive changed long UTF-16 metadata");
                const mapNode = header.nodes[archiveProperty(header, extraRef, "map").ref];
                assert(mapNode.type === "map" && mapNode.entries[0][1].value === "NaN"
                    && mapNode.entries[1][1].value === "Infinity", "Recovery archive changed map values");
                const setNode = header.nodes[archiveProperty(header, extraRef, "set").ref];
                assert(setNode.type === "set" && setNode.entries.length === 2,
                    "Recovery archive changed set entries");
                const blobNode = header.nodes[archiveProperty(header, extraRef, "blob").ref];
                assert(blobNode.type === "blob" && blobNode.mediaType === "application/test"
                    && new Uint8Array(await parts[blobNode.part].arrayBuffer()).join() === "31,32",
                "Recovery archive changed a Blob");
                const dateNode = header.nodes[archiveProperty(header, extraRef, "date").ref];
                assert(dateNode.type === "date" && dateNode.time.value === extra.date.getTime(),
                    "Recovery archive changed a date");
                const partial = await other.recoveryRescueBrowserRecord("documents", `${id}/${keys[6]}`);
                assert(!partial.archive && partial.raw && partial.warning,
                    "An unsupported record was silently presented as a complete archive");
                assert(new Uint8Array(await partial.raw.arrayBuffer()).join() === "6,0,255",
                    "An unsupported record lost its available raw bytes");
                const sessionExport = await other.recoveryRescueBrowserRecord("sessions", id);
                assert(sessionExport.archive, "Session metadata was not exportable");
                assert(JSON.stringify(await readSessionRecord(id)) === JSON.stringify(beforeSession),
                    "Read-only rescue rewrote session metadata");
                const after = await readRecord(id, keys[5]);
                assert(after.key === "different-key" && !("session" in after)
                    && new Uint8Array(after.bytes).join() === "5,0,255",
                "Read-only rescue changed the malformed source record");
            } finally {
                frame.remove();
                await removeFixture(id, keys);
            }
        });
        await test("rescue reports denied storage without creating or changing sources", async () => {
            const prefix = "thinoda.vfs:/thinoda/data/recovery/";
            const key = `${prefix}${crypto.randomUUID()}/docs/denied.json`;
            const value = "original malformed envelope";
            localStorage.setItem(key, value);
            const frame = await child("deny-idb=1");
            const other = frame.contentWindow.recoveryTestApi;
            try {
                let denied = false;
                try { await other.recoveryRescueBrowserPage("documents"); } catch (_) { denied = true; }
                assert(denied, "Denied IndexedDB was reported as an empty successful scan");
                const legacy = await other.recoveryRescueLegacyArchive(prefix, key);
                assert(legacy.size > 0 && localStorage.getItem(key) === value,
                    "Denied IndexedDB prevented safe legacy rescue or changed the source");
                let invalid = false;
                try { await other.recoveryRescueLegacyArchive(prefix, "unrelated-key"); } catch (_) { invalid = true; }
                assert(invalid && localStorage.getItem(key) === value, "Rescue accepted a key outside recovery");
            } finally {
                frame.remove();
                localStorage.removeItem(key);
            }
        });
        await test("unindexed and sessionless recovery records remain gated and rescueable", async () => {
            const indexedSession = crypto.randomUUID();
            const missingSession = crypto.randomUUID();
            await seedSession(indexedSession, [{
                id: `${indexedSession}/unindexed`, key: "unindexed", revision: 1,
                deleted: false, bytes: new Uint8Array([21, 22]).buffer,
            }], []);
            await seedSession(missingSession, [{
                id: `${missingSession}/sessionless`, session: missingSession, key: "sessionless",
                revision: 1, deleted: false, bytes: new Uint8Array([23, 24]).buffer,
            }], [{ key: "sessionless", revision: 1 }]);
            await removeSessionRecord(missingSession);
            const originalSession = await readSessionRecord(indexedSession);
            const frame = await child();
            const other = frame.contentWindow.recoveryTestApi;
            try {
                await other.recoveryInitialize();
                for (let attempt = 0; attempt < 2; attempt++) {
                    const found = await other.recoveryDiscover(other.recoveryStartupDeadline());
                    assert(found.warning.includes("unindexed") && found.warning.includes("session"),
                        "Unindexed recovery data looked like a completed discovery");
                    assert(JSON.stringify(await readSessionRecord(indexedSession)) === JSON.stringify(originalSession),
                        "Retry rewrote an uninspectable session record");
                    const first = await other.recoveryRescueBrowserRecord(
                        "documents", `${indexedSession}/unindexed`,
                    );
                    const second = await other.recoveryRescueBrowserRecord(
                        "documents", `${missingSession}/sessionless`,
                    );
                    assert(new Uint8Array(await first.raw.arrayBuffer()).join() === "21,22"
                        && new Uint8Array(await second.raw.arrayBuffer()).join() === "23,24",
                    "Rescue changed an unindexed or sessionless source");
                    assert(!await readSessionRecord(missingSession),
                        "Retry created a replacement session for a missing original");
                }
            } finally {
                frame.remove();
                await removeFixture(indexedSession, ["unindexed"]);
                await removeFixture(missingSession, ["sessionless"]);
            }
        });
        await test("startup checks persistence without requesting consent", async () => {
            const frame = await child("observe-persistence=1");
            try {
                const result = JSON.parse(await frame.contentWindow.recoveryTestApi.recoveryInitialize());
                assert(result.persistent === false, "Non-persistent storage was reported as persistent");
                assert(frame.contentWindow.testPersistenceRequests === 0,
                    "Recovery startup requested persistence without a user action");
                const id = crypto.randomUUID();
                const other = frame.contentWindow.recoveryTestApi;
                await other.recoveryCreate(id);
                frame.contentWindow.testPersistenceGranted = true;
                const now = frame.contentWindow.Date.now;
                frame.contentWindow.Date.now = () => now() + 31_000;
                await eventually(() => JSON.parse(other.recoveryStatus(id)).persistent,
                    "refreshing explicitly granted persistence");
                assert(frame.contentWindow.testPersistenceRequests === 0,
                    "Refreshing status requested persistence");
                other.recoveryFinish(id);
            } finally {
                frame.remove();
            }
        });
        await test("unsupported strict durability is reported instead of hidden", async () => {
            const frame = await child();
            const id = crypto.randomUUID();
            const other = frame.contentWindow.recoveryTestApi;
            const prototype = frame.contentWindow.IDBDatabase.prototype;
            const original = prototype.transaction;
            try {
                prototype.transaction = function (stores, mode, options) {
                    if (mode === "readwrite" && options?.durability === "strict") {
                        throw new frame.contentWindow.TypeError("strict durability unsupported");
                    }
                    return original.call(this, stores, mode, options);
                };
                await other.recoveryInitialize();
                await other.recoveryCreate(id);
                await other.recoveryPersist(id, "doc", new Uint8Array([42]));
                const status = JSON.parse(other.recoveryStatus(id));
                assert(status.strict_durability === false, "Unsupported strict hint looked confirmed");
                assert(new Uint8Array((await readRecord(id, "doc")).bytes)[0] === 42,
                    "Fallback did not commit to IndexedDB");
            } finally {
                prototype.transaction = original;
                other.recoveryRelease(id);
                frame.remove();
            }
        });
        await test("unsupported Web Locks are an explicit failure", async () => {
            const key = `thinoda.vfs:/recovery-test/${crypto.randomUUID()}/docs/synthetic.json`;
            const original = JSON.stringify({ m: 0, l: 3, d: "AQID" });
            localStorage.setItem(key, original);
            const frame = await child("without-locks=1");
            try {
                let failure = "";
                try { await frame.contentWindow.recoveryTestApi.recoveryInitialize(); }
                catch (error) { failure = String(error); }
                assert(failure.includes("Web Locks"), `Expected Web Locks failure, got ${failure}`);
                failure = "";
                try { await frame.contentWindow.recoveryTestApi.recoveryClaimLegacy(); }
                catch (error) { failure = String(error); }
                assert(failure.includes("Web Locks"), `Legacy recovery was not fenced: ${failure}`);
                assert(localStorage.getItem(key) === original, "Missing locks changed the legacy source");
            } finally {
                frame.remove();
                localStorage.removeItem(key);
            }
        });
        await test("denied IndexedDB is an explicit failure", async () => {
            const frame = await child("deny-idb=1");
            let failure = "";
            try { await frame.contentWindow.recoveryTestApi.recoveryInitialize(); }
            catch (error) { failure = String(error); }
            assert(failure.includes("SecurityError"), `Expected denied-storage failure, got ${failure}`);
            frame.remove();
        });
        await test("denied legacy Web Locks are an explicit failure", async () => {
            const key = `thinoda.vfs:/recovery-test/${crypto.randomUUID()}/docs/synthetic.json`;
            const original = JSON.stringify({ m: 0, l: 3, d: "AQID" });
            localStorage.setItem(key, original);
            const frame = await child("deny-locks=1");
            try {
                let failure = "";
                try { await frame.contentWindow.recoveryTestApi.recoveryClaimLegacy(); }
                catch (error) { failure = String(error); }
                assert(failure.includes("SecurityError"), `Expected denied-lock failure, got ${failure}`);
                assert(localStorage.getItem(key) === original, "Denied locks changed the legacy source");
            } finally {
                frame.remove();
                localStorage.removeItem(key);
            }
        });
        await test("late database-open results are closed after startup timeout", async () => {
            const frame = await child("stall-open=1");
            try {
                let failed = false;
                try {
                    await frame.contentWindow.recoveryTestApi.recoveryInitialize(
                        frame.contentWindow.performance.now() + 40,
                    );
                }
                catch (error) { failed = String(error).includes("timed out"); }
                assert(failed, "The stalled database open did not time out");
                await eventually(() => frame.contentWindow.testLateDatabaseClosed, "late database close");
            } finally {
                frame.remove();
            }
        });
        await test("one startup deadline fences late lock results and permits retry", async () => {
            const id = crypto.randomUUID();
            await api.recoveryCreate(id);
            await api.recoveryPersist(id, "doc", new Uint8Array([6, 7]));
            assert(api.recoverySuspend(id) === "suspended", "Could not release test session");
            const stalled = await child("stall-locks=1");
            try {
                const other = stalled.contentWindow.recoveryTestApi;
                await other.recoveryInitialize();
                const deadline = stalled.contentWindow.performance.now() + 40;
                let failed = false;
                try { await other.recoveryDiscover(deadline); }
                catch (error) { failed = String(error).includes("timed out"); }
                assert(failed, "Discovery did not honor the shared deadline");
                failed = false;
                try { await other.recoveryClaimLegacy(deadline); }
                catch (error) { failed = String(error).includes("timed out"); }
                assert(failed, "A later phase got a fresh timeout");
                await sleep(250);
                await api.recoveryResumeAll();
                assert(api.recoveryHeld(id), "A late startup lock was retained");
                assert(new Uint8Array((await readRecord(id, "doc")).bytes).join() === "6,7",
                    "Timed-out discovery changed the source snapshot");
            } finally {
                stalled.remove();
            }
            assert(api.recoverySuspend(id) === "suspended", "Could not prepare retry");
            const retry = await child();
            try {
                const other = retry.contentWindow.recoveryTestApi;
                await other.recoveryInitialize();
                const found = (await other.recoveryDiscover(other.recoveryStartupDeadline())).sessions;
                const doc = found.find(session => session.id === id)?.docs.find(item => item.key === "doc");
                assert(doc && doc.bytes.join() === "6,7", "Retry did not discover the intact snapshot");
                await other.recoveryDiscard(id, "doc", doc.revision, undefined, doc.bytes);
                assert(await other.recoveryCompleteDiscard(id, "doc", doc.revision, undefined, doc.bytes) === "absent",
                    "The explicitly completed discard did not verify");
                other.recoveryRelease(id);
            } finally {
                api.recoveryRelease(id);
                retry.remove();
            }
        });
        await test("pagehide cancels an in-flight startup claim without leaking its lock", async () => {
            const frame = await child("stall-locks=1");
            try {
                const other = frame.contentWindow.recoveryTestApi;
                const pending = other.recoveryClaimLegacy(frame.contentWindow.performance.now() + 2_000);
                await sleep(30);
                other.recoveryPauseStartup();
                let failed = false;
                try { await pending; } catch (_) { failed = true; }
                assert(failed, "Hidden startup unexpectedly claimed a lock");
                await eventually(
                    () => api.recoveryClaimLegacy(),
                    "cancelled startup releasing the legacy lock",
                );
                api.recoveryRelease("legacy-localstorage");
                await other.recoveryResumeAll();
            } finally {
                frame.remove();
            }
        });
        await test("pagehide invalidates completed startup results across BFCache", async () => {
            const frame = await child();
            try {
                const other = frame.contentWindow.recoveryTestApi;
                const generation = other.recoveryStartupGeneration();
                assert(other.recoveryStartupIsCurrent(generation), "Visible generation was not current");
                other.recoveryPauseStartup();
                assert(!other.recoveryStartupIsCurrent(generation), "Hidden result remained current");
                await other.recoveryResumeAll();
                assert(!other.recoveryStartupIsCurrent(generation), "Old result became current after pageshow");
                assert(other.recoveryStartupIsCurrent(other.recoveryStartupGeneration()),
                    "The resumed generation was not available for retry");
            } finally {
                frame.remove();
            }
        });
        await test("an eviction warning survives an interrupted initialization", async () => {
            const frame = await child("simulate-eviction=1");
            try {
                const other = frame.contentWindow.recoveryTestApi;
                const pending = other.recoveryInitialize(other.recoveryStartupDeadline());
                await eventually(() => frame.contentWindow.testEvictionOpenCompleted,
                    "synthetic recreated database");
                other.recoveryPauseStartup();
                let interrupted = false;
                try { await pending; } catch (_) { interrupted = true; }
                assert(interrupted, "Hidden initialization unexpectedly completed");
                await other.recoveryResumeAll();
                const retry = JSON.parse(await other.recoveryInitialize(other.recoveryStartupDeadline()));
                assert(retry.warning.includes("cleared or evicted"),
                    "Retry silently forgot a possible loss of earlier snapshots");
            } finally {
                frame.remove();
            }
        });
        await test("pagehide fences late BFCache resume and permits another retry", async () => {
            const frame = await child("stall-locks=1");
            try {
                const other = frame.contentWindow.recoveryTestApi;
                await other.recoveryInitialize();
                const id = crypto.randomUUID();
                await other.recoveryCreate(id);
                await other.recoveryPersist(id, "doc", new Uint8Array([5]));
                assert(other.recoverySuspend(id) === "suspended", "Could not suspend BFCache session");
                const pending = other.recoveryResumeAll();
                await sleep(30);
                other.recoveryPauseStartup(true);
                await pending;
                assert(!other.recoveryHeld(id), "Late resume reactivated a hidden page");
                await other.recoveryResumeAll();
                assert(other.recoveryHeld(id), "A canceled resume was not retryable");
                assert(new Uint8Array((await readRecord(id, "doc")).bytes)[0] === 5,
                    "Canceled resume changed the committed snapshot");
                other.recoveryFinish(id);
                await eventually(() => {
                    try { other.recoveryStatus(id); return false; } catch (_) { return true; }
                }, "BFCache test cleanup");
            } finally {
                frame.remove();
            }
        });
        await test("BFCache retry reopens a closed database before checking ownership", async () => {
            const frame = await child();
            try {
                const other = frame.contentWindow.recoveryTestApi;
                await other.recoveryInitialize();
                const id = crypto.randomUUID();
                await other.recoveryCreate(id);
                const prototype = frame.contentWindow.IDBDatabase.prototype;
                const original = prototype.transaction;
                let connection;
                prototype.transaction = function (...args) {
                    connection = this;
                    return original.apply(this, args);
                };
                try {
                    await other.recoveryPersist(id, "doc", new Uint8Array([5, 6]));
                } finally {
                    prototype.transaction = original;
                }
                assert(connection, "Could not capture the recovery connection");
                assert(other.recoverySuspend(id) === "suspended", "Could not suspend the writer");
                connection.dispatchEvent(new frame.contentWindow.Event("close"));
                await other.recoveryResumeAll();
                assert(other.recoveryHeld(id), "The writer did not reopen and verify its session");
                await other.recoveryPersist(id, "doc", new Uint8Array([7]));
                assert(new Uint8Array((await readRecord(id, "doc")).bytes)[0] === 7,
                    "The resumed writer could not commit");
                other.recoveryFinish(id);
            } finally {
                frame.remove();
            }
        });
        await test("concurrent BFCache resumes do not mistake their own lock for another tab", async () => {
            const frame = await child("stall-locks=1");
            const other = frame.contentWindow.recoveryTestApi;
            const id = crypto.randomUUID();
            try {
                await other.recoveryInitialize();
                await other.recoveryCreate(id);
                await other.recoveryPersist(id, "doc", new Uint8Array([8]));
                assert(other.recoverySuspend(id) === "suspended", "Could not suspend the writer");
                await Promise.all([other.recoveryResumeAll(), other.recoveryResumeAll()]);
                const status = JSON.parse(other.recoveryStatus(id));
                assert(other.recoveryHeld(id) && !status.error,
                    `Concurrent resumes lost the writer: ${status.error}`);
                await other.recoveryPersist(id, "doc", new Uint8Array([9]));
                assert(new Uint8Array((await readRecord(id, "doc")).bytes)[0] === 9,
                    "The resumed writer could not commit");
                assert(other.recoveryFinish(id) === "finished", "Could not finish resumed writer");
            } finally {
                other.recoveryRelease(id);
                frame.remove();
            }
        });
        await test("oversized discovery and rescue retain every source byte", async () => {
            const id = crypto.randomUUID();
            const key = "oversized";
            const bytes = new Uint8Array(32 * 1024 * 1024 + 1);
            bytes[0] = 17;
            bytes[bytes.length - 1] = 29;
            await seedSession(id, [{
                id: `${id}/${key}`, session: id, key, revision: 1, deleted: false, bytes: bytes.buffer,
            }], [{ key, revision: 1 }]);
            const originalSession = await readSessionRecord(id);
            const frame = await child();
            const other = frame.contentWindow.recoveryTestApi;
            try {
                await other.recoveryInitialize();
                const found = await other.recoveryDiscover(other.recoveryStartupDeadline());
                assert(found.warning.includes("preview limit"), "Oversized scan looked complete");
                assert(!found.sessions.some(session => session.id === id),
                    "Oversized bytes were materialized as a complete preview");
                const source = await readRecord(id, key);
                assert(source?.bytes.byteLength === bytes.length, "Oversized source was removed");
                const actual = new Uint8Array(source.bytes);
                assert(actual[0] === 17 && actual.at(-1) === 29, "Oversized source changed");
                assert(await readSessionRecord(id), "Oversized session metadata was removed");
                assert(JSON.stringify(await readSessionRecord(id)) === JSON.stringify(originalSession),
                    "Uninspectable oversized session metadata was rewritten");
                const sessionBefore = await readSessionRecord(id);
                const rescue = await other.recoveryRescueBrowserRecord("documents", `${id}/${key}`);
                assert(rescue.archive && rescue.raw && rescue.raw.size === bytes.length,
                    "Oversized original was not available for rescue");
                const originalHash = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
                const rawHash = new Uint8Array(await crypto.subtle.digest("SHA-256", await rescue.raw.arrayBuffer()));
                assert(originalHash.join() === rawHash.join(), "Oversized raw rescue changed or truncated bytes");
                const { header, parts } = await unpackRescueArchive(rescue.archive);
                const bytesRef = archiveProperty(header, header.record, "bytes");
                const archived = parts[header.nodes[bytesRef.ref].part];
                const archiveHash = new Uint8Array(await crypto.subtle.digest("SHA-256", await archived.arrayBuffer()));
                assert(originalHash.join() === archiveHash.join(), "Oversized archive changed or truncated bytes");
                assert(JSON.stringify(await readSessionRecord(id)) === JSON.stringify(sessionBefore),
                    "Rescue changed the oversized session record");
                assert((await readRecord(id, key)).bytes.byteLength === bytes.length,
                    "Rescue removed the oversized original");
            } finally {
                other.recoveryRelease(id);
                frame.remove();
                await removeFixture(id, [key]);
            }
        });
        await test("rescue and repeated retry preserve original records and tombstones", async () => {
            const id = crypto.randomUUID();
            const empty = crypto.randomUUID();
            await seedSession(id, [{
                id: `${id}/live`, session: id, key: "live", revision: 3, deleted: false,
                bytes: new Uint8Array([9, 8, 7]).buffer,
            }], [{ key: "live", revision: 3 }]);
            await seedSession(empty, [{
                id: `${empty}/deleted`, session: empty, key: "deleted", revision: 4,
                deleted: true, bytes: null,
            }], []);
            const beforeSession = await readSessionRecord(id);
            const beforeEmpty = await readSessionRecord(empty);
            const frame = await child();
            const other = frame.contentWindow.recoveryTestApi;
            try {
                await other.recoveryInitialize();
                for (let attempt = 0; attempt < 2; attempt++) {
                    const found = await other.recoveryDiscover(other.recoveryStartupDeadline());
                    const live = found.sessions.find(session => session.id === id);
                    assert(live?.docs[0]?.bytes.join() === "9,8,7", "Retry lost the previous snapshot");
                    assert(new Uint8Array((await readRecord(id, "live")).bytes).join() === "9,8,7",
                        "Retry changed snapshot bytes");
                    const tombstone = await readRecord(empty, "deleted");
                    assert(tombstone?.deleted && tombstone.revision === 4 && tombstone.bytes === null,
                        "Retry removed or changed a tombstone record");
                    const current = await readSessionRecord(id);
                    const currentEmpty = await readSessionRecord(empty);
                    assert(current?.created === beforeSession.created
                        && JSON.stringify(current.manifest) === JSON.stringify(beforeSession.manifest)
                        && currentEmpty?.created === beforeEmpty.created
                        && JSON.stringify(currentEmpty.manifest) === JSON.stringify(beforeEmpty.manifest),
                    "Retry removed or altered recovery manifests");
                    // A claim's fencing token may change, but source document
                    // records and manifests are not deleted or rewritten.
                    other.recoveryRelease(id, live.lease);
                }
            } finally {
                frame.remove();
                await removeFixture(id, ["live"]);
                await removeFixture(empty, ["deleted"]);
            }
        });
        await test("tombstone and empty-session limits remain gated across reload", async () => {
            const suffix = crypto.randomUUID();
            const tombSession = `scan-${suffix}`;
            const liveSession = `z-live-${suffix}`;
            const writer = `reload-writer-${suffix}`;
            const emptySessions = Array.from({ length: 300 }, (_, index) =>
                `empty-${suffix}-${String(index).padStart(3, "0")}`);
            const tombstones = Array.from({ length: 1_300 }, (_, index) => {
                const key = `dead${String(index).padStart(4, "0")}`;
                return {
                    id: `${tombSession}/${key}`, session: tombSession, key,
                    revision: index + 1, deleted: true, bytes: null,
                };
            });
            const row = (id, manifest = []) => ({
                id, token: "synthetic-abandoned-writer", created: 1, manifest,
            });
            const live = {
                id: `${liveSession}/doc`, session: liveSession, key: "doc", revision: 1,
                deleted: false, bytes: new Uint8Array([0, 255, 42]).buffer,
            };
            const ids = [tombSession, liveSession, writer, ...emptySessions];
            await seedBatch([
                row(tombSession), ...emptySessions.map(id => row(id)),
                row(liveSession, [{ key: "doc", revision: 1 }]),
            ], [...tombstones, live]);
            const beforeSession = await readSessionRecord(tombSession);
            const beforeEmpty = await readSessionRecord(emptySessions[150]);
            const frame = await child();
            let other = frame.contentWindow.recoveryTestApi;
            try {
                await other.recoveryInitialize();
                const first = await discover(other);
                assert(first.warning.includes("preview limit"),
                    "A limited tombstone scan looked complete");
                for (const session of first.sessions) other.recoveryRelease(session.id, session.lease);
                const reloaded = new Promise(resolve => frame.addEventListener("load", resolve, { once: true }));
                frame.contentWindow.location.reload();
                await reloaded;
                other = await eventually(() => frame.contentWindow.recoveryTestApi, "reloaded scan module");
                await other.recoveryInitialize();
                await other.recoveryCreate(writer);
                const next = await discover(other);
                assert(next.warning.includes("preview limit"),
                    "Reload hid the unresolved tombstone and session limits");
                assert(new Uint8Array((await readRecord(liveSession, "doc")).bytes).join() === "0,255,42",
                    "The limited scan changed the live source");
                assert(JSON.stringify(await readSessionDocuments(tombSession)) === JSON.stringify(tombstones),
                    "The scan changed or removed a tombstone");
                assert(JSON.stringify(await readSessionRecord(tombSession)) === JSON.stringify(beforeSession)
                    && JSON.stringify(await readSessionRecord(emptySessions[150])) === JSON.stringify(beforeEmpty),
                "The scan changed an empty or tombstone-only session");
                for (const session of next.sessions) other.recoveryRelease(session.id, session.lease);
                assert(other.recoveryFinish(writer) === "finished", "Could not finish the empty test writer");
            } finally {
                other.recoveryRelease(writer);
                frame.remove();
                await removeBatch(ids, [...tombstones.map(record => record.id), live.id]);
            }
        });
        await test("an uncoordinated snapshot inserted between retries cannot be silently skipped", async () => {
            const id = `000-scan-${crypto.randomUUID()}`;
            const tombstones = Array.from({ length: 1_000 }, (_, index) => {
                const key = `dead${String(index).padStart(4, "0")}`;
                return {
                    id: `${id}/${key}`, session: id, key, revision: index + 1,
                    deleted: true, bytes: null,
                };
            });
            await seedBatch([{
                id, token: "synthetic-abandoned-writer", created: 1, manifest: [],
            }], tombstones);
            const frame = await child();
            const other = frame.contentWindow.recoveryTestApi;
            try {
                await other.recoveryInitialize();
                const first = await other.recoveryDiscover();
                assert(first.warning.includes("preview limit"), "The first scan was unexpectedly complete");
                for (const session of first.sessions) other.recoveryRelease(session.id, session.lease);
                const connection = await db();
                try {
                    // This writer deliberately does not participate in any
                    // application-defined checkpoint or epoch protocol.
                    await new Promise((resolve, reject) => {
                        const tx = connection.transaction(["sessions", "documents"], "readwrite");
                        const sessions = tx.objectStore("sessions");
                        const request = sessions.get(id);
                        request.onsuccess = () => {
                            const row = request.result;
                            row.manifest.push({ key: "000-new", revision: 1 });
                            sessions.put(row);
                        };
                        tx.objectStore("documents").put({
                            id: `${id}/000-new`, session: id, key: "000-new", revision: 1,
                            deleted: false, bytes: new Uint8Array([51, 52]).buffer,
                        });
                        tx.oncomplete = resolve;
                        tx.onabort = () => reject(tx.error);
                    });
                } finally {
                    connection.close();
                }
                const found = await discover(other);
                const recovered = found.sessions.find(session => session.id === id);
                assert(found.warning || recovered?.docs.some(doc =>
                    doc.key === "000-new" && doc.bytes.join() === "51,52"),
                "A successful retry silently skipped the newly inserted snapshot");
                assert(new Uint8Array((await readRecord(id, "000-new")).bytes).join() === "51,52",
                    "Retry changed the newly inserted snapshot");
                assert(JSON.stringify((await readSessionDocuments(id)).filter(record => record.deleted))
                    === JSON.stringify(tombstones), "Retry changed retained tombstones");
                for (const session of found.sessions) other.recoveryRelease(session.id, session.lease);
            } finally {
                other.recoveryRelease(id);
                frame.remove();
                await removeBatch([id], [...tombstones.map(record => record.id), `${id}/000-new`]);
            }
        });
        await test("an old version-1 client cannot reopen the upgraded recovery store for writing", async () => {
            const outcome = await new Promise(resolve => {
                const request = indexedDB.open("thinoda.recovery.v1", 1);
                request.onerror = () => resolve(request.error?.name);
                request.onsuccess = () => {
                    request.result.close();
                    resolve("opened");
                };
            });
            assert(outcome === "VersionError", `An old recovery client unexpectedly ${outcome}`);
        });
        await test("an orphan claimed by another tab remains an unresolved recovery decision", async () => {
            const id = `claimed-${crypto.randomUUID()}`;
            const record = {
                id: `${id}/doc`, session: id, key: "doc", revision: 1,
                deleted: false, bytes: new Uint8Array([61]).buffer,
            };
            await seedSession(id, [record], [{ key: "doc", revision: 1 }]);
            const first = await child();
            const second = await child();
            const owner = first.contentWindow.recoveryTestApi;
            const other = second.contentWindow.recoveryTestApi;
            try {
                await owner.recoveryInitialize();
                await other.recoveryInitialize();
                const claimed = (await discover(owner)).sessions.find(session => session.id === id);
                assert(claimed?.docs[0].bytes[0] === 61, "The first tab did not claim the original");
                await eventually(async () => {
                    const found = await other.recoveryDiscover();
                    for (const session of found.sessions) other.recoveryRelease(session.id, session.lease);
                    return found.warning.includes("in use in another tab");
                }, "warning about the other tab's unresolved original");
                assert(new Uint8Array((await readRecord(id, "doc")).bytes)[0] === 61,
                    "The blocked scan changed the claimed original");
                owner.recoveryRelease(id, claimed.lease);
                const recovered = await eventually(async () => {
                    const found = await discover(other);
                    const session = found.sessions.find(item => item.id === id);
                    if (session) return session;
                    for (const item of found.sessions) other.recoveryRelease(item.id, item.lease);
                    return null;
                }, "recovery after the other tab releases its claim");
                assert(recovered.docs[0].bytes[0] === 61, "Retry changed the claimed original");
                other.recoveryRelease(id, recovered.lease);
            } finally {
                owner.recoveryRelease(id);
                other.recoveryRelease(id);
                first.remove();
                second.remove();
                await removeFixture(id, ["doc"]);
            }
        });
        await test("snapshot count limits cannot silently delete unscanned originals", async () => {
            const id = crypto.randomUUID();
            const keys = Array.from({ length: 129 }, (_, index) => `doc${String(index).padStart(3, "0")}`);
            await seedSession(id, keys.map((key, index) => ({
                id: `${id}/${key}`, session: id, key, revision: 1, deleted: false,
                bytes: new Uint8Array([index]).buffer,
            })), keys.map(key => ({ key, revision: 1 })));
            const frame = await child();
            const other = frame.contentWindow.recoveryTestApi;
            try {
                await other.recoveryInitialize();
                const found = await other.recoveryDiscover(other.recoveryStartupDeadline());
                const session = found.sessions.find(item => item.id === id);
                assert(found.warning.includes("preview limit"), "Truncated scan looked complete");
                assert(session?.docs.length === 128, "Unexpected bounded preview size");
                assert((await readRecord(id, keys.at(-1)))?.bytes.byteLength === 1,
                    "Unscanned original was removed");
                let rejected = false;
                try { await other.recoveryDiscard(id, keys[0], 1, undefined, new Uint8Array([0])); }
                catch (_) { rejected = true; }
                assert(rejected, "Incomplete metadata allowed destructive discard");
                assert(await readRecord(id, keys[0]), "A rejected discard removed the source");
            } finally {
                other.recoveryRelease(id);
                frame.remove();
                await removeFixture(id, keys);
            }
        });
        await test("malformed record identities cannot masquerade as another snapshot", async () => {
            const id = crypto.randomUUID();
            await seedSession(id, [
                { id: `${id}/valid`, session: id, key: "valid", revision: 1, deleted: false,
                    bytes: new Uint8Array([41]).buffer },
                { id: `${id}/bad`, session: id, key: "valid", revision: 1, deleted: false,
                    bytes: new Uint8Array([42]).buffer },
            ], [{ key: "valid", revision: 1 }]);
            const frame = await child();
            const other = frame.contentWindow.recoveryTestApi;
            try {
                await other.recoveryInitialize();
                const found = await other.recoveryDiscover(other.recoveryStartupDeadline());
                const session = found.sessions.find(item => item.id === id);
                assert(found.warning.includes("malformed"), "Malformed source was not reported");
                assert(session?.docs.length === 1 && session.docs[0].bytes[0] === 41,
                    "Malformed identity was exposed as a different original");
                let rejected = false;
                try { await other.recoveryDiscard(id, "bad", 1, undefined, new Uint8Array([42])); }
                catch (_) { rejected = true; }
                assert(rejected, "Malformed source was accepted for discard");
                assert(new Uint8Array((await readRecord(id, "valid")).bytes)[0] === 41,
                    "Malformed discard removed the valid original");
                assert(new Uint8Array((await readRecord(id, "bad")).bytes)[0] === 42,
                    "Malformed original was not retained");
            } finally {
                other.recoveryRelease(id);
                frame.remove();
                await removeFixture(id, ["valid", "bad"]);
            }
        });

        await test("stale orphan claims cannot release or discard a newer claim", async () => {
            const id = crypto.randomUUID();
            await seedSession(id, [{
                id: `${id}/doc`, session: id, key: "doc", revision: 1, deleted: false,
                bytes: new Uint8Array([37]).buffer,
            }], [{ key: "doc", revision: 1 }]);
            const frame = await child();
            const other = frame.contentWindow.recoveryTestApi;
            try {
                await other.recoveryInitialize();
                const find = () => other.recoveryDiscover(other.recoveryStartupDeadline())
                    .then(result => result.sessions.find(session => session.id === id));
                const first = await eventually(find, "first recovery claim");
                assert(typeof first.lease === "string" && first.lease, "Missing first claim token");
                const overlapping = await other.recoveryDiscover(other.recoveryStartupDeadline());
                assert(overlapping.warning.includes("earlier attempt"),
                    "A still-owned orphan looked like a complete overlapping scan");
                assert(other.recoveryHeld(id, first.lease), "An overlapping scan released the first claim");
                other.recoveryRelease(id, first.lease);
                const second = await eventually(find, "replacement recovery claim");
                assert(second.lease && first.lease !== second.lease, "A new claim reused the old token");
                other.recoveryRelease(id, first.lease);
                assert(!other.recoveryHeld(id, first.lease) && other.recoveryHeld(id, second.lease),
                    "A stale release affected the replacement claim");
                let rejected = false;
                try { await other.recoveryDiscard(id, "doc", 1, first.lease, new Uint8Array([37])); }
                catch (_) { rejected = true; }
                assert(rejected, "A stale claim was allowed to discard the original");
                rejected = false;
                try { await other.recoveryInspect(id, "doc", 1, first.lease, new Uint8Array([37])); }
                catch (_) { rejected = true; }
                assert(rejected, "A stale claim was allowed to inspect the replacement claim");
                assert(new Uint8Array((await readRecord(id, "doc")).bytes)[0] === 37,
                    "A stale claim changed the original");
                assert(await other.recoveryInspect(id, "doc", 1, second.lease, new Uint8Array([37])) === "present",
                    "The replacement claim could not inspect the intact original");
            } finally {
                other.recoveryRelease(id);
                frame.remove();
                await removeFixture(id, ["doc"]);
            }
        });
        await test("stale legacy claims cannot release a newer claim", async () => {
            const id = "legacy-localstorage";
            let second;
            try {
                const first = await api.recoveryClaimLegacy();
                assert(typeof first === "string" && first, "Missing first legacy claim token");
                api.recoveryRelease(id, first);
                second = await eventually(() => api.recoveryClaimLegacy(), "replacement legacy claim");
                assert(first !== second, "A new legacy claim reused the old token");
                api.recoveryRelease(id, first);
                assert(!api.recoveryHeld(id, first) && api.recoveryHeld(id, second),
                    "A stale legacy release affected the replacement claim");
            } finally {
                if (second) api.recoveryRelease(id, second);
            }
        });
        await test("a stale writer token cannot finish a live writer", async () => {
            const id = crypto.randomUUID();
            const token = await api.recoveryCreate(id);
            try {
                await api.recoveryPersist(id, "doc", new Uint8Array([38]));
                let rejected = false;
                try { api.recoveryFinish(id, `${token}-stale`); } catch (_) { rejected = true; }
                assert(rejected && api.recoveryHeld(id, token), "A stale writer token ended the writer");
                assert(new Uint8Array((await readRecord(id, "doc")).bytes)[0] === 38,
                    "A stale finish changed the committed snapshot");
                assert(api.recoveryFinish(id, token) === "finished", "Current writer could not finish");
            } finally {
                api.recoveryRelease(id, token);
                await removeFixture(id, ["doc"]);
            }
        });

        const previous = crypto.randomUUID();
        await api.recoveryCreate(previous);
        await api.recoveryPersist(previous, "same-document", new Uint8Array([1]));
        assert(api.recoverySuspend(previous) === "suspended", "Could not release previous session");
        api.recoveryRelease(previous);
        const writer = crypto.randomUUID();
        await api.recoveryCreate(writer);
        await test("live writer remains separate during discovery, retry, and handoff", async () => {
            await api.recoveryPersist(writer, "live-document", new Uint8Array([2]));
            const discovery = api.recoveryDiscover(api.recoveryStartupDeadline());
            api.recoveryQueue(writer, "live-document", new Uint8Array([3]));
            const found = (await discovery).sessions;
            assert(!found.some(session => session.id === writer), "Fresh live writer became an orphan");
            let older = found.find(session => session.id === previous);
            assert(older?.docs[0]?.bytes[0] === 1, "Previous recovery changed during live editing");
            await idle(writer);
            assert(new Uint8Array((await readRecord(writer, "live-document")).bytes)[0] === 3,
                "Discovery overwrote the live edit");
            api.recoveryRelease(previous);
            older = await eventually(async () =>
                (await api.recoveryDiscover(api.recoveryStartupDeadline())).sessions
                    .find(session => session.id === previous), "retrying previous recovery");
            assert(older?.docs[0]?.bytes[0] === 1, "Retry changed the previous snapshot");
            // Explicitly simulate a separate-copy handoff; UI authorization
            // and same-file isolation are checked in the actual app.
            await api.recoveryPersist(writer, "separate-restored-copy", older.docs[0].bytes);
            await api.recoveryDiscard(previous, older.docs[0].key, older.docs[0].revision,
                undefined, older.docs[0].bytes);
            assert(await api.recoveryCompleteDiscard(previous, older.docs[0].key, older.docs[0].revision,
                undefined, older.docs[0].bytes) === "absent", "Could not complete the requested discard");
            api.recoveryRelease(previous);
            assert(new Uint8Array((await readRecord(writer, "live-document")).bytes)[0] === 3,
                "Handoff replaced the live document");
        });
        await test("a dirty BFCache writer stays owned even before its next queued snapshot", async () => {
            const frame = await child();
            try {
                const other = frame.contentWindow.recoveryTestApi;
                await other.recoveryInitialize();
                assert(api.recoveryPauseStartup(true) === "pending", "Dirty writer was released");
                assert(api.recoveryHeld(writer), "Dirty writer lost its lock on pagehide");
                const found = (await other.recoveryDiscover()).sessions;
                assert(!found.some(session => session.id === writer), "Dirty live writer became an orphan");
                await api.recoveryResumeAll();
                api.recoveryQueue(writer, "live-document", new Uint8Array([4]));
                await idle(writer);
                assert(new Uint8Array((await readRecord(writer, "live-document")).bytes)[0] === 4,
                    "Live writer could not resume after BFCache");
            } finally {
                frame.remove();
            }
        });
        await test("legacy migration requires exclusive ownership across tabs and BFCache", async () => {
            const frame = await child();
            const other = frame.contentWindow.recoveryTestApi;
            assert(await api.recoveryClaimLegacy(), "Could not claim legacy snapshots");
            assert(!(await other.recoveryClaimLegacy()), "Two tabs claimed legacy snapshots");
            assert(api.recoverySuspend(writer) === "suspended", "Could not suspend test page");
            assert(await other.recoveryClaimLegacy(), "Released legacy snapshots were not claimable");
            await api.recoveryResumeAll();
            assert(!api.recoveryHeld("legacy-localstorage"),
                "A resumed page retained a legacy lease claimed by another tab");
            other.recoveryRelease("legacy-localstorage");
            api.recoveryRelease("legacy-localstorage");
            frame.remove();
        });
        await test("document migration and recovery share quota without sharing sources", async () => {
            const suffix = crypto.randomUUID();
            const documentId = `/Documents/recovery-migration-${suffix}.thinoda`;
            const documentKey = `thinoda.vfs:${documentId}`;
            const recoveryKey = `thinoda.vfs:/thinoda/data/recovery/${suffix}/docs/legacy.json`;
            const documentEnvelope = JSON.stringify({ m: 0, l: 3, d: "AQID" });
            const recoveryEnvelope = JSON.stringify({ m: 0, l: 3, d: "BAUG" });
            localStorage.setItem(documentKey, documentEnvelope);
            localStorage.setItem(recoveryKey, recoveryEnvelope);
            const storage = await import("../../../../../../../crates/thinoda-platform/src/browser_storage.js");
            try {
                const report = JSON.parse(await storage.initializeDocuments());
                assert(report.documents.includes(documentId), "Legacy document was not migrated");
                assert((await storage.readDocument(documentId)).bytes.join() === "1,2,3",
                    "Migrated document bytes changed");
                assert(localStorage.getItem(documentKey) === documentEnvelope,
                    "Document migration removed the original");
                assert(localStorage.getItem(recoveryKey) === recoveryEnvelope,
                    "Document migration changed legacy recovery bytes");

                await api.recoveryPersist(writer, "migration-quota", new Uint8Array([71]));
                const transaction = IDBDatabase.prototype.transaction;
                IDBDatabase.prototype.transaction = function (stores, mode, options) {
                    if (this.name === "thinoda.recovery.v1" && mode === "readwrite") {
                        throw new DOMException("synthetic shared quota", "QuotaExceededError");
                    }
                    return transaction.call(this, stores, mode, options);
                };
                try {
                    let failed = false;
                    try { await api.recoveryPersist(writer, "migration-quota", new Uint8Array([72])); }
                    catch (_) { failed = true; }
                    assert(failed, "Recovery quota failure was reported as success");
                } finally {
                    IDBDatabase.prototype.transaction = transaction;
                }
                assert(new Uint8Array((await readRecord(writer, "migration-quota")).bytes)[0] === 71,
                    "Quota failure replaced the earlier recovery snapshot");
                assert((await storage.readDocument(documentId)).bytes.join() === "1,2,3",
                    "Recovery quota failure altered the migrated document");
                assert(localStorage.getItem(recoveryKey) === recoveryEnvelope,
                    "Recovery quota failure altered legacy recovery");
                await api.recoveryRetryWriter(writer);
            } finally {
                localStorage.removeItem(documentKey);
                localStorage.removeItem(recoveryKey);
            }
        });
        await test("large raw snapshot commits and reads back", async () => {
            const bytes = new Uint8Array(12_318_011);
            bytes.fill(0x5a);
            bytes[bytes.length - 1] = 0x27;
            await api.recoveryPersist(writer, "large", bytes);
            const record = await readRecord(writer, "large");
            assert(record.bytes.byteLength === bytes.length, "Large snapshot length changed");
            const read = new Uint8Array(record.bytes);
            assert(read[0] === 0x5a && read.at(-1) === 0x27, "Large snapshot contents changed");
        });
        await test("failed quota write preserves the previous committed snapshot", async () => {
            await api.recoveryPersist(writer, "quota", new Uint8Array([1, 2, 3]));
            const original = IDBDatabase.prototype.transaction;
            IDBDatabase.prototype.transaction = function (stores, mode, options) {
                if (mode === "readwrite") throw new DOMException("test quota", "QuotaExceededError");
                return original.call(this, stores, mode, options);
            };
            try {
                api.recoveryQueue(writer, "quota", new Uint8Array([9, 9]));
                const failed = await eventually(() => {
                    const state = JSON.parse(api.recoveryStatus(writer));
                    return state.error && state;
                }, "quota error");
                assert(failed.error.includes("full"), `Quota failure not surfaced: ${failed.error}`);
                let suspendFailed = false;
                try { api.recoverySuspend(writer); } catch (_) { suspendFailed = true; }
                assert(suspendFailed && JSON.parse(api.recoveryStatus(writer)).active,
                    "Failed suspension released the live recovery lease");
            } finally {
                IDBDatabase.prototype.transaction = original;
            }
            const prior = await readRecord(writer, "quota");
            assert(new Uint8Array(prior.bytes).join() === "1,2,3", "Failed write replaced the prior snapshot");
            await api.recoveryRetryWriter(writer);
            await eventually(() => {
                const state = JSON.parse(api.recoveryStatus(writer));
                return !state.pending && !state.error;
            }, "retrying the retained quota operation");
            assert(new Uint8Array((await readRecord(writer, "quota")).bytes).join() === "9,9",
                "Explicit retry did not commit the retained operation");
            api.recoveryQueue(writer, "quota", new Uint8Array([4, 5]));
            await idle(writer);
            const retried = await readRecord(writer, "quota");
            assert(new Uint8Array(retried.bytes).join() === "4,5", "Retry did not commit");
        });
        await test("a failed handoff does not alter legacy localStorage bytes", async () => {
            // This checks the storage backend's failure contract. The actual
            // Rust migration/UI flow requires a separate application test.
            const key = `thinoda.vfs:/recovery-test/${crypto.randomUUID()}/docs/synthetic.json`;
            const original = JSON.stringify({ m: 0, l: 3, d: "AQID" });
            localStorage.setItem(key, original);
            assert(await api.recoveryClaimLegacy(), "Could not claim legacy snapshots");
            const transaction = IDBDatabase.prototype.transaction;
            IDBDatabase.prototype.transaction = function (stores, mode, options) {
                if (mode === "readwrite") throw new DOMException("test quota", "QuotaExceededError");
                return transaction.call(this, stores, mode, options);
            };
            try {
                let failed = false;
                try { await api.recoveryPersist(writer, "legacy-handoff", new Uint8Array([1, 2, 3])); }
                catch (_) { failed = true; }
                assert(failed, "Injected migration write unexpectedly succeeded");
                assert(localStorage.getItem(key) === original, "Failed handoff changed the legacy source");
            } finally {
                IDBDatabase.prototype.transaction = transaction;
                api.recoveryRelease("legacy-localstorage");
                localStorage.removeItem(key);
            }
            // Clear the injected failure before exercising the next case.
            await api.recoveryRetryWriter(writer);
            assert(!JSON.parse(api.recoveryStatus(writer)).error, "Writable retry did not clear the error");
            await api.recoveryPersist(writer, "after-legacy-failure", new Uint8Array([0]));
        });
        await test("newer writes and a delete fence older queued writes", async () => {
            api.recoveryQueue(writer, "race", new Uint8Array([1]));
            assert(api.recoverySuspend(writer) === "pending", "Pending write was treated as flushed");
            api.recoveryQueue(writer, "race", new Uint8Array([2]));
            await idle(writer);
            assert(new Uint8Array((await readRecord(writer, "race")).bytes)[0] === 2, "Older revision won");
            api.recoveryQueue(writer, "race", new Uint8Array([3]));
            api.recoveryRemove(writer, "race");
            await idle(writer);
            await sleep(100);
            const record = await readRecord(writer, "race");
            assert(record.deleted === true && record.bytes === null, "A pending write resurrected a deleted snapshot");
        });
        await test("a superseded handoff cannot report an obsolete commit as current", async () => {
            const id = crypto.randomUUID();
            await api.recoveryCreate(id);
            const transaction = IDBDatabase.prototype.transaction;
            let deliverCompletion;
            try {
                let intercepted = false;
                IDBDatabase.prototype.transaction = function (stores, mode, options) {
                    const tx = transaction.call(this, stores, mode, options);
                    if (mode !== "readwrite" || intercepted) return tx;
                    intercepted = true;
                    return new Proxy(tx, {
                        get(target, property) {
                            const value = Reflect.get(target, property, target);
                            return typeof value === "function" ? value.bind(target) : value;
                        },
                        set(target, property, value) {
                            if (property === "oncomplete") {
                                target.oncomplete = event => {
                                    deliverCompletion = () => value.call(target, event);
                                };
                                return true;
                            }
                            return Reflect.set(target, property, value, target);
                        },
                    });
                };
                const outcome = api.recoveryPersist(id, "doc", new Uint8Array([19]))
                    .then(() => "committed", () => "superseded");
                await eventually(() => deliverCompletion, "delayed handoff completion");
                api.recoveryRemove(id, "doc");
                IDBDatabase.prototype.transaction = transaction;
                deliverCompletion();
                assert(await outcome === "superseded", "An obsolete handoff was reported as current");
                await idle(id);
                assert((await readRecord(id, "doc")).deleted === true, "The newer tombstone did not win");
            } finally {
                IDBDatabase.prototype.transaction = transaction;
                api.recoveryRelease(id);
            }
        });
        await test("acknowledgements follow exact per-document committed revisions", async () => {
            const id = crypto.randomUUID();
            await api.recoveryCreate(id);
            try {
                const firstA = api.recoveryQueue(id, "a", new Uint8Array([1]));
                assert(api.recoveryCurrent(id, "a", firstA), "A queued put was not marked current");
                assert(!api.recoveryAcknowledged(id, "a", firstA), "A queued write was acknowledged before commit");
                assert(api.recoveryHasUnconfirmedWork(), "Beforeunload could miss an accepted write");
                await idle(id);
                assert(api.recoveryAcknowledged(id, "a", firstA), "Committed A was not acknowledged");
                const firstB = api.recoveryQueue(id, "b", new Uint8Array([2]));
                assert(api.recoveryAcknowledged(id, "a", firstA), "New B invalidated A's acknowledgement");
                assert(!api.recoveryAcknowledged(id, "b", firstB), "Pending B was acknowledged early");
                await idle(id);
                assert(api.recoveryAcknowledged(id, "a", firstA), "A lost its acknowledgement after B committed");
                assert(api.recoveryAcknowledged(id, "b", firstB), "Committed B was not acknowledged");
                const nextA = api.recoveryQueue(id, "a", new Uint8Array([3]));
                assert(!api.recoveryAcknowledged(id, "a", firstA), "Superseded A stayed acknowledged");
                assert(!api.recoveryAcknowledged(id, "a", nextA), "New A was acknowledged before commit");
                assert(api.recoveryAcknowledged(id, "b", firstB), "New A invalidated B's acknowledgement");
                await idle(id);
                assert(api.recoveryAcknowledged(id, "a", nextA), "New A was not acknowledged after commit");
                api.recoveryRemove(id, "a");
                assert(!api.recoveryCurrent(id, "a", nextA), "A tombstone left the old put current");
                assert(!api.recoveryAcknowledged(id, "a", nextA), "A deleted snapshot stayed acknowledged");
                await idle(id);
                const undoA = api.recoveryQueue(id, "a", new Uint8Array([3]));
                assert(api.recoveryCurrent(id, "a", undoA), "Undo could not requeue the old document watermark");
                await idle(id);
                assert(api.recoveryAcknowledged(id, "a", undoA), "Undo's replacement was not acknowledged");
                assert(api.recoveryFinish(id) === "finished", "Could not release acknowledgement writer");
                assert(!api.recoveryAcknowledged(id, "b", firstB), "A released writer retained an acknowledgement");
            } finally {
                api.recoveryRelease(id);
            }
        });
        await test("a failed document does not starve or invalidate another document", async () => {
            const id = crypto.randomUUID();
            await api.recoveryCreate(id);
            const transaction = IDBDatabase.prototype.transaction;
            try {
                IDBDatabase.prototype.transaction = function (stores, mode, options) {
                    if (mode === "readwrite") throw new DOMException("test quota", "QuotaExceededError");
                    return transaction.call(this, stores, mode, options);
                };
                const first = api.recoveryQueue(id, "a", new Uint8Array([21]));
                await eventually(() => JSON.parse(api.recoveryStatus(id)).error, "first document failure");
                IDBDatabase.prototype.transaction = transaction;
                const second = api.recoveryQueue(id, "b", new Uint8Array([22]));
                await eventually(() => api.recoveryAcknowledged(id, "b", second), "independent second document");
                const duringFailure = JSON.parse(api.recoveryStatus(id));
                assert(duringFailure.error && duringFailure.pending,
                    "Another document's success hid the unresolved failure");
                assert(!api.recoveryAcknowledged(id, "a", first), "Failed first document was acknowledged");
                await eventually(() => {
                    api.recoveryStatus(id);
                    return api.recoveryAcknowledged(id, "a", first);
                }, "retry of first document");
                assert(api.recoveryAcknowledged(id, "b", second), "Retry invalidated the second document");
            } finally {
                IDBDatabase.prototype.transaction = transaction;
                api.recoveryRelease(id);
            }
        });
        await test("cross-tab claim fences a suspended page", async () => {
            const frame = await child();
            const other = frame.contentWindow.recoveryTestApi;
            await other.recoveryInitialize();
            let found = (await other.recoveryDiscover()).sessions;
            assert(!found.some(session => session.id === writer), "A live session was claimed");
            assert(api.recoverySuspend(writer) === "suspended", "Idle session was not suspended");
            found = (await other.recoveryDiscover()).sessions;
            const claimed = found.find(session => session.id === writer);
            assert(claimed, "Released session was not discoverable");
            const large = claimed.docs.find(doc => doc.key === "large");
            assert(large?.bytes.length === 12_318_011, "Claimed large snapshot is missing");
            await api.recoveryResumeAll();
            let rejected = false;
            try { api.recoveryQueue(writer, "large", new Uint8Array([7])); }
            catch (_) { rejected = true; }
            assert(rejected, "Former owner could write after another tab claimed its session");
            await other.recoveryDiscard(writer, "large", large.revision, undefined, large.bytes);
            assert(await other.recoveryInspect(writer, "large", large.revision, undefined, large.bytes) === "pending",
                "The pending discard did not retain the original");
            assert(await other.recoveryCompleteDiscard(writer, "large", large.revision, undefined, large.bytes) === "absent",
                "Could not complete the orphan discard");
            assert((await readRecord(writer, "large"))?.discardReceipt?.version === 1,
                "Completed orphan discard did not leave its receipt");
            other.recoveryRelease(writer);
            frame.remove();
        });
        await test("unexpected session removal is reported, not recreated", async () => {
            const evicted = crypto.randomUUID();
            await api.recoveryCreate(evicted);
            await api.recoveryPersist(evicted, "doc", new Uint8Array([8]));
            await removeSessionRecord(evicted);
            api.recoveryQueue(evicted, "doc", new Uint8Array([9]));
            const failed = await eventually(() => {
                const state = JSON.parse(api.recoveryStatus(evicted));
                return state.error && state;
            }, "eviction failure");
            assert(failed.error.includes("lost") || failed.error.includes("evicted"), failed.error);
            assert(new Uint8Array((await readRecord(evicted, "doc")).bytes)[0] === 8, "Eviction recreated the snapshot");
            api.recoveryRelease(evicted);
        });
        await test("a missing live snapshot is reported instead of silently recreated", async () => {
            const id = crypto.randomUUID();
            await api.recoveryCreate(id);
            await api.recoveryPersist(id, "missing", new Uint8Array([31]));
            await api.recoveryPersist(id, "survivor", new Uint8Array([32]));
            await removeDocumentRecord(id, "missing");
            api.recoveryQueue(id, "missing", new Uint8Array([33]));
            const failed = await eventually(() => {
                const state = JSON.parse(api.recoveryStatus(id));
                return state.error && state;
            }, "missing live snapshot failure");
            assert(failed.error.includes("missing") || failed.error.includes("inconsistent"), failed.error);
            assert(!(await readRecord(id, "missing")), "Missing record was silently recreated");
            assert(new Uint8Array((await readRecord(id, "survivor")).bytes)[0] === 32,
                "Another committed snapshot was altered");
            const row = await readSessionRecord(id);
            assert(row.manifest.some(entry => entry.key === "missing"), "Missing entry was hidden");
            api.recoveryRelease(id);
        });
        await test("invalid live metadata cannot overwrite previous committed data", async () => {
            const id = crypto.randomUUID();
            await api.recoveryCreate(id);
            await api.recoveryPersist(id, "doc", new Uint8Array([41]));
            await changeSessionManifest(id);
            api.recoveryQueue(id, "doc", new Uint8Array([42]));
            const failed = await eventually(() => {
                const state = JSON.parse(api.recoveryStatus(id));
                return state.error && state;
            }, "invalid metadata failure");
            assert(failed.error.includes("metadata"), failed.error);
            assert(new Uint8Array((await readRecord(id, "doc")).bytes)[0] === 41,
                "Invalid metadata allowed the previous snapshot to be replaced");
            assert((await readSessionRecord(id)).manifest === null, "Invalid metadata was silently replaced");
            api.recoveryRelease(id);
        });
        await test("shutdown waits for accepted writes and preserves committed records", async () => {
            const id = crypto.randomUUID();
            await api.recoveryCreate(id);
            const pending = api.recoveryPersist(id, "doc", new Uint8Array([6]));
            assert(api.recoveryFinish(id) === "pending", "Shutdown cancelled an accepted write");
            assert(api.recoveryHeld(id), "A pending write lost its owner");
            await pending;
            const transaction = IDBDatabase.prototype.transaction;
            IDBDatabase.prototype.transaction = function (stores, mode, options) {
                if (mode === "readwrite") throw new DOMException("test failure", "QuotaExceededError");
                return transaction.call(this, stores, mode, options);
            };
            try {
                assert(api.recoveryFinish(id) === "finished", "Shutdown did not release its owner");
            } finally {
                IDBDatabase.prototype.transaction = transaction;
            }
            const frame = await child();
            try {
                const other = frame.contentWindow.recoveryTestApi;
                await other.recoveryInitialize();
                const found = await eventually(async () =>
                    (await other.recoveryDiscover()).sessions.find(item => item.id === id),
                "discovering retained shutdown snapshot");
                assert(found.docs[0]?.bytes[0] === 6, "Shutdown removed the committed snapshot");
                other.recoveryRelease(id);
            } finally {
                frame.remove();
            }
        });
        await test("dropping a writer aborts a pending replacement before releasing its lease", async () => {
            const id = crypto.randomUUID();
            await api.recoveryCreate(id);
            await api.recoveryPersist(id, "doc", new Uint8Array([51]));
            const pending = api.recoveryPersist(id, "doc", new Uint8Array([52]));
            api.recoveryRelease(id);
            let failed = false;
            try { await pending; } catch (_) { failed = true; }
            assert(failed, "Released pending operation was reported as committed");
            await sleep(50);
            assert(new Uint8Array((await readRecord(id, "doc")).bytes)[0] === 51,
                "A released writer replaced the previously committed snapshot");
            const frame = await child();
            try {
                const other = frame.contentWindow.recoveryTestApi;
                await other.recoveryInitialize();
                const found = (await other.recoveryDiscover()).sessions.find(item => item.id === id);
                assert(found?.docs.find(doc => doc.key === "doc")?.bytes[0] === 51,
                    "The prior snapshot was not recoverable by the next owner");
                other.recoveryRelease(id);
            } finally {
                frame.remove();
            }
        });
        await test("missing committed records are reported without deleting survivors", async () => {
            const damaged = crypto.randomUUID();
            await api.recoveryCreate(damaged);
            await api.recoveryPersist(damaged, "missing", new Uint8Array([1]));
            await api.recoveryPersist(damaged, "survivor", new Uint8Array([2]));
            await removeDocumentRecord(damaged, "missing");
            assert(api.recoverySuspend(damaged) === "suspended", "Could not release test session");
            const frame = await child();
            const other = frame.contentWindow.recoveryTestApi;
            await other.recoveryInitialize();
            const found = await other.recoveryDiscover();
            assert(found.warning.includes("missing") || found.warning.includes("evicted"), found.warning);
            const session = found.sessions.find(item => item.id === damaged);
            assert(session?.docs.some(doc => doc.key === "survivor" && doc.bytes[0] === 2),
                "Surviving recovery data was deleted");
            other.recoveryRelease(damaged);
            frame.remove();
        });
        await test("a malformed empty manifest is reported and retained", async () => {
            const id = crypto.randomUUID();
            await api.recoveryCreate(id);
            assert(api.recoveryFinish(id) === "finished", "Could not release malformed test session");
            await changeSessionManifest(id);
            const frame = await child();
            try {
                const other = frame.contentWindow.recoveryTestApi;
                await other.recoveryInitialize();
                const found = await other.recoveryDiscover();
                assert(found.warning.includes("missing") || found.warning.includes("evicted"), found.warning);
                assert(await readSessionRecord(id), "Malformed session metadata was deleted");
            } finally {
                frame.remove();
                await removeSessionRecord(id);
            }
        });
        await test("a missing claimed snapshot cannot be silently discarded", async () => {
            const id = crypto.randomUUID();
            await api.recoveryCreate(id);
            await api.recoveryPersist(id, "doc", new Uint8Array([3]));
            assert(api.recoverySuspend(id) === "suspended", "Could not release test session");
            api.recoveryRelease(id);
            const frame = await child();
            try {
                const other = frame.contentWindow.recoveryTestApi;
                await other.recoveryInitialize();
                const found = await eventually(async () =>
                    (await other.recoveryDiscover()).sessions.find(item => item.id === id),
                "claiming snapshot before simulated eviction");
                await removeDocumentRecord(id, "doc");
                let failed = false;
                try { await other.recoveryDiscard(id, "doc", found.docs[0].revision, undefined, found.docs[0].bytes); }
                catch (error) { failed = String(error).includes("missing") || String(error).includes("evicted"); }
                assert(failed, "Missing snapshot was reported as a successful discard");
                assert(other.recoveryHeld(id), "Failed discard released the orphan claim");
                other.recoveryRelease(id);
            } finally {
                frame.remove();
            }
        });
        await test("keeping a pending or already-completed discard preserves the cached original", async () => {
            const id = `keep-${crypto.randomUUID()}`;
            const original = new Uint8Array([0, 5, 255, 7]);
            await seedSession(id, [{
                id: `${id}/doc`, session: id, key: "doc", revision: 1,
                deleted: false, bytes: original.buffer,
            }], [{ key: "doc", revision: 1 }]);
            const frame = await child();
            let other = frame.contentWindow.recoveryTestApi;
            try {
                await other.recoveryInitialize();
                let session = (await discover(other)).sessions.find(item => item.id === id);
                let doc = session?.docs.find(item => item.key === "doc");
                assert(doc, "Could not claim the original");
                await other.recoveryDiscard(id, "doc", doc.revision, session.lease, doc.bytes);
                assert(await other.recoveryInspect(id, "doc", doc.revision, session.lease, doc.bytes) === "pending",
                    "The discard request did not remain pending");
                const pending = await readRecord(id, "doc");
                assert(!pending.deleted && new Uint8Array(pending.bytes).join() === original.join()
                    && (await readSessionRecord(id)).manifest[0]?.key === "doc",
                "The pending request changed the original or its manifest");
                const rescue = await other.recoveryRescueBrowserRecord("documents", `${id}/doc`);
                assert(rescue.raw && new Uint8Array(await rescue.raw.arrayBuffer()).join() === original.join(),
                    "The pending source could not be rescued byte-for-byte");
                assert(await other.recoveryCancelDiscard(id, "doc", doc.revision, session.lease, doc.bytes) === "present",
                    "Keeping the pending original did not succeed");
                assert(!Object.hasOwn(await readRecord(id, "doc"), "discardIntent"),
                    "Keeping the original left an unresolved intent");

                await other.recoveryDiscard(id, "doc", doc.revision, session.lease, doc.bytes);
                assert(await other.recoveryCompleteDiscard(id, "doc", doc.revision, session.lease, doc.bytes) === "absent",
                    "Could not complete the second request");
                // This models the user choosing Keep while completion was
                // committed but its acknowledgement was not delivered.
                assert(await other.recoveryCancelDiscard(id, "doc", doc.revision, session.lease, doc.bytes) === "present",
                    "Keep could not restore the content-bound cached copy");
                const restored = await readRecord(id, "doc");
                assert(!restored.deleted && new Uint8Array(restored.bytes).join() === original.join()
                    && !Object.hasOwn(restored, "discardReceipt")
                    && (await readSessionRecord(id)).manifest[0]?.revision === 1,
                "Keeping after completion failed to restore the original bytes and manifest");
                other.recoveryRelease(id, session.lease);

                const reloaded = new Promise(resolve => frame.addEventListener("load", resolve, { once: true }));
                frame.contentWindow.location.reload();
                await reloaded;
                other = await eventually(() => frame.contentWindow.recoveryTestApi, "reloaded keep module");
                await other.recoveryInitialize();
                session = await eventually(async () =>
                    (await discover(other)).sessions.find(item => item.id === id),
                "kept original after reload");
                doc = session.docs.find(item => item.key === "doc");
                assert(doc?.pendingDiscard === false && doc.bytes.join() === original.join(),
                    "Reload did not preserve the kept source");
                await other.recoveryDiscard(id, "doc", doc.revision, session.lease, doc.bytes);
                assert(await other.recoveryCompleteDiscard(id, "doc", doc.revision, session.lease, doc.bytes) === "absent",
                    "A kept source could not be explicitly discarded on retry");
                other.recoveryRelease(id, session.lease);
            } finally {
                other.recoveryRelease(id);
                frame.remove();
                await removeFixture(id, ["doc"]);
            }
        });
        await test("missing records and ordinary tombstones cannot confirm a discard", async () => {
            const missing = `missing-${crypto.randomUUID()}`;
            const tomb = `ordinary-${crypto.randomUUID()}`;
            const record = (id, value) => ({
                id: `${id}/doc`, session: id, key: "doc", revision: 1,
                deleted: false, bytes: new Uint8Array([value]).buffer,
            });
            await seedSession(missing, [record(missing, 71)], [{ key: "doc", revision: 1 }]);
            await seedSession(tomb, [record(tomb, 72)], [{ key: "doc", revision: 1 }]);
            const frame = await child();
            const other = frame.contentWindow.recoveryTestApi;
            try {
                await other.recoveryInitialize();
                const found = await discover(other);
                const missingClaim = found.sessions.find(item => item.id === missing);
                const tombClaim = found.sessions.find(item => item.id === tomb);
                assert(missingClaim && tombClaim, "Could not claim both synthetic originals");
                const missingDoc = missingClaim.docs[0];
                const tombDoc = tombClaim.docs[0];
                await other.recoveryDiscard(missing, "doc", 1, missingClaim.lease, missingDoc.bytes);
                await other.recoveryDiscard(tomb, "doc", 1, tombClaim.lease, tombDoc.bytes);
                await removeDocumentRecord(missing, "doc");
                const connection = await db();
                try {
                    await new Promise((resolve, reject) => {
                        const tx = fixtureWrite(connection, ["sessions", "documents"]);
                        tx.objectStore("documents").put({
                            id: `${tomb}/doc`, session: tomb, key: "doc", revision: 1,
                            deleted: true, bytes: null,
                        });
                        const sessions = tx.objectStore("sessions");
                        const request = sessions.get(tomb);
                        request.onsuccess = () => {
                            const row = request.result;
                            row.manifest = [];
                            sessions.put(row);
                        };
                        tx.oncomplete = resolve;
                        tx.onabort = () => reject(tx.error);
                    });
                } finally {
                    connection.close();
                }
                for (const [id, claim, doc] of [[missing, missingClaim, missingDoc], [tomb, tombClaim, tombDoc]]) {
                    for (const operation of [
                        other.recoveryInspect, other.recoveryCompleteDiscard, other.recoveryCancelDiscard,
                    ]) {
                        let unknown = false;
                        try { await operation(id, "doc", 1, claim.lease, doc.bytes); }
                        catch (error) { unknown = error.name === "RecoveryOutcomeUnknown"; }
                        assert(unknown, `${id}: missing or unrelated tombstone falsely confirmed discard`);
                    }
                }
                assert(!(await readRecord(missing, "doc"))
                    && (await readRecord(tomb, "doc")).deleted === true
                    && missingDoc.bytes[0] === 71 && tombDoc.bytes[0] === 72,
                "Verification changed the simulated records or cached originals");
                for (const session of found.sessions) other.recoveryRelease(session.id, session.lease);
            } finally {
                frame.remove();
                await removeFixture(missing, ["doc"]);
                await removeFixture(tomb, ["doc"]);
            }
        });
        await test("aborted single-source discard preserves every original after restart", async () => {
            const id = crypto.randomUUID();
            await api.recoveryCreate(id);
            await api.recoveryPersist(id, "first", new Uint8Array([11]));
            await api.recoveryPersist(id, "second", new Uint8Array([12]));
            assert(api.recoveryFinish(id) === "finished", "Could not release source writer");
            let frame = await child();
            try {
                const other = frame.contentWindow.recoveryTestApi;
                await other.recoveryInitialize();
                const session = (await other.recoveryDiscover()).sessions.find(item => item.id === id);
                assert(session, "Could not claim the source");
                const doc = session.docs.find(item => item.key === "first");
                const originalTimer = frame.contentWindow.setTimeout;
                let timeoutInjected = false;
                frame.contentWindow.setTimeout = function (callback, delay, ...args) {
                    if (delay === 15_000) {
                        timeoutInjected = true;
                        const timer = originalTimer(() => {}, delay);
                        queueMicrotask(callback);
                        return timer;
                    }
                    return originalTimer(callback, delay, ...args);
                };
                try {
                    let aborted = false;
                    try { await other.recoveryDiscard(id, doc.key, doc.revision, undefined, doc.bytes); }
                    catch (error) { aborted = String(error).includes("aborted"); }
                    assert(timeoutInjected, "Could not inject the transaction timeout");
                    assert(aborted, "An aborted discard did not report failure");
                } finally {
                    frame.contentWindow.setTimeout = originalTimer;
                }
                assert(await other.recoveryInspect(id, doc.key, doc.revision, undefined, doc.bytes) === "present",
                    "Aborted discard did not retain the selected original");
                other.recoveryRelease(id);
            } finally {
                frame.remove();
            }
            frame = await child();
            try {
                const restarted = frame.contentWindow.recoveryTestApi;
                await restarted.recoveryInitialize();
                const session = (await restarted.recoveryDiscover()).sessions.find(item => item.id === id);
                assert(session?.docs.some(doc => doc.key === "first" && doc.bytes[0] === 11),
                    "Aborted original was not recoverable after restart");
                assert(session?.docs.some(doc => doc.key === "second" && doc.bytes[0] === 12),
                    "A different original was affected by the discard");
                restarted.recoveryRelease(id);
            } finally {
                frame.remove();
            }
        });
        await test("unacknowledged discard survives reload, verification, and completion retry", async () => {
            const id = crypto.randomUUID();
            await api.recoveryCreate(id);
            await api.recoveryPersist(id, "doc", new Uint8Array([13]));
            assert(api.recoveryFinish(id) === "finished", "Could not release source writer");
            const frame = await child();
            try {
                let other = frame.contentWindow.recoveryTestApi;
                await other.recoveryInitialize();
                const session = (await other.recoveryDiscover()).sessions.find(item => item.id === id);
                const doc = session?.docs.find(item => item.key === "doc");
                assert(doc, "Could not claim the source");
                const loseCompletion = async operation => {
                    const originalTimer = frame.contentWindow.setTimeout;
                    const prototype = frame.contentWindow.IDBDatabase.prototype;
                    const originalTransaction = prototype.transaction;
                    let fireTimeout;
                    frame.contentWindow.setTimeout = function (callback, delay, ...args) {
                        if (delay === 15_000) {
                            fireTimeout = callback;
                            return originalTimer(() => {}, delay);
                        }
                        return originalTimer(callback, delay, ...args);
                    };
                    prototype.transaction = function (stores, mode, options) {
                        const transaction = originalTransaction.call(this, stores, mode, options);
                        if (mode !== "readwrite") return transaction;
                        return new Proxy(transaction, {
                            get(target, property) {
                                const value = Reflect.get(target, property, target);
                                return typeof value === "function" ? value.bind(target) : value;
                            },
                            set(target, property, value) {
                                if (property === "oncomplete") {
                                    target.oncomplete = event => {
                                        fireTimeout();
                                        value.call(target, event);
                                    };
                                    return true;
                                }
                                return Reflect.set(target, property, value, target);
                            },
                        });
                    };
                    try {
                        let unknown = false;
                        try { await operation(); } catch (error) { unknown = error.name === "RecoveryOutcomeUnknown"; }
                        assert(unknown, "A committed transaction with a lost acknowledgement looked definite");
                    } finally {
                        prototype.transaction = originalTransaction;
                        frame.contentWindow.setTimeout = originalTimer;
                    }
                };
                await loseCompletion(() => other.recoveryDiscard(id, doc.key, doc.revision, undefined, doc.bytes));
                assert(await other.recoveryInspect(id, doc.key, doc.revision, undefined, doc.bytes) === "pending",
                    "The unacknowledged request did not remain pending");
                const retained = await readRecord(id, "doc");
                assert(!retained.deleted && new Uint8Array(retained.bytes).join() === "13"
                    && retained.discardIntent?.version === 1,
                "The pending request changed or removed original bytes");

                const reloaded = new Promise(resolve => frame.addEventListener("load", resolve, { once: true }));
                frame.contentWindow.location.reload();
                await reloaded;
                other = await eventually(() => frame.contentWindow.recoveryTestApi, "reloaded recovery module");
                await other.recoveryInitialize();
                const restored = await eventually(async () =>
                    (await other.recoveryDiscover()).sessions.find(item => item.id === id),
                "pending discard after reload");
                const afterReload = restored.docs.find(item => item.key === "doc");
                assert(afterReload?.pendingDiscard === true && afterReload.bytes.join() === "13",
                    "Reload hid the unresolved discard or lost its bytes");
                let unknown = false;
                try { await other.recoveryInspect(id, "doc", afterReload.revision, restored.lease, new Uint8Array([14])); }
                catch (error) { unknown = error.name === "RecoveryOutcomeUnknown"; }
                assert(unknown && new Uint8Array((await readRecord(id, "doc")).bytes)[0] === 13,
                    "A different cached copy verified the pending discard");

                await loseCompletion(() => other.recoveryCompleteDiscard(
                    id, "doc", afterReload.revision, restored.lease, afterReload.bytes));
                assert(await other.recoveryInspect(id, "doc", afterReload.revision, restored.lease, afterReload.bytes) === "absent",
                    "The content-bound receipt did not verify the completed discard");
                unknown = false;
                try { await other.recoveryInspect(id, "doc", afterReload.revision, restored.lease, new Uint8Array([14])); }
                catch (error) { unknown = error.name === "RecoveryOutcomeUnknown"; }
                assert(unknown, "A receipt for different bytes falsely confirmed discard");
                assert(await other.recoveryCompleteDiscard(id, "doc", afterReload.revision, restored.lease, afterReload.bytes) === "absent",
                    "Retry could not verify the already completed discard");
                other.recoveryRelease(id, restored.lease);
            } finally {
                frame.remove();
            }
            const receipt = await readRecord(id, "doc");
            assert(receipt?.deleted === true && receipt.bytes === null
                && receipt.discardReceipt?.version === 1 && receipt.discardReceipt.digest?.length === 64,
                "The verified deletion receipt did not survive release");
            await removeFixture(id, ["doc"]);
        });
        if (params.get("filter") && testsRun === 0) throw new Error("No browser recovery checks matched the filter.");
        document.body.dataset.result = "pass";
        write(params.get("filter")
            ? `Passed ${testsRun} filtered browser recovery storage checks.`
            : `All ${testsRun} browser recovery storage checks passed.`);
    } catch (error) {
        document.body.dataset.result = "fail";
        write(`FAIL ${error.stack || error}`);
        console.error(error);
    }
}
