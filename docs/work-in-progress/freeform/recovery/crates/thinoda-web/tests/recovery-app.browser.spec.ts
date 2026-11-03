/**
 * Application-level regression for a large browser recovery snapshot.
 *
 * Serve a freshly built Thinoda bundle on an isolated local origin, then run:
 *   THINODA_WEB_URL=http://127.0.0.1:8080/
 *   THINODA_RECOVERY_FIXTURE=/path/to/recovery-1000.json
 *   playwright test recovery-app.browser.spec.ts
 *
 * The fixture is a synthetic recovery envelope with a 1,000-object board.
 * The source is seeded only to simulate an earlier crashed session: the app
 * must discover and restore it, accept a real canvas edit, persist that edit,
 * survive an abrupt tab close, and export the recovered copy after reopening.
 */
import { expect, test, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { inflateRawSync } from "node:zlib";

test.use({ viewport: { width: 1252, height: 1278 }, deviceScaleFactor: 1 });
test.setTimeout(180_000);

function zipEntry(zip: Buffer, name: string): Buffer {
    const end = zip.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
    if (end < 0) throw new Error("Missing ZIP directory");
    const count = zip.readUInt16LE(end + 10);
    let offset = zip.readUInt32LE(end + 16);
    for (let i = 0; i < count; i++) {
        if (zip.readUInt32LE(offset) !== 0x02014b50) throw new Error("Invalid ZIP directory");
        const method = zip.readUInt16LE(offset + 10);
        const size = zip.readUInt32LE(offset + 20);
        const nameLength = zip.readUInt16LE(offset + 28);
        const extraLength = zip.readUInt16LE(offset + 30);
        const commentLength = zip.readUInt16LE(offset + 32);
        const localOffset = zip.readUInt32LE(offset + 42);
        const entryName = zip.toString("utf8", offset + 46, offset + 46 + nameLength);
        if (entryName === name) {
            if (zip.readUInt32LE(localOffset) !== 0x04034b50) throw new Error("Invalid ZIP entry");
            const start = localOffset + 30 + zip.readUInt16LE(localOffset + 26)
                + zip.readUInt16LE(localOffset + 28);
            const bytes = zip.subarray(start, start + size);
            if (method === 0) return bytes;
            if (method === 8) return inflateRawSync(bytes);
            throw new Error(`Unsupported ZIP compression method ${method}`);
        }
        offset += 46 + nameLength + extraLength + commentLength;
    }
    throw new Error(`Missing ZIP entry ${name}`);
}

async function frames(page: Page) {
    await page.evaluate(() => new Promise<void>(resolve => {
        requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
    }));
}

async function command(page: Page, name: string) {
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+Shift+p");
    await frames(page);
    const input = page.locator("input[type='text']");
    await expect(input).toBeVisible();
    await input.pressSequentially(name);
    await page.waitForTimeout(300);
    await input.press("Enter");
    await frames(page);
}

async function renameRoot(page: Page, title: string) {
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("F2");
    await page.keyboard.press("Control+a");
    await page.keyboard.type(title);
    await page.keyboard.press("Enter");
    await frames(page);
}

async function review(page: Page) {
    const panel = page.locator("#thinoda-recovery-accessibility");
    const reviewButton = panel.locator('[data-action="review"]');
    await expect.poll(async () => {
        return (await reviewButton.count()) > 0
            && await reviewButton.evaluate(element => !(element as HTMLButtonElement).hidden);
    }, { timeout: 60_000 }).toBe(true);
    await panel.locator('[role="status"]').focus();
    await expect(reviewButton).toBeVisible();
    if (await reviewButton.getAttribute("aria-expanded") !== "true") {
        await reviewButton.click();
    }
    await expect(panel.locator('[data-section="review"]')).toBeVisible();
    return panel;
}

async function boards(page: Page) {
    return page.evaluate(async () => {
        const db = await new Promise<IDBDatabase>((resolve, reject) => {
            const request = indexedDB.open("thinoda.recovery.v1");
            request.onsuccess = () => resolve(request.result);
            request.onerror = () => reject(request.error);
        });
        try {
            const records = await new Promise<any[]>((resolve, reject) => {
                const tx = db.transaction("documents", "readonly");
                const request = tx.objectStore("documents").getAll();
                request.onsuccess = () => resolve(request.result);
                request.onerror = () => reject(request.error);
            });
            return records.filter(record => !record.deleted && record.bytes instanceof ArrayBuffer)
                .map(record => {
                    try {
                        const snapshot = JSON.parse(new TextDecoder().decode(record.bytes));
                        return {
                            session: record.session as string,
                            key: record.key as string,
                            size: record.bytes.byteLength as number,
                            pendingDiscard: !!record.discardIntent,
                            objects: (snapshot.workbook.sheets[0].canvas?.objects?.length ?? 0) as number,
                            sheetTitle: snapshot.workbook.sheets[0].title as string,
                            titles: Object.values(snapshot.workbook.sheets[0].topics)
                                .map((topic: any) => topic.title as string),
                        };
                    } catch {
                        return null;
                    }
                }).filter((record): record is NonNullable<typeof record> => record !== null);
        } finally {
            db.close();
        }
    });
}

async function seed(page: Page, fixture: Buffer, base: URL, session: string, key: string) {
    await page.route("**/__recovery_seed__", route => route.fulfill({
        contentType: "text/html", body: "<!doctype html><title>Recovery fixture</title>",
    }));
    await page.route("**/__recovery_fixture__", route => route.fulfill({
        contentType: "application/json", body: fixture,
    }));
    await page.goto(new URL("__recovery_seed__", base).href);
    await page.evaluate(async ({ session, key, fixtureUrl }) => {
        const bytes = await (await fetch(fixtureUrl)).arrayBuffer();
        const db = await new Promise<IDBDatabase>((resolve, reject) => {
            const request = indexedDB.open("thinoda.recovery.v1");
            request.onupgradeneeded = () => {
                const sessions = request.result.createObjectStore("sessions", { keyPath: "id" });
                void sessions;
                const docs = request.result.createObjectStore("documents", { keyPath: "id" });
                docs.createIndex("session", "session", { unique: false });
            };
            request.onsuccess = () => resolve(request.result);
            request.onerror = () => reject(request.error);
        });
        try {
            await new Promise<void>((resolve, reject) => {
                const tx = db.transaction(["sessions", "documents"], "readwrite");
                tx.objectStore("sessions").add({
                    id: session, token: "abandoned-writer", created: Date.now(),
                    manifest: [{ key, revision: 1 }],
                });
                tx.objectStore("documents").add({
                    id: `${session}/${key}`, session, key, revision: 1, deleted: false, bytes,
                });
                tx.oncomplete = () => resolve();
                tx.onabort = () => reject(tx.error);
            });
        } finally {
            db.close();
        }
    }, { session, key, fixtureUrl: new URL("__recovery_fixture__", base).href });
}

// Allow the next recovery transaction to commit while withholding its
// completion callback. This simulates termination after IndexedDB's commit
// but before the application can observe the result; it is not BFCache proof.
async function holdNextRecoveryAcknowledgement(page: Page) {
    await page.evaluate(() => {
        const transaction = IDBDatabase.prototype.transaction;
        (window as any).__holdRecoveryAcknowledgement = true;
        (window as any).__heldRecoveryCommit = false;
        IDBDatabase.prototype.transaction = function (stores, mode, options) {
            const names = typeof stores === "string" ? [stores] : stores;
            const tx = transaction.call(this, stores, mode, options);
            if (!(window as any).__holdRecoveryAcknowledgement ||
                mode !== "readwrite" || !names.includes("documents")) return tx;
            (window as any).__holdRecoveryAcknowledgement = false;
            return new Proxy(tx, {
                get(target, property) {
                    const value = Reflect.get(target, property, target);
                    return typeof value === "function" ? value.bind(target) : value;
                },
                set(target, property, value) {
                    if (property === "oncomplete") {
                        target.oncomplete = () => { (window as any).__heldRecoveryCommit = true; };
                        return true;
                    }
                    return Reflect.set(target, property, value, target);
                },
            });
        };
    });
}

test("large recovered board survives an actual edit and abrupt close", async ({ page, context }) => {
    const fixturePath = process.env.THINODA_RECOVERY_FIXTURE;
    test.skip(!fixturePath, "Set THINODA_RECOVERY_FIXTURE to the synthetic large-board envelope");
    const fixture = await readFile(fixturePath!);
    expect(fixture.length).toBeGreaterThan(12_000_000);
    const expected = JSON.parse(fixture.toString("utf8"));
    expect(expected.workbook.sheets[0].canvas.objects).toHaveLength(1000);
    const base = new URL(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    const session = crypto.randomUUID();
    const key = "large-synthetic-board";

    await seed(page, fixture, base, session, key);
    await page.goto(base.href);
    await expect(page.locator("#loading")).toBeHidden({ timeout: 60_000 });
    const panel = await review(page);
    const restore = panel.locator(`[data-focus="restore:indexeddb:${session}"]`);
    await expect(restore).toBeEnabled({ timeout: 30_000 });
    await restore.click();
    await expect.poll(async () => (await boards(page)).some(record =>
        record.session !== session && record.objects === 1000 && record.size > 12_000_000
    ), { timeout: 60_000 }).toBe(true);

    // Add a sticky note through the actual editor; do not mutate IndexedDB to
    // simulate the edit. The copy must remain independent of the source.
    await command(page, "Sticky note");
    await page.locator("#thinoda_canvas").click({ position: { x: 620, y: 680 } });
    let editedSession = "";
    await expect.poll(async () => {
        const records = await boards(page);
        const edited = records.find(record => record.session !== session && record.objects > 1000);
        editedSession = edited?.session ?? "";
        return !!edited && records.some(record => record.session === session && record.objects === 1000);
    }, { timeout: 60_000 }).toBe(true);
    await expect(panel.locator('[role="status"]')).toContainText(
        "confirmed recovery snapshots", { timeout: 30_000 },
    );

    const unloadWarned = await page.evaluate(() => {
        const event = new Event("beforeunload", { cancelable: true });
        window.dispatchEvent(event);
        return event.defaultPrevented;
    });
    expect(unloadWarned).toBe(true);
    await page.evaluate(() => {
        window.dispatchEvent(new PageTransitionEvent("pagehide", { persisted: true }));
    });
    const observer = await context.newPage();
    await observer.goto(new URL("manifest.webmanifest", base).href);
    const lockStillHeld = await observer.evaluate(id => navigator.locks.request(
        `thinoda.recovery.v1:${id}`, { ifAvailable: true }, lock => lock === null,
    ), editedSession);
    expect(lockStillHeld).toBe(true);
    await page.evaluate(() => {
        window.dispatchEvent(new PageTransitionEvent("pageshow", { persisted: true }));
    });
    await observer.close();

    // Bypass beforeunload just as a terminated/crashed tab would; reopen with
    // the same browser profile and recover the newly committed edited copy.
    await page.close({ runBeforeUnload: false });
    const reopened = await context.newPage();
    await reopened.goto(base.href);
    await expect(reopened.locator("#loading")).toBeHidden({ timeout: 60_000 });
    const reopenedPanel = await review(reopened);
    const download = reopenedPanel.locator(
        `[data-focus^="orphan:indexeddb:${editedSession}:"]`,
    );
    await expect(download).toBeVisible({ timeout: 30_000 });
    const pendingDownload = reopened.waitForEvent("download", { timeout: 60_000 });
    await download.click();
    const file = await pendingDownload;
    const archive = await readFile((await file.path())!);
    const workbook = JSON.parse(zipEntry(archive, "content.json").toString("utf8"));
    expect(workbook.sheets[0].canvas.objects.length).toBeGreaterThan(1000);
    expect((await boards(reopened)).some(record =>
        record.session === session && record.objects === 1000
    )).toBe(true);
});

test("a failed replacement retains the original across pagehide and restart", async ({ page, context }) => {
    const fixturePath = process.env.THINODA_RECOVERY_FIXTURE;
    test.skip(!fixturePath, "Set THINODA_RECOVERY_FIXTURE to the synthetic large-board envelope");
    const fixture = await readFile(fixturePath!);
    const base = new URL(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    const sourceSession = crypto.randomUUID();
    const sourceKey = "failed-handoff";
    await seed(page, fixture, base, sourceSession, sourceKey);
    await page.goto(base.href);
    await expect(page.locator("#loading")).toBeHidden({ timeout: 60_000 });
    const panel = await review(page);
    const restore = panel.locator(`[data-focus="restore:indexeddb:${sourceSession}"]`);
    await expect(restore).toBeEnabled({ timeout: 30_000 });
    await page.evaluate(() => {
        const transaction = IDBDatabase.prototype.transaction;
        IDBDatabase.prototype.transaction = function (stores, mode, options) {
            const names = typeof stores === "string" ? [stores] : stores;
            if (mode === "readwrite" && names.includes("documents")) {
                throw new DOMException("synthetic quota", "QuotaExceededError");
            }
            return transaction.call(this, stores, mode, options);
        };
    });
    await restore.click();
    await expect(panel.locator('[role="status"]')).toContainText("full", { timeout: 30_000 });
    expect((await boards(page)).some(record =>
        record.session === sourceSession && record.key === sourceKey && record.objects === 1000
    )).toBe(true);
    await page.evaluate(() => window.dispatchEvent(new PageTransitionEvent("pagehide", { persisted: true })));
    await page.close({ runBeforeUnload: false });

    const reopened = await context.newPage();
    await reopened.goto(base.href);
    await expect(reopened.locator("#loading")).toBeHidden({ timeout: 60_000 });
    const reopenedPanel = await review(reopened);
    await expect(reopenedPanel.locator(
        `[data-focus="orphan:indexeddb:${sourceSession}:${sourceKey}"]`,
    )).toBeVisible();
});

test("a committed replacement survives termination before the restore reaches the UI", async ({ page, context }) => {
    const fixturePath = process.env.THINODA_RECOVERY_FIXTURE;
    test.skip(!fixturePath, "Set THINODA_RECOVERY_FIXTURE to the synthetic large-board envelope");
    const fixture = await readFile(fixturePath!);
    const base = new URL(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    const sourceSession = crypto.randomUUID();
    const sourceKey = "handoff-original";
    await seed(page, fixture, base, sourceSession, sourceKey);
    await page.goto(base.href);
    await expect(page.locator("#loading")).toBeHidden({ timeout: 60_000 });
    const panel = await review(page);
    const restore = panel.locator(`[data-focus="restore:indexeddb:${sourceSession}"]`);
    await expect(restore).toBeEnabled({ timeout: 30_000 });
    await holdNextRecoveryAcknowledgement(page);
    await restore.click();
    await expect.poll(() => page.evaluate(() => (window as any).__heldRecoveryCommit), {
        timeout: 60_000,
    }).toBe(true);
    const committed = await boards(page);
    const replacement = committed.find(record =>
        record.session !== sourceSession && record.objects === 1000 && record.size > 12_000_000
    );
    expect(replacement).toBeTruthy();
    expect(committed.some(record => record.session === sourceSession && record.key === sourceKey)).toBe(true);
    await page.evaluate(() => window.dispatchEvent(new PageTransitionEvent("pagehide", { persisted: true })));
    await page.close({ runBeforeUnload: false });

    const reopened = await context.newPage();
    await reopened.goto(base.href);
    await expect(reopened.locator("#loading")).toBeHidden({ timeout: 60_000 });
    const reopenedPanel = await review(reopened);
    await expect(reopenedPanel.locator(
        `[data-focus="orphan:indexeddb:${sourceSession}:${sourceKey}"]`,
    )).toBeVisible();
    await expect(reopenedPanel.locator(
        `[data-focus="orphan:indexeddb:${replacement!.session}:${replacement!.key}"]`,
    )).toBeVisible();
});

test("an unacknowledged discard is retained and verified after reload", async ({ page, context }) => {
    const fixturePath = process.env.THINODA_RECOVERY_FIXTURE;
    test.skip(!fixturePath, "Set THINODA_RECOVERY_FIXTURE to the synthetic large-board envelope");
    const fixture = await readFile(fixturePath!);
    const base = new URL(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    const sourceSession = crypto.randomUUID();
    const sourceKey = "discard-original";
    await seed(page, fixture, base, sourceSession, sourceKey);
    await page.goto(base.href);
    await expect(page.locator("#loading")).toBeHidden({ timeout: 60_000 });
    const panel = await review(page);
    const restore = panel.locator(`[data-focus="restore:indexeddb:${sourceSession}"]`);
    await expect(restore).toBeEnabled({ timeout: 30_000 });
    await restore.click();
    await expect.poll(async () => (await boards(page)).some(record =>
        record.session !== sourceSession && record.objects === 1000
    ), { timeout: 60_000 }).toBe(true);
    await expect(panel.locator('[role="status"]')).toContainText(
        "confirmed recovery snapshots", { timeout: 60_000 },
    );
    await review(page);
    const discard = panel.locator(`[data-focus="discard:indexeddb:${sourceSession}:${sourceKey}"]`);
    const confirm = panel.locator(`[data-focus="confirm:indexeddb:${sourceSession}:${sourceKey}"]`);
    await discard.click();
    await holdNextRecoveryAcknowledgement(page);
    await confirm.click();
    await expect.poll(() => page.evaluate(() => (window as any).__heldRecoveryCommit), {
        timeout: 30_000,
    }).toBe(true);
    await expect.poll(async () => (await boards(page)).some(record =>
        record.session === sourceSession && record.key === sourceKey && record.pendingDiscard
    ), { timeout: 30_000 }).toBe(true);
    const afterDiscard = await boards(page);
    const replacement = afterDiscard.find(record => record.session !== sourceSession && record.objects === 1000);
    expect(replacement).toBeTruthy();
    expect(afterDiscard.some(record => record.session === sourceSession && record.key === sourceKey)).toBe(true);
    await page.evaluate(() => window.dispatchEvent(new PageTransitionEvent("pagehide", { persisted: true })));
    await page.close({ runBeforeUnload: false });

    const reopened = await context.newPage();
    await reopened.goto(base.href);
    await expect(reopened.locator("#loading")).toBeHidden({ timeout: 60_000 });
    const reopenedPanel = await review(reopened);
    await expect(reopenedPanel.locator(
        `[data-focus="orphan:indexeddb:${replacement!.session}:${replacement!.key}"]`,
    )).toBeVisible();
    const verify = reopenedPanel.locator(`[data-focus="verify:indexeddb:${sourceSession}:${sourceKey}"]`);
    const keep = reopenedPanel.locator(`[data-focus="keep:indexeddb:${sourceSession}:${sourceKey}"]`);
    await expect(verify).toBeVisible();
    await expect(keep).toBeVisible();
    await expect(reopenedPanel.locator(`[data-focus="restore:indexeddb:${sourceSession}"]`)).toBeDisabled();
    await verify.click();
    await expect.poll(async () => (await boards(reopened)).some(record =>
        record.session === sourceSession && record.key === sourceKey
    ), { timeout: 30_000 }).toBe(false);
    expect((await boards(reopened)).some(record =>
        record.session === replacement!.session && record.key === replacement!.key
    )).toBe(true);
});

test("discovery gates edits until prior snapshots are known", async ({ page }) => {
    const fixturePath = process.env.THINODA_RECOVERY_FIXTURE;
    test.skip(!fixturePath, "Set THINODA_RECOVERY_FIXTURE to the synthetic large-board envelope");
    const fixture = await readFile(fixturePath!);
    const base = new URL(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    const session = crypto.randomUUID();
    await seed(page, fixture, base, session, "late-original");
    await page.addInitScript(({ sessionId }) => {
        const locks = navigator.locks;
        Object.defineProperty(navigator, "locks", {
            configurable: true,
            value: {
                request(name: string, options: LockOptions, callback: LockGrantedCallback) {
                    if (name.endsWith(`:${sessionId}`)) {
                        return new Promise((resolve, reject) => {
                            setTimeout(() => locks.request(name, options, callback).then(resolve, reject), 12_000);
                        });
                    }
                    return locks.request(name, options, callback);
                },
            },
        });
    }, { sessionId: session });
    await page.goto(base.href);
    await expect(page.locator("#loading")).toBeHidden({ timeout: 60_000 });
    const panel = page.locator("#thinoda-recovery-accessibility");
    await expect(panel).toHaveAttribute("data-phase", "discovery-pending", { timeout: 30_000 });
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+n");
    await frames(page);
    await renameRoot(page, "Must not edit during discovery");
    await page.waitForTimeout(1500);
    expect(await panel.getAttribute("data-phase")).toBe("discovery-pending");
    expect((await boards(page)).some(record =>
        record.session !== session && record.titles.includes("Must not edit during discovery")
    )).toBe(false);
    await expect.poll(
        () => panel.locator('[data-action="review"]').evaluate(
            element => !(element as HTMLButtonElement).hidden,
        ),
        { timeout: 30_000 },
    ).toBe(true);
    expect(await panel.locator('[data-action="review"]').getAttribute("aria-expanded")).toBe("false");
    expect((await boards(page)).some(record => record.session === session)).toBe(true);
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+n");
    await frames(page);
    await renameRoot(page, "Live edit after discovery");
    await expect.poll(async () => (await boards(page)).some(record =>
        record.session !== session && record.titles.includes("Live edit after discovery")
    ), { timeout: 30_000 }).toBe(true);
    expect((await boards(page)).some(record => record.session === session)).toBe(true);
});

test("pagehide fences an unfinished discovery until an explicit retry", async ({ page }) => {
    const fixturePath = process.env.THINODA_RECOVERY_FIXTURE;
    test.skip(!fixturePath, "Set THINODA_RECOVERY_FIXTURE to the synthetic large-board envelope");
    const fixture = await readFile(fixturePath!);
    const base = new URL(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    const session = crypto.randomUUID();
    await seed(page, fixture, base, session, "hidden-original");
    await page.addInitScript(({ sessionId }) => {
        const locks = navigator.locks;
        Object.defineProperty(navigator, "locks", {
            configurable: true,
            value: {
                request(name: string, options: LockOptions, callback: LockGrantedCallback) {
                    if (name.endsWith(`:${sessionId}`)) {
                        return new Promise((resolve, reject) => {
                            setTimeout(() => locks.request(name, options, callback).then(resolve, reject), 4000);
                        });
                    }
                    return locks.request(name, options, callback);
                },
            },
        });
    }, { sessionId: session });
    await page.goto(base.href);
    await expect(page.locator("#loading")).toBeHidden({ timeout: 60_000 });
    const panel = page.locator("#thinoda-recovery-accessibility");
    await expect(panel).toHaveAttribute("data-phase", "discovery-pending", { timeout: 30_000 });
    await page.evaluate(() => {
        window.dispatchEvent(new PageTransitionEvent("pagehide", { persisted: true }));
        window.dispatchEvent(new PageTransitionEvent("pageshow", { persisted: true }));
    });
    await expect(panel).toHaveAttribute("data-phase", "deferred", { timeout: 30_000 });
    await expect(panel.locator('[data-action="review"]')).toBeHidden();
    expect((await boards(page)).some(record => record.session === session)).toBe(true);
    await panel.locator('[role="status"]').focus();
    await panel.locator('[data-action="retry"]').click();
    await review(page);
    await expect(panel.locator(`[data-focus="restore:indexeddb:${session}"]`)).toBeEnabled({
        timeout: 30_000,
    });
    expect((await boards(page)).some(record => record.session === session)).toBe(true);
});

test("oversized metadata remains unverified and the raw source is available", async ({ page }) => {
    const fixturePath = process.env.THINODA_RECOVERY_FIXTURE;
    test.skip(!fixturePath, "Set THINODA_RECOVERY_FIXTURE to the synthetic large-board envelope");
    const snapshot = JSON.parse((await readFile(fixturePath!)).toString("utf8"));
    snapshot.meta.title = "x".repeat(70_000);
    snapshot.workbook.sheets[0].canvas.objects = [];
    snapshot.workbook.sheets[0].resources.entries = {};
    for (const topic of Object.values(snapshot.workbook.sheets[0].topics) as any[]) {
        delete topic.image;
        delete topic.image_size;
    }
    const fixture = Buffer.from(JSON.stringify(snapshot), "utf8");
    const base = new URL(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    const session = crypto.randomUUID();
    const key = "unverified-metadata";
    await seed(page, fixture, base, session, key);
    await page.goto(base.href);
    await expect(page.locator("#loading")).toBeHidden({ timeout: 60_000 });
    const panel = await review(page);
    await expect(panel).toHaveAttribute("data-phase", "deferred");
    await expect(panel.locator('[role="status"]')).toContainText("could not be verified");
    await expect(panel.locator(`[data-focus="restore:indexeddb:${session}"]`)).toBeDisabled();
    const requested = page.waitForEvent("download", { timeout: 30_000 });
    await panel.locator(`[data-focus="raw:indexeddb:${session}:${key}"]`).click();
    const download = await requested;
    expect(await readFile((await download.path())!)).toEqual(fixture);
    expect((await boards(page)).some(record => record.session === session)).toBe(true);
});

test("a confirmed snapshot is recoverable after an offline shell reload", async ({ page, context }) => {
    const base = new URL(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await page.goto(base.href);
    await expect(page.locator("#loading")).toBeHidden({ timeout: 60_000 });
    const supportsWorker = await page.evaluate(() => "serviceWorker" in navigator);
    test.skip(!supportsWorker, "The browser does not support service workers");
    await page.evaluate(() => navigator.serviceWorker.ready.then(() => undefined));
    await page.reload();
    await expect(page.locator("#loading")).toBeHidden({ timeout: 60_000 });
    await expect.poll(() => page.evaluate(() => !!navigator.serviceWorker.controller), {
        timeout: 30_000,
    }).toBe(true);
    const panel = page.locator("#thinoda-recovery-accessibility");
    await expect(panel).not.toHaveAttribute(
        "data-phase", /waiting-writer|discovery-pending|deferred|unavailable/, { timeout: 30_000 },
    );
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+n");
    await frames(page);
    await renameRoot(page, "Synthetic offline recovery");
    let session = "";
    await expect.poll(async () => {
        const record = (await boards(page)).find(item => item.titles.includes("Synthetic offline recovery"));
        session = record?.session ?? "";
        return !!record;
    }, { timeout: 30_000 }).toBe(true);
    await expect(panel.locator('[role="status"]')).toContainText(
        "confirmed recovery snapshots", { timeout: 30_000 },
    );
    await page.close({ runBeforeUnload: false });
    try {
        await context.setOffline(true);
        const offline = await context.newPage();
        const response = await offline.goto(base.href, { waitUntil: "domcontentloaded" });
        expect(response?.ok()).toBe(true);
        await expect(offline.locator("#loading")).toBeHidden({ timeout: 60_000 });
        const recovered = await review(offline);
        await expect(recovered.locator(`[data-focus^="raw:indexeddb:${session}:"]`)).toBeVisible({
            timeout: 30_000,
        });
        expect((await boards(offline)).some(record =>
            record.session === session && record.titles.includes("Synthetic offline recovery")
        )).toBe(true);
    } finally {
        await context.setOffline(false);
    }
});

test("saving tombstones the snapshot and undo queues a fresh recovery", async ({ page }) => {
    const base = new URL(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await page.goto(base.href);
    await expect(page.locator("#loading")).toBeHidden({ timeout: 60_000 });
    const panel = page.locator("#thinoda-recovery-accessibility");
    await expect(panel).toBeAttached({ timeout: 30_000 });
    await expect(panel).not.toHaveAttribute(
        "data-phase", /waiting-writer|discovery-pending|deferred|unavailable/, { timeout: 30_000 },
    );
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+n");
    await frames(page);
    await renameRoot(page, "Recovery before save");
    let session = "";
    let key = "";
    await expect.poll(async () => {
        const record = (await boards(page)).find(item => item.titles.includes("Recovery before save"));
        session = record?.session ?? "";
        key = record?.key ?? "";
        return !!record;
    }, { timeout: 30_000 }).toBe(true);
    await expect(panel.locator('[role="status"]')).toContainText(
        "confirmed recovery snapshots", { timeout: 30_000 },
    );

    await renameRoot(page, "Recovery saved version");
    await page.evaluate(() => { window.prompt = () => "recovery-save-undo.thinoda"; });
    const download = page.waitForEvent("download", { timeout: 30_000 });
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+Shift+s");
    await download;
    await expect.poll(async () => (await boards(page)).some(record =>
        record.session === session && record.key === key
    ), { timeout: 30_000 }).toBe(false);

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+z");
    await expect.poll(async () => (await boards(page)).some(record =>
        record.session === session && record.key === key && record.titles.includes("Recovery before save")
    ), { timeout: 30_000 }).toBe(true);
    await expect(panel.locator('[role="status"]')).toContainText(
        "confirmed recovery snapshots", { timeout: 30_000 },
    );
});

test("a real BFCache navigation keeps a dirty writer fenced when the browser caches the page", async ({ page, context }) => {
    const base = new URL(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await page.goto(base.href);
    await expect(page.locator("#loading")).toBeHidden({ timeout: 60_000 });
    const panel = page.locator("#thinoda-recovery-accessibility");
    await expect(panel).toBeAttached({ timeout: 30_000 });
    await expect(panel).not.toHaveAttribute("data-phase", "waiting-writer", { timeout: 30_000 });
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+n");
    await frames(page);
    await renameRoot(page, "BFCache recovery draft");
    let writer = "";
    await expect.poll(async () => {
        const record = (await boards(page)).find(item => item.titles.includes("BFCache recovery draft"));
        writer = record?.session ?? "";
        return !!record;
    }, { timeout: 30_000 }).toBe(true);
    await expect(panel.locator('[role="status"]')).toContainText(
        "confirmed recovery snapshots", { timeout: 30_000 },
    );
    await page.evaluate(() => {
        (window as any).__actualBfCacheRestore = false;
        window.addEventListener("pageshow", event => {
            if (event.persisted) (window as any).__actualBfCacheRestore = true;
        });
    });
    page.once("dialog", dialog => dialog.accept());
    await page.goto(new URL("manifest.webmanifest", base).href);
    const observer = await context.newPage();
    await observer.goto(new URL("manifest.webmanifest", base).href);
    const lockStillHeld = await observer.evaluate(id => navigator.locks.request(
        `thinoda.recovery.v1:${id}`, { ifAvailable: true }, lock => lock === null,
    ), writer);
    await page.goBack({ waitUntil: "domcontentloaded" });
    const restoredFromBfCache = await page.evaluate(() => (window as any).__actualBfCacheRestore === true);
    await observer.close();
    test.skip(!restoredFromBfCache, "This browser did not restore the page from BFCache");
    expect(lockStillHeld).toBe(true);
    await renameRoot(page, "BFCache resumed edit");
    await expect.poll(async () => (await boards(page)).some(record =>
        record.session === writer && record.titles.includes("BFCache resumed edit")
    ), { timeout: 30_000 }).toBe(true);
    await expect(page.locator("#thinoda-recovery-accessibility [role='status']")).toContainText(
        "confirmed recovery snapshots", { timeout: 30_000 },
    );
});

test("legacy recovery survives failed migration and retains its non-atomic source", async ({ page }) => {
    const fixturePath = process.env.THINODA_RECOVERY_FIXTURE;
    test.skip(!fixturePath, "Set THINODA_RECOVERY_FIXTURE to the synthetic large-board envelope");
    const snapshot = JSON.parse((await readFile(fixturePath!)).toString("utf8"));
    snapshot.meta.title = 'Synthetic legacy } "quoted" \\ board';
    const sheet = snapshot.workbook.sheets[0];
    sheet.title = "Synthetic legacy board";
    sheet.canvas.objects = [];
    sheet.resources.entries = {};
    for (const topic of Object.values(sheet.topics) as any[]) {
        delete topic.image;
        delete topic.image_size;
    }
    const bytes = Buffer.from(JSON.stringify(snapshot), "utf8");
    expect(bytes.length).toBeLessThan(100_000);
    const encoded = bytes.toString("base64");
    const base = new URL(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    const session = crypto.randomUUID();
    const key = "legacy-synthetic-board";
    const storageKey = `thinoda.vfs:/thinoda/data/recovery/${session}/docs/${key}.json`;
    const legacyEnvelope = JSON.stringify({ m: 0, l: bytes.length, d: encoded });
    await page.route("**/__legacy_seed__", route => route.fulfill({
        contentType: "text/html", body: "<!doctype html><title>Legacy recovery fixture</title>",
    }));
    await page.goto(new URL("__legacy_seed__", base).href);
    await page.evaluate(({ storageKey, legacyEnvelope }) => {
        localStorage.setItem(storageKey, legacyEnvelope);
    }, { storageKey, legacyEnvelope });
    await page.addInitScript(() => {
        const transaction = IDBDatabase.prototype.transaction;
        IDBDatabase.prototype.transaction = function (stores, mode, options) {
            const names = typeof stores === "string" ? [stores] : stores;
            if ((window as any).__failRecoveryWrites && mode === "readwrite" && names.includes("documents")) {
                throw new DOMException("synthetic quota", "QuotaExceededError");
            }
            return transaction.call(this, stores, mode, options);
        };
    });
    await page.goto(base.href);
    await expect(page.locator("#loading")).toBeHidden({ timeout: 60_000 });
    const panel = await review(page);
    await expect(panel).toContainText('Synthetic legacy } "quoted" \\ board');
    const restore = panel.locator(`[data-focus="restore:legacy:${session}"]`);
    await expect(restore).toBeEnabled({ timeout: 30_000 });
    await page.evaluate(() => { (window as any).__failRecoveryWrites = true; });
    await restore.click();
    await expect(panel.locator('[role="status"]')).toContainText("full", { timeout: 30_000 });
    expect(await page.evaluate(key => localStorage.getItem(key), storageKey)).toBe(legacyEnvelope);

    await page.evaluate(() => { (window as any).__failRecoveryWrites = false; });
    await panel.locator('[role="status"]').focus();
    await panel.locator('[data-action="retry"]').click();
    await expect(restore).toBeEnabled({ timeout: 30_000 });
    await review(page);
    await restore.click();
    await expect.poll(async () => (await boards(page)).some(record =>
        record.sheetTitle === "Synthetic legacy board" && record.objects === 0
    ), { timeout: 30_000 }).toBe(true);
    expect(await page.evaluate(key => localStorage.getItem(key), storageKey)).toBe(legacyEnvelope);
    await review(page);

    const discard = panel.locator(`[data-focus="discard:legacy:${session}:${key}"]`);
    const raw = panel.locator(`[data-focus="raw:legacy:${session}:${key}"]`);
    await panel.locator('[role="status"]').focus();
    await expect(discard).toHaveCount(0);
    await expect(panel).toContainText("older tabs cannot be excluded safely");
    const requested = page.waitForEvent("download", { timeout: 30_000 });
    await raw.click();
    const download = await requested;
    expect(await readFile((await download.path())!, "utf8")).toBe(legacyEnvelope);
    expect(await page.evaluate(key => localStorage.getItem(key), storageKey)).toBe(legacyEnvelope);
    expect((await boards(page)).some(record =>
        record.sheetTitle === "Synthetic legacy board" && record.objects === 0
    )).toBe(true);
});
