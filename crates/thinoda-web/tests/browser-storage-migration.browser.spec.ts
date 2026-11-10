import { expect, test, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { colorBounds, frames, zipEntry } from "./board-browser-helpers";

test.use({
    viewport: { width: 1024, height: 768 },
    deviceScaleFactor: 1,
    locale: "en-US",
    serviceWorkers: "block",
});
test.setTimeout(180_000);

const url = process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/";
const parentId = "/Documents/legacy-folder";
const childId = `${parentId}/child.thinoda`;
const parentKey = `thinoda.vfs:${parentId}`;
const childKey = `thinoda.vfs:${childId}`;
const envelope = (contents: string) => JSON.stringify({
    m: 123, l: Buffer.byteLength(contents), d: Buffer.from(contents).toString("base64"),
});
const parentValue = envelope("legacy parent");
const childValue = envelope("legacy child");

async function started(page: Page) {
    await page.waitForFunction(() => {
        const loading = document.querySelector("#loading");
        return !loading || loading.getAttribute("role") === "alert"
            || getComputedStyle(loading).display === "none";
    }, null, { timeout: 120_000 });
    const error = await page.evaluate(
        () => document.querySelector("#loading[role='alert']")?.textContent ?? null,
    );
    expect(error, "browser document initialization should succeed").toBeNull();
    await expect(page.locator("#loading")).toBeHidden();
}

async function storageModule(page: Page) {
    return page.evaluate(async () => {
        const app = document.querySelector<HTMLScriptElement>("script[type='module'][src]");
        if (!app) throw new Error("Missing app module");
        const response = await fetch(app.src);
        if (!response.ok) throw new Error("Cannot inspect the active app module");
        const source = await response.text();
        const importPath = source.match(/from ['"](\.\/snippets\/[^'"]+\/src\/browser_storage\.js)['"]/)?.[1];
        if (!importPath) throw new Error("Missing active browser storage module");
        return new URL(importPath, app.src).href;
    });
}

async function inspect(page: Page, retainedId: string, moduleUrl: string, original = childId) {
    return page.evaluate(async ({ retainedId, original, moduleUrl }) => {
        const storage = await import(moduleUrl);
        const report = JSON.parse(await storage.initializeDocuments());
        const retainedBytes = new TextDecoder().decode((await storage.readDocument(retainedId)).bytes);
        const conflicts: { original: string; preserved: string }[] = report.conflicts;
        const preserved = conflicts.find(conflict => conflict.original === original);
        if (!preserved) throw new Error("Missing preserved legacy document");
        const preservedBytes = new TextDecoder().decode(
            (await storage.readDocument(preserved.preserved)).bytes,
        );
        return { report, retainedBytes, preservedBytes, preserved: preserved.preserved };
    }, { retainedId, original, moduleUrl });
}

test("legacy file and directory collisions retain both documents across reloads", async ({ page }) => {
    await page.addInitScript(({ parentKey, childKey, parentValue, childValue }) => {
        if (sessionStorage.getItem("legacy-collision-seeded") !== "yes") {
            localStorage.setItem(parentKey, parentValue);
            localStorage.setItem(childKey, childValue);
            sessionStorage.setItem("legacy-collision-seeded", "yes");
        }
    }, { parentKey, childKey, parentValue, childValue });
    await page.goto(url, { waitUntil: "commit", timeout: 120_000 });
    await started(page);
    await expect(page.locator("#thinoda-browser-migration")).toHaveText(
        "Preserved browser documents are available in File > Recovered browser documents.",
        { timeout: 30_000 },
    );

    const moduleUrl = await storageModule(page);
    const initial = await inspect(page, parentId, moduleUrl);
    expect(initial.report.verified).toBe(2);
    expect(initial.report.conflicts).toHaveLength(1);
    expect(initial.retainedBytes).toBe("legacy parent");
    expect(initial.preservedBytes).toBe("legacy child");
    expect(initial.preserved).toMatch(/^\/Documents\/\.thinoda-legacy\//);
    expect(await page.evaluate(([parentKey, childKey]) => [
        localStorage.getItem(parentKey), localStorage.getItem(childKey),
    ], [parentKey, childKey])).toEqual([parentValue, childValue]);

    await page.reload({ waitUntil: "commit", timeout: 120_000 });
    await started(page);
    const restored = await inspect(page, parentId, await storageModule(page));
    expect(restored).toEqual(initial);
    expect(await page.evaluate(([parentKey, childKey]) => [
        localStorage.getItem(parentKey), localStorage.getItem(childKey),
    ], [parentKey, childKey])).toEqual([parentValue, childValue]);
});

test("user can open and save a recovered browser document without replacing its source", async ({ page }) => {
    const original = await readFile(join(__dirname, "fixtures", "affinity-diagram.thinoda"));
    const documentValue = JSON.stringify({ m: 123, l: original.length, d: original.toString("base64") });
    await page.addInitScript(({ parentKey, childKey, parentValue, documentValue }) => {
        localStorage.setItem(parentKey, parentValue);
        localStorage.setItem(childKey, documentValue);
    }, { parentKey, childKey, parentValue, documentValue });
    await page.goto(url, { waitUntil: "commit", timeout: 120_000 });
    await started(page);
    await expect(page.locator("#thinoda-browser-migration")).toContainText(
        "Recovered browser documents", { timeout: 30_000 },
    );
    const moduleUrl = await storageModule(page);
    const copy = await page.evaluate(async ({ moduleUrl, childId }) => {
        const storage = await import(moduleUrl);
        const report = JSON.parse(await storage.initializeDocuments());
        const conflict = report.conflicts.find((entry: { original: string }) => entry.original === childId);
        if (!conflict) throw new Error("Missing preserved copy");
        return conflict.preserved as string;
    }, { moduleUrl, childId });
    const canvas = page.locator("#thinoda_canvas");
    // The menu is rendered inside the canvas, without DOM buttons.
    for (const [x, y] of [[26, 22], [28, 57], [310, 185], [600, 185]]) {
        await canvas.click({ position: { x, y } });
        await frames(page);
    }
    await expect.poll(async () => (
        await colorBounds(page, await page.screenshot(), [[165, 216, 255]], 5)
    )[0].count, { timeout: 30_000 }).toBeGreaterThan(500);

    await page.evaluate(() => { window.prompt = () => "recovered-copy.thinoda"; });
    const pending = page.waitForEvent("download", { timeout: 30_000 });
    await canvas.focus();
    await page.keyboard.press("Control+s");
    const download = await pending;
    expect(download.suggestedFilename()).toBe("recovered-copy.thinoda");
    const saved = await readFile(await download.path());
    const savedContent = JSON.parse(zipEntry(saved, "content.json").toString("utf8"));
    const originalContent = JSON.parse(zipEntry(original, "content.json").toString("utf8"));
    expect(savedContent.sheets[0].title).toBe(originalContent.sheets[0].title);
    const retained = await page.evaluate(async ({ moduleUrl, copy, childKey }) => {
        const storage = await import(moduleUrl);
        return {
            original: Array.from((await storage.readDocument(copy)).bytes as Uint8Array),
            saved: Array.from((await storage.readDocument("/Documents/recovered-copy.thinoda")).bytes as Uint8Array),
            legacy: localStorage.getItem(childKey),
        };
    }, { moduleUrl, copy, childKey });
    expect(Buffer.from(retained.original)).toEqual(original);
    expect(Buffer.from(retained.saved)).toEqual(saved);
    expect(retained.legacy).toBe(documentValue);
});

test("legacy path collisions are preserved with the IndexedDB-only backend", async ({ page }) => {
    await page.addInitScript(({ parentKey, childKey, parentValue, childValue }) => {
        Object.defineProperty(navigator.storage, "getDirectory", {
            configurable: true, value: undefined,
        });
        localStorage.setItem(parentKey, parentValue);
        localStorage.setItem(childKey, childValue);
    }, { parentKey, childKey, parentValue, childValue });
    await page.goto(url, { waitUntil: "commit", timeout: 120_000 });
    await started(page);
    const initial = await inspect(page, parentId, await storageModule(page));
    expect(initial.report.backend).toBe("indexeddb");
    expect(initial.retainedBytes).toBe("legacy parent");
    expect(initial.preservedBytes).toBe("legacy child");
    await page.reload({ waitUntil: "commit", timeout: 120_000 });
    await started(page);
    expect(await inspect(page, parentId, await storageModule(page))).toEqual(initial);
    expect(await page.evaluate(([parentKey, childKey]) => [
        localStorage.getItem(parentKey), localStorage.getItem(childKey),
    ], [parentKey, childKey])).toEqual([parentValue, childValue]);
});

test("a durable parent file and legacy child are both preserved", async ({ page }) => {
    await page.goto(url, { waitUntil: "commit", timeout: 120_000 });
    await started(page);
    const moduleUrl = await storageModule(page);
    await page.evaluate(async ({ moduleUrl, parentId, childKey, childValue }) => {
        const storage = await import(moduleUrl);
        await storage.writeDocument(
            parentId, new TextEncoder().encode("durable parent"), "missing", () => false,
        );
        localStorage.setItem(childKey, childValue);
    }, { moduleUrl, parentId, childKey, childValue });
    await page.reload({ waitUntil: "commit", timeout: 120_000 });
    await started(page);

    const restored = await inspect(page, parentId, await storageModule(page));
    expect(restored.report.verified).toBe(1);
    expect(restored.report.conflicts).toHaveLength(1);
    expect(restored.retainedBytes).toBe("durable parent");
    expect(restored.preservedBytes).toBe("legacy child");
    expect(await page.evaluate(key => localStorage.getItem(key), childKey)).toBe(childValue);
});

test("a legacy parent does not overwrite a durable child", async ({ page }) => {
    await page.goto(url, { waitUntil: "commit", timeout: 120_000 });
    await started(page);
    await page.evaluate(async ({ moduleUrl, childId, parentKey, parentValue }) => {
        const storage = await import(moduleUrl);
        await storage.writeDocument(
            childId, new TextEncoder().encode("durable child"), "missing", () => false,
        );
        localStorage.setItem(parentKey, parentValue);
    }, { moduleUrl: await storageModule(page), childId, parentKey, parentValue });
    await page.reload({ waitUntil: "commit", timeout: 120_000 });
    await started(page);

    const restored = await inspect(page, childId, await storageModule(page), parentId);
    expect(restored.report.verified).toBe(1);
    expect(restored.report.conflicts).toHaveLength(1);
    expect(restored.retainedBytes).toBe("durable child");
    expect(restored.preservedBytes).toBe("legacy parent");
    expect(await page.evaluate(key => localStorage.getItem(key), parentKey)).toBe(parentValue);
});

test("concurrent tabs reuse the same verified legacy conflict copy", async ({ page, context }) => {
    await page.goto(url, { waitUntil: "commit", timeout: 120_000 });
    await started(page);
    const peer = await context.newPage();
    await peer.goto(url, { waitUntil: "commit", timeout: 120_000 });
    await started(peer);
    await page.evaluate(({ parentKey, childKey, parentValue, childValue }) => {
        localStorage.setItem(parentKey, parentValue);
        localStorage.setItem(childKey, childValue);
    }, { parentKey, childKey, parentValue, childValue });
    const run = async (tab: Page) => tab.evaluate(async moduleUrl => {
        const storage = await import(moduleUrl);
        return Promise.all(Array.from({ length: 2 }, async () =>
            JSON.parse(await storage.initializeDocuments())));
    }, await storageModule(tab));
    const reports = (await Promise.all([run(page), run(peer)])).flat();
    expect(reports.every(report => JSON.stringify(report) === JSON.stringify(reports[0]))).toBe(true);
    expect(reports[0].conflicts).toHaveLength(1);
    expect((await inspect(page, parentId, await storageModule(page))).preservedBytes).toBe("legacy child");
});

test("previously preserved versions remain discoverable when the legacy record changes", async ({ page }) => {
    await page.addInitScript(({ parentKey, childKey, parentValue, childValue }) => {
        if (sessionStorage.getItem("legacy-history-seeded") !== "yes") {
            localStorage.setItem(parentKey, parentValue);
            localStorage.setItem(childKey, childValue);
            sessionStorage.setItem("legacy-history-seeded", "yes");
        }
    }, { parentKey, childKey, parentValue, childValue });
    await page.goto(url, { waitUntil: "commit", timeout: 120_000 });
    await started(page);
    const moduleUrl = await storageModule(page);
    const initial = await inspect(page, parentId, moduleUrl);

    await page.evaluate(key => localStorage.removeItem(key), childKey);
    const withoutLegacy = await inspect(page, parentId, moduleUrl);
    expect(withoutLegacy.preserved).toBe(initial.preserved);
    expect(withoutLegacy.preservedBytes).toBe("legacy child");
    expect(withoutLegacy.report.verified).toBe(1);

    const changedValue = envelope("changed legacy child");
    await page.evaluate(([key, value]) => localStorage.setItem(key, value), [childKey, changedValue]);
    await page.reload({ waitUntil: "commit", timeout: 120_000 });
    await started(page);
    const recovered = await page.evaluate(async ({ moduleUrl, childId }) => {
        const storage = await import(moduleUrl);
        const report = JSON.parse(await storage.initializeDocuments());
        const copies = report.conflicts.filter((conflict: { original: string }) =>
            conflict.original === childId);
        return Promise.all(copies.map(async (copy: { preserved: string }) => ({
            path: copy.preserved,
            bytes: new TextDecoder().decode((await storage.readDocument(copy.preserved)).bytes),
        })));
    }, { moduleUrl: await storageModule(page), childId });
    expect(recovered).toHaveLength(2);
    expect(recovered.map(copy => copy.bytes).sort()).toEqual(["changed legacy child", "legacy child"]);
    expect(recovered.map(copy => copy.path)).toContain(initial.preserved);
    expect(recovered.map(copy => copy.path)).toEqual(recovered.map(copy => copy.path).sort());
    expect(await page.evaluate(key => localStorage.getItem(key), childKey)).toBe(changedValue);
});

test("unavailable hashing is not reported as corrupted recovery metadata", async ({ page }) => {
    await page.addInitScript(({ parentKey, childKey, parentValue, childValue }) => {
        localStorage.setItem(parentKey, parentValue);
        localStorage.setItem(childKey, childValue);
    }, { parentKey, childKey, parentValue, childValue });
    await page.goto(url, { waitUntil: "commit", timeout: 120_000 });
    await started(page);
    const result = await page.evaluate(async ({ moduleUrl, parentKey, childKey }) => {
        const storage = await import(moduleUrl);
        localStorage.removeItem(parentKey);
        localStorage.removeItem(childKey);
        const previous = Object.getOwnPropertyDescriptor(crypto, "subtle");
        Object.defineProperty(crypto, "subtle", { value: undefined, configurable: true });
        try {
            await storage.initializeDocuments();
            return null;
        } catch (error) {
            const failure = error as { code?: string; reason?: string | null };
            return { code: failure.code, reason: failure.reason };
        } finally {
            if (previous) Object.defineProperty(crypto, "subtle", previous);
            else Reflect.deleteProperty(crypto, "subtle");
        }
    }, { moduleUrl: await storageModule(page), parentKey, childKey });
    expect(result).toEqual({ code: "unsupported", reason: null });
});

test("corrupted recovery metadata blocks startup without changing stored data", async ({ page }) => {
    await page.addInitScript(({ parentKey, childKey, parentValue, childValue }) => {
        localStorage.setItem(parentKey, parentValue);
        localStorage.setItem(childKey, childValue);
    }, { parentKey, childKey, parentValue, childValue });
    await page.goto(url, { waitUntil: "commit", timeout: 120_000 });
    await started(page);
    const initial = await inspect(page, parentId, await storageModule(page));
    await page.evaluate(async copy => {
        const db = await new Promise<IDBDatabase>((resolve, reject) => {
            const request = indexedDB.open("thinoda.documents.v1");
            request.onsuccess = () => resolve(request.result);
            request.onerror = () => reject(request.error);
        });
        await new Promise<void>((resolve, reject) => {
            const tx = db.transaction("documents", "readwrite");
            const documents = tx.objectStore("documents");
            const request = documents.get(copy);
            request.onsuccess = () => {
                documents.put({ ...request.result, legacyOriginal: "/Documents/wrong" });
            };
            tx.oncomplete = () => resolve();
            tx.onabort = () => reject(tx.error);
        });
        db.close();
    }, initial.preserved);
    await page.reload({ waitUntil: "commit", timeout: 120_000 });
    const alert = page.locator("#loading");
    await expect(alert).toHaveAttribute("role", "alert", { timeout: 120_000 });
    await expect(alert).toContainText("failed verification");
    const retained = await page.evaluate(async ({ moduleUrl, parentId, copy, parentKey, childKey }) => {
        const storage = await import(moduleUrl);
        const decode = async (id: string) =>
            new TextDecoder().decode((await storage.readDocument(id)).bytes);
        return [await decode(parentId), await decode(copy), localStorage.getItem(parentKey),
            localStorage.getItem(childKey)];
    }, {
        moduleUrl: await storageModule(page), parentId, copy: initial.preserved, parentKey, childKey,
    });
    expect(retained).toEqual(["legacy parent", "legacy child", parentValue, childValue]);
});

test("a conflicting legacy-copy namespace blocks startup without overwriting data", async ({ page }) => {
    await page.goto(url, { waitUntil: "commit", timeout: 120_000 });
    await started(page);
    const reservedId = "/Documents/.thinoda-legacy";
    await page.evaluate(async ({ moduleUrl, parentId, reservedId, childKey, childValue }) => {
        const storage = await import(moduleUrl);
        await storage.writeDocument(
            parentId, new TextEncoder().encode("durable parent"), "missing", () => false,
        );
        await storage.writeDocument(
            reservedId, new TextEncoder().encode("reserved file"), "missing", () => false,
        );
        localStorage.setItem(childKey, childValue);
    }, { moduleUrl: await storageModule(page), parentId, reservedId, childKey, childValue });
    await page.reload({ waitUntil: "commit", timeout: 120_000 });
    const alert = page.locator("#loading");
    await expect(alert).toHaveAttribute("role", "alert", { timeout: 120_000 });
    await expect(alert).toContainText("file or folder");

    const retained = await page.evaluate(async ({ moduleUrl, parentId, reservedId, childKey }) => {
        const storage = await import(moduleUrl);
        const decode = async (id: string) =>
            new TextDecoder().decode((await storage.readDocument(id)).bytes);
        return [await decode(parentId), await decode(reservedId), localStorage.getItem(childKey)];
    }, { moduleUrl: await storageModule(page), parentId, reservedId, childKey });
    expect(retained).toEqual(["durable parent", "reserved file", childValue]);
});

test("an existing different conflict copy blocks startup without replacing it", async ({ page }) => {
    await page.goto(url, { waitUntil: "commit", timeout: 120_000 });
    await started(page);
    const copyId = await page.evaluate(async ({ moduleUrl, parentId, childId, childKey, childValue }) => {
        const storage = await import(moduleUrl);
        const encoder = new TextEncoder();
        const hash = async (value: string) => Array.from(
            new Uint8Array(await crypto.subtle.digest("SHA-256", encoder.encode(value))),
            byte => byte.toString(16).padStart(2, "0"),
        ).join("");
        const copy = `/Documents/.thinoda-legacy/${await hash(childId)}/${await hash("legacy child")}/child.thinoda`;
        await storage.writeDocument(parentId, encoder.encode("durable parent"), "missing", () => false);
        await storage.writeDocument(copy, encoder.encode("different copy"), "missing", () => false);
        localStorage.setItem(childKey, childValue);
        return copy;
    }, { moduleUrl: await storageModule(page), parentId, childId, childKey, childValue });
    await page.reload({ waitUntil: "commit", timeout: 120_000 });
    const alert = page.locator("#loading");
    await expect(alert).toHaveAttribute("role", "alert", { timeout: 120_000 });
    await expect(alert).toContainText("failed verification");
    const retained = await page.evaluate(async ({ moduleUrl, parentId, copyId, childKey }) => {
        const storage = await import(moduleUrl);
        const decode = async (id: string) =>
            new TextDecoder().decode((await storage.readDocument(id)).bytes);
        return [
            await decode(parentId),
            await decode(copyId),
            localStorage.getItem(childKey),
        ];
    }, { moduleUrl: await storageModule(page), parentId, copyId, childKey });
    expect(retained).toEqual(["durable parent", "different copy", childValue]);
});

test("a malformed legacy record blocks migration without disclosing or changing it", async ({ page }) => {
    const invalidKey = "thinoda.vfs:/Documents/private-filename.thinoda";
    const invalidValue = "private document contents";
    await page.addInitScript(({ parentKey, parentValue, invalidKey, invalidValue }) => {
        localStorage.setItem(parentKey, parentValue);
        localStorage.setItem(invalidKey, invalidValue);
    }, { parentKey, parentValue, invalidKey, invalidValue });
    await page.goto(url, { waitUntil: "commit", timeout: 120_000 });
    const alert = page.locator("#loading");
    await expect(alert).toHaveAttribute("role", "alert", { timeout: 120_000 });
    await expect(alert).toContainText("legacy document record is invalid");
    await expect(alert).not.toContainText("private");
    expect(await page.evaluate(async ({ moduleUrl, parentId, parentKey, invalidKey }) => {
        const storage = await import(moduleUrl);
        return [
            await storage.documentRevision(parentId),
            localStorage.getItem(parentKey),
            localStorage.getItem(invalidKey),
        ];
    }, { moduleUrl: await storageModule(page), parentId, parentKey, invalidKey }))
        .toEqual([null, parentValue, invalidValue]);
});
