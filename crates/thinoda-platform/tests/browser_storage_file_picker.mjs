import assert from "node:assert/strict";
import { createHash, webcrypto } from "node:crypto";
import { readFile } from "node:fs/promises";
import test from "node:test";

const source = await readFile(new URL("../src/browser_storage.js", import.meta.url), "utf8");
let moduleNumber = 0;

async function freshModule() {
    const encoded = Buffer.from(source).toString("base64");
    return import(`data:text/javascript;base64,${encoded}#test-${moduleNumber++}`);
}

function installGlobals(storage = {}) {
    Object.defineProperty(globalThis, "crypto", { value: webcrypto, configurable: true });
    Object.defineProperty(globalThis, "BroadcastChannel", { value: undefined, configurable: true });
    Object.defineProperty(globalThis, "isSecureContext", { value: true, configurable: true });
    Object.defineProperty(globalThis, "navigator", {
        value: {
            storage,
            locks: {
                async request(_name, options, callback) {
                    return typeof options === "function" ? options({}) : callback({});
                }
            }
        },
        configurable: true
    });
}

function digest(bytes) {
    return `sha256:${createHash("sha256").update(bytes).digest("hex")}`;
}

function handle(name, initial, options = {}) {
    let bytes = Uint8Array.from(initial);
    let bufferReads = 0;
    let fileReads = 0;
    const result = {
        name,
        async getFile() {
            fileReads++;
            if (options.getFileError) throw options.getFileError;
            const snapshot = bytes.slice();
            return {
                name: options.fileName ?? name,
                size: options.reportedSize ?? snapshot.length,
                async arrayBuffer() {
                    bufferReads++;
                    options.onBuffer?.();
                    return snapshot.buffer;
                }
            };
        },
        async queryPermission() { return options.permission ?? "granted"; },
        async requestPermission() { return options.permission ?? "granted"; },
        async createWritable() {
            let pending;
            return {
                async write(input) { pending = Uint8Array.from(input); },
                async close() { bytes = pending; },
                async abort() {}
            };
        },
        setBytes(input) { bytes = Uint8Array.from(input); },
        bytes() { return [...bytes]; },
        get bufferReads() { return bufferReads; },
        get fileReads() { return fileReads; }
    };
    return result;
}

function fakeDatabase(records, contents, counters) {
    return {
        transaction() {
            let pending = 0;
            let completed = false;
            const tx = {
                objectStore(store) {
                    return {
                        get(key) {
                            const request = {};
                            pending++;
                            queueMicrotask(() => {
                                if (store === "contents") counters.contentReads++;
                                request.result = (store === "documents" ? records : contents).get(key);
                                request.onsuccess?.();
                                pending--;
                                complete();
                            });
                            return request;
                        }
                    };
                }
            };
            function complete() {
                if (!completed && pending === 0) {
                    completed = true;
                    tx.oncomplete?.();
                }
            }
            queueMicrotask(complete);
            return tx;
        },
        close() {}
    };
}

function installDatabase(db) {
    Object.defineProperty(globalThis, "indexedDB", {
        configurable: true,
        value: {
            open() {
                const request = {};
                queueMicrotask(() => {
                    request.result = db;
                    request.onsuccess?.();
                });
                return request;
            }
        }
    });
}

test("multiple handles are read separately and save back independently", async () => {
    installGlobals();
    const storage = await freshModule();
    const first = handle("same.mmap", [1, 2, 3, 4]);
    const unreadable = handle("unreadable.mmap", [], { getFileError: new Error("read failed") });
    const huge = handle("huge.mmap", [], { reportedSize: 64 * 1024 * 1024 + 1 });
    const second = handle("same.mmap", [1, 2, 3, 4]);
    let pickerOptions;
    globalThis.showOpenFilePicker = async options => {
        pickerOptions = options;
        return [first, unreadable, huge, second];
    };
    const picked = await storage.pickFileHandles(true);
    assert.equal(pickerOptions.multiple, true);
    assert.equal(first.fileReads, 0, "picker must not read before per-file staging");
    assert.equal(picked.length, 4);
    assert.notEqual(picked[0].session, picked[3].session);

    const firstRead = await storage.readPickedFile(picked[0].session, () => false, 4);
    await assert.rejects(storage.readPickedFile(picked[1].session, () => false, 4));
    await assert.rejects(
        storage.readPickedFile(picked[2].session, () => false, 64 * 1024 * 1024),
        error => error.code === "too_large"
    );
    assert.equal(huge.bufferReads, 0, "oversized File must not call arrayBuffer");
    const secondRead = await storage.readPickedFile(picked[3].session, () => false, 4);
    assert.deepEqual([...firstRead.bytes], [1, 2, 3, 4]);
    assert.deepEqual([...secondRead.bytes], [1, 2, 3, 4]);

    const firstRevision = await storage.writePickedFile(
        picked[0].session, [4, 3, 2, 1], `match:${firstRead.revision}`, () => false
    );
    assert.deepEqual(first.bytes(), [4, 3, 2, 1]);
    assert.deepEqual(second.bytes(), [1, 2, 3, 4]);
    await storage.writePickedFile(
        picked[3].session, [5, 6, 7, 8], `match:${secondRead.revision}`, () => false
    );
    assert.deepEqual(first.bytes(), [4, 3, 2, 1]);
    assert.deepEqual(second.bytes(), [5, 6, 7, 8]);

    first.setBytes([9, 9, 9, 9]);
    await assert.rejects(
        storage.writePickedFile(picked[0].session, [0], `match:${firstRevision}`, () => false),
        error => error.code === "conflict" && typeof error.actual === "string"
    );
    assert.deepEqual(first.bytes(), [9, 9, 9, 9]);
    first.setBytes([9, 9, 9, 9, 9]);
    const before = first.bufferReads;
    await assert.rejects(
        storage.writePickedFile(picked[0].session, [0], `match:${firstRevision}`, () => false),
        error => error.code === "too_large"
    );
    assert.equal(first.bufferReads, before, "oversized external change must not be read");
    assert.deepEqual(first.bytes(), [9, 9, 9, 9, 9]);
    for (const entry of picked) storage.forgetPickedFile(entry.session);
    await assert.rejects(storage.readPickedFile(picked[0].session, () => false, 4));
});

test("single-file mode, picker cancellation, growth, and read cancellation", async () => {
    installGlobals();
    const storage = await freshModule();
    const single = handle("one.xdf3", [1, 2, 3, 4]);
    let options;
    globalThis.showOpenFilePicker = async value => {
        options = value;
        return [single];
    };
    const [picked] = await storage.pickFileHandles(false);
    assert.equal(options.multiple, false);
    const singleRead = await storage.readPickedFile(picked.session, () => false, 4);
    assert.deepEqual([...singleRead.bytes], [1, 2, 3, 4]);
    const largeRevision = await storage.writePickedFile(
        picked.session, [1, 2, 3, 4, 5], `match:${singleRead.revision}`, () => false
    );
    assert.deepEqual(single.bytes(), [1, 2, 3, 4, 5], "output may exceed the import limit");
    single.setBytes([9, 9, 9, 9, 9, 9]);
    const beforeUnexpectedGrowth = single.bufferReads;
    await assert.rejects(
        storage.writePickedFile(picked.session, [7, 8], `match:${largeRevision}`, () => false),
        error => error.code === "too_large"
    );
    assert.equal(single.bufferReads, beforeUnexpectedGrowth);
    assert.deepEqual(single.bytes(), [9, 9, 9, 9, 9, 9]);
    single.setBytes([1, 2, 3, 4, 5]);
    const smallRevision = await storage.writePickedFile(
        picked.session, [7, 8], `match:${largeRevision}`, () => false
    );
    assert.deepEqual(single.bytes(), [7, 8], "a smaller subsequent save must succeed");
    single.setBytes([1, 2, 3, 4, 5]);
    const beforeStaleGrowth = single.bufferReads;
    await assert.rejects(
        storage.writePickedFile(picked.session, [7], `match:${smallRevision}`, () => false),
        error => error.code === "too_large"
    );
    assert.equal(single.bufferReads, beforeStaleGrowth, "the bound follows the latest save");

    let cancelled = false;
    const growing = handle("growing.xdf3", [1, 2, 3, 4, 5], { reportedSize: 4 });
    const cancelDuringRead = handle("cancel.xdf3", [1], { onBuffer: () => { cancelled = true; } });
    globalThis.showOpenFilePicker = async () => [growing, cancelDuringRead];
    const [growth, cancelledFile] = await storage.pickFileHandles(true);
    await assert.rejects(
        storage.readPickedFile(growth.session, () => false, 4),
        error => error.code === "too_large"
    );
    await assert.rejects(
        storage.readPickedFile(cancelledFile.session, () => cancelled, 4),
        error => error.code === "cancelled"
    );
    const valid = handle("valid.xdf3", [1]);
    globalThis.showOpenFilePicker = async () => [null, valid];
    const [unsupported, supported] = await storage.pickFileHandles(true);
    assert.equal(unsupported.error.code, "unsupported");
    assert.deepEqual(
        [...(await storage.readPickedFile(supported.session, () => false, 4)).bytes],
        [1]
    );
    globalThis.showOpenFilePicker = async () => {
        const error = new Error("cancelled");
        error.name = "AbortError";
        throw error;
    };
    await assert.rejects(storage.pickFileHandles(true), error => error.code === "cancelled");
    for (const entry of [picked, growth, cancelledFile, supported]) {
        storage.forgetPickedFile(entry.session);
    }
});

test("IndexedDB metadata limits precede content fetch and bytes remain verified", async () => {
    installGlobals();
    const counters = { contentReads: 0 };
    const records = new Map();
    const contents = new Map();
    installDatabase(fakeDatabase(records, contents, counters));
    const storage = await freshModule();
    const id = "/Documents/map.mmap";
    const blob = "blob";
    const bytes = Uint8Array.from([1, 2, 3, 4]);
    records.set(id, { id, size: 5, revision: digest(bytes), backend: "indexeddb", blob });
    contents.set(blob, { bytes });
    await assert.rejects(
        storage.readDocumentBounded(id, () => false, 4),
        error => error.code === "too_large"
    );
    assert.equal(counters.contentReads, 0);

    records.get(id).size = -1;
    await assert.rejects(
        storage.readDocumentBounded(id, () => false, 4),
        error => error.code === "io"
    );
    assert.equal(counters.contentReads, 0);

    records.get(id).size = 4;
    const read = await storage.readDocumentBounded(id, () => false, 4);
    assert.deepEqual([...read.bytes], [...bytes]);
    assert.equal(read.revision, digest(bytes));
    assert.equal(counters.contentReads, 1);

    contents.set(blob, { bytes: Uint8Array.from([1, 2, 3, 4, 5]) });
    await assert.rejects(
        storage.readDocumentBounded(id, () => false, 4),
        error => error.code === "too_large"
    );
    // IndexedDB structured-clones the value before its actual length is visible.
    assert.equal(counters.contentReads, 2);
    contents.set(blob, { bytes });
    records.get(id).revision = "sha256:incorrect";
    await assert.rejects(
        storage.readDocumentBounded(id, () => false, 4),
        error => error.code === "io"
    );
    await assert.rejects(
        storage.readDocumentBounded(id, () => true, 4),
        error => error.code === "cancelled"
    );
});

test("OPFS File.size is checked before arrayBuffer and exact bytes are verified", async () => {
    const files = new Map();
    const directory = {
        async getFileHandle(name, options) {
            if (name.startsWith("probe-") && options?.create) {
                return { async createWritable() { return { async close() {} }; } };
            }
            return files.get(name);
        },
        async removeEntry() {}
    };
    installGlobals({ async getDirectory() {
        return { async getDirectoryHandle() { return directory; } };
    } });
    const counters = { contentReads: 0 };
    const records = new Map();
    installDatabase(fakeDatabase(records, new Map(), counters));
    const storage = await freshModule();
    const id = "/Documents/map.xdf2";
    const bytes = Uint8Array.from([1, 2, 3, 4]);
    const file = handle("blob", [1, 2, 3, 4, 5]);
    files.set("blob", file);
    records.set(id, { id, size: 4, revision: digest(bytes), backend: "opfs", blob: "blob" });
    await assert.rejects(
        storage.readDocumentBounded(id, () => false, 4),
        error => error.code === "too_large"
    );
    assert.equal(file.bufferReads, 0);
    file.setBytes(bytes);
    const read = await storage.readDocumentBounded(id, () => false, 4);
    assert.deepEqual([...read.bytes], [...bytes]);
    assert.equal(read.revision, digest(bytes));
    assert.equal(counters.contentReads, 0);
});
