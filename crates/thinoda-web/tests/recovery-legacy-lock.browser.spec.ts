import { expect, test, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { command, frames, imagePicker, largeJpeg, saveAs, zipEntry } from "./board-browser-helpers";

test.use({
    viewport: { width: 1252, height: 1278 },
    deviceScaleFactor: 1,
    locale: "en-US",
    serviceWorkers: "block",
    acceptDownloads: true,
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
});
test.setTimeout(180_000);

async function recoverySnapshots(page: Page) {
    return page.evaluate(() => Object.keys(localStorage)
        .filter((key) => /^thinoda\.vfs:.*\/recovery\/[^/]+\/docs\/[^/]+\.json$/.test(key))
        .map((key) => ({ key, value: localStorage.getItem(key) })));
}

function snapshotContents(value: string | null) {
    if (!value) throw new Error("Missing recovery snapshot");
    return (JSON.parse(value) as { d: string }).d;
}

async function waitForSnapshot(page: Page, text: string) {
    await expect.poll(async () => {
        const snapshots = await recoverySnapshots(page);
        if (snapshots.length !== 1 || !snapshots[0].value) return false;
        const entry: { d: string } = JSON.parse(snapshots[0].value);
        return Buffer.from(entry.d, "base64").toString("utf8").includes(text);
    }, { timeout: 20_000 }).toBe(true);
    const [snapshot] = await recoverySnapshots(page);
    if (!snapshot?.value) throw new Error("Missing recovery snapshot");
    return { key: snapshot.key, value: snapshot.value };
}

type PendingInk = {
    gesture: {
        mode: string;
        pointer: { kind: string };
        source_current: boolean;
        was_held: boolean;
        samples: { x_bits: number; y_bits: number; pressure_bits: number | null }[];
    } | null;
};

async function pendingInkSnapshots(page: Page) {
    return (await recoverySnapshots(page)).flatMap(({ key, value }) => {
        if (!value) return [];
        const entry: { d: string } = JSON.parse(value);
        const snapshot: { workbook: unknown } = JSON.parse(Buffer.from(entry.d, "base64").toString("utf8"));
        if (!Array.isArray(snapshot.workbook) || snapshot.workbook[0] !== "THINODA-RECOVERY-INK-1") return [];
        return [{
            key,
            value,
            workbook: snapshot.workbook[1] as {
                sheets: { canvas?: { objects: { kind: { ink?: unknown } }[] } }[];
            },
            ink: JSON.parse(snapshot.workbook[2]) as PendingInk,
        }];
    });
}

async function waitForPendingInk(page: Page, key?: string) {
    await expect.poll(async () => {
        const snapshot = (await pendingInkSnapshots(page)).find(item => !key || item.key === key);
        return snapshot?.ink.gesture?.samples.length ?? 0;
    }, { timeout: 20_000 }).toBeGreaterThanOrEqual(2);
    const snapshot = (await pendingInkSnapshots(page)).find(item => !key || item.key === key);
    if (!snapshot?.ink.gesture) throw new Error("Missing pending ink snapshot");
    return snapshot;
}

async function startHeldPen(page: Page) {
    await command(page, "Pen");
    const cdp = await page.context().newCDPSession(page);
    await cdp.send("Input.dispatchMouseEvent", {
        type: "mousePressed", x: 350, y: 800, button: "left", buttons: 1,
        clickCount: 1, pointerType: "pen", force: 0.2,
    });
    await frames(page);
    await cdp.send("Input.dispatchMouseEvent", {
        type: "mouseMoved", x: 500, y: 850, button: "left", buttons: 1,
        pointerType: "pen", force: 0.6,
    });
    await frames(page);
    return cdp;
}

async function addSticky(page: Page, text: string, x: number) {
    await command(page, "Sticky note");
    await page.mouse.click(x, 400);
    await frames(page);
    const input = page.locator("input[type='text']");
    await expect(input).toBeFocused();
    await input.pressSequentially(text);
    await page.keyboard.press("Enter");
    await frames(page);
}

async function newBoard(page: Page) {
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+n");
    await frames(page);
    await command(page, "Hide central topic");
    await page.keyboard.press("Escape");
    await frames(page);
    await addSticky(page, "Before takeover", 220);
}

async function startBoard(page: Page) {
    await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    await newBoard(page);
    return waitForSnapshot(page, "Before takeover");
}

async function recoveryTabs(page: Page, count: number) {
    await page.locator("#thinoda_canvas").focus();
    for (let i = 0; i < count; i++) {
        await page.keyboard.press("Tab");
        await frames(page);
    }
}

const rescueError = "Raw download failed. Keep recovery data, check browser storage and downloads, close other tabs, and try again.";
const rescueRequested = "Download requested. Check browser downloads; the original recovery data was kept.";
const rescueBase = "thinoda.vfs:/thinoda/data/recovery/rescue-fixture/docs/";
const sourceKey = "thinoda.vfs:/Documents/original.thinoda";
const sourceValue = JSON.stringify({ m: 1, l: 8, d: "b3JpZ2luYWw=" });

async function openRescueFixture(page: Page, records: { key: string; value: string }[]) {
    await page.addInitScript((records) => {
        if (sessionStorage.getItem("rescue-fixture-seeded")) return;
        for (const { key, value } of records) localStorage.setItem(key, value);
        sessionStorage.setItem("rescue-fixture-seeded", "yes");
    }, records);
    page.on("dialog", (dialog) => dialog.accept());
    await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    await expect(page.locator("#thinoda-recovery-discovery")).not.toBeEmpty();
    // Install a separate legacy document after migration has completed.
    await page.evaluate(({ key, value }) => localStorage.setItem(key, value), {
        key: sourceKey, value: sourceValue,
    });
    await frames(page);
}

function recoveryWarnings(page: Page) {
    const warnings: string[] = [];
    page.on("console", (message) => {
        if (message.text().includes("crash recovery unavailable:")) warnings.push(message.text());
    });
    return warnings;
}

test("held ink survives browser reload and repeated recovery handoffs", async ({ page, browserName }) => {
    test.skip(browserName !== "chromium", "Pen input synthesis requires the Chromium DevTools protocol");
    page.on("dialog", dialog => dialog.accept());
    const initial = await startBoard(page);
    const cdp = await startHeldPen(page);
    const captured = await waitForPendingInk(page, initial.key);
    expect(captured.ink.gesture?.mode).toBe("pen");
    expect(captured.ink.gesture?.pointer.kind).toBe("pen");
    expect(captured.ink.gesture?.was_held).toBe(true);
    expect(captured.ink.gesture?.source_current).toBe(true);
    const pressures = captured.ink.gesture!.samples.map(sample => {
        if (sample.pressure_bits === null) throw new Error("Missing pen pressure");
        const bytes = Buffer.allocUnsafe(4);
        bytes.writeUInt32LE(sample.pressure_bits);
        return bytes.readFloatLE();
    });
    expect(pressures[0]).toBeCloseTo(0.2, 2);
    expect(pressures.at(-1)).toBeCloseTo(0.6, 2);
    expect((captured.workbook.sheets[0].canvas?.objects ?? []).some(object => object.kind.ink)).toBe(false);
    await cdp.detach();

    let previousKey = initial.key;
    for (let restart = 0; restart < 2; restart++) {
        await page.reload();
        await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
        const orphan = await waitForPendingInk(page, previousKey);
        await recoveryTabs(page, 4);
        await page.keyboard.press("Enter");
        await expect.poll(async () => {
            const snapshots = await pendingInkSnapshots(page);
            return snapshots.some(item => item.key !== previousKey)
                && !snapshots.some(item => item.key === previousKey);
        }, { timeout: 20_000 }).toBe(true);
        const handedOff = (await pendingInkSnapshots(page)).find(item => item.key !== previousKey);
        if (!handedOff?.ink.gesture) throw new Error("Missing handed-off ink");
        expect(handedOff.ink.gesture.samples).toEqual(orphan.ink.gesture?.samples);
        expect(handedOff.ink.gesture.source_current).toBe(true);
        expect((handedOff.workbook.sheets[0].canvas?.objects ?? []).some(object => object.kind.ink)).toBe(false);
        previousKey = handedOff.key;
    }

    await page.evaluate(() => { window.prompt = () => "unexpected-partial-ink.thinoda"; });
    const download = page.waitForEvent("download", { timeout: 2_000 }).then(() => true, () => false);
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+Shift+s");
    expect(await download).toBe(false);
});

test("ink recovery retries after a browser quota failure without replacing the prior snapshot", async ({ page, browserName }) => {
    test.skip(browserName !== "chromium", "Pen input synthesis requires the Chromium DevTools protocol");
    const warnings = recoveryWarnings(page);
    const initial = await startBoard(page);
    await page.evaluate((snapshotKey) => {
        const original = Storage.prototype.setItem;
        Reflect.set(window, "__restoreInkRecoveryStorage", () => { Storage.prototype.setItem = original; });
        Storage.prototype.setItem = function (key, value) {
            if (key === snapshotKey) throw new DOMException("synthetic quota", "QuotaExceededError");
            return original.call(this, key, value);
        };
    }, initial.key);
    const cdp = await startHeldPen(page);
    await expect.poll(() => warnings.length, { timeout: 10_000 }).toBe(1);
    await expect(page.locator("#thinoda-recovery-availability")).toHaveText(
        "Crash recovery is unavailable. Save or download your work.",
    );
    expect(snapshotContents(await page.evaluate(key => localStorage.getItem(key), initial.key)))
        .toBe(snapshotContents(initial.value));
    await page.evaluate(() => (Reflect.get(window, "__restoreInkRecoveryStorage") as () => void)());
    const recovered = await waitForPendingInk(page, initial.key);
    expect(recovered.ink.gesture?.was_held).toBe(true);
    expect(recovered.ink.gesture?.source_current).toBe(true);
    await expect(page.locator("#thinoda-recovery-availability")).toBeEmpty();
    await cdp.detach();
});

test("real browser storage exhaustion preserves and retries pending ink", async ({ page, browserName }) => {
    test.skip(browserName !== "chromium", "Pen input synthesis requires the Chromium DevTools protocol");
    const initial = await startBoard(page);
    await page.evaluate((snapshotKey) => {
        const failures: string[] = [];
        Reflect.set(window, "__inkQuotaFailures", failures);
        const original = Storage.prototype.setItem;
        Storage.prototype.setItem = function (key, value) {
            try {
                return original.call(this, key, value);
            } catch (error) {
                if (key === snapshotKey && error instanceof DOMException) failures.push(error.name);
                throw error;
            }
        };
        let low = 0;
        let high = 10 * 1024 * 1024;
        while (low + 1 < high) {
            const size = Math.floor((low + high) / 2);
            try {
                localStorage.setItem("__thinoda_ink_quota_fixture", "x".repeat(size));
                low = size;
            } catch (error) {
                if (!(error instanceof DOMException) || error.name !== "QuotaExceededError") throw error;
                high = size;
            }
        }
        if (low === 0) throw new Error("Could not fill browser storage");
    }, initial.key);

    const cdp = await startHeldPen(page);
    await expect.poll(
        () => page.evaluate(() => (Reflect.get(window, "__inkQuotaFailures") as string[])
            .includes("QuotaExceededError")),
        { timeout: 20_000 },
    ).toBe(true);
    await expect(page.locator("#thinoda-recovery-availability")).toHaveText(
        "Crash recovery is unavailable. Save or download your work.",
    );
    expect(snapshotContents(await page.evaluate(key => localStorage.getItem(key), initial.key)))
        .toBe(snapshotContents(initial.value));
    await page.evaluate(() => localStorage.removeItem("__thinoda_ink_quota_fixture"));
    const recovered = await waitForPendingInk(page, initial.key);
    expect(recovered.ink.gesture?.source_current).toBe(true);
    await expect(page.locator("#thinoda-recovery-availability")).toBeEmpty();
    await cdp.detach();
});

test("a competing browser tab claims pending ink without an old page overwriting it", async ({ page, context, browserName }) => {
    test.skip(browserName !== "chromium", "Pen input synthesis requires the Chromium DevTools protocol");
    const warnings = recoveryWarnings(page);
    const initial = await startBoard(page);
    const cdp = await startHeldPen(page);
    await waitForPendingInk(page, initial.key);
    const lockKey = initial.key.replace(/\/docs\/[^/]+\.json$/, "/lock");
    const lockOwner = () => page.evaluate((key) => {
        const value = localStorage.getItem(key);
        if (!value) return null;
        const entry: { d: string } = JSON.parse(value);
        return (JSON.parse(atob(entry.d)) as { owner: string }).owner;
    }, lockKey);
    const originalOwner = await lockOwner();
    expect(originalOwner).not.toBeNull();
    await page.evaluate(() => window.dispatchEvent(new PageTransitionEvent("pagehide", { persisted: true })));
    const retained = await page.evaluate(key => localStorage.getItem(key), initial.key);
    expect(retained).not.toBeNull();
    expect(await page.evaluate(key => localStorage.getItem(key), lockKey)).toBeNull();

    const other = await context.newPage();
    await other.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(other.locator("#loading")).toBeHidden({ timeout: 120_000 });
    await expect.poll(lockOwner).not.toBeNull();
    const competingOwner = await lockOwner();
    expect(competingOwner).not.toBe(originalOwner);
    await page.evaluate(() => window.dispatchEvent(new PageTransitionEvent("pageshow", { persisted: true })));
    await page.bringToFront();
    await cdp.send("Input.dispatchMouseEvent", {
        type: "mouseMoved", x: 550, y: 875, button: "left", buttons: 1,
        pointerType: "pen", force: 0.8,
    });
    await expect.poll(() => warnings.length, { timeout: 10_000 }).toBe(1);
    await page.waitForTimeout(2_000);
    expect(await page.evaluate(key => localStorage.getItem(key), initial.key)).toBe(retained);
    expect(await lockOwner()).toBe(competingOwner);
    await cdp.detach();
    await other.close();
});

for (const fault of ["fresh owner", "stale owner", "missing lock", "corrupt lock"] as const) {
    test(`a ${fault} cannot be overwritten by the old owner`, async ({ page }) => {
        const warnings = recoveryWarnings(page);
        const snapshot = await startBoard(page);
        const lockKey = snapshot.key.replace(/\/docs\/[^/]+\.json$/, "/lock");
        const changedLock = await page.evaluate(({ key, fault }) => {
            const value = localStorage.getItem(key);
            if (!value) throw new Error("Missing recovery lease");
            if (fault === "missing lock") {
                localStorage.removeItem(key);
                return null;
            }
            if (fault === "corrupt lock") {
                localStorage.setItem(key, "corrupt lease");
                return "corrupt lease";
            }
            const entry: { m: number; l: number; d: string } = JSON.parse(value);
            const record: { owner: string; heartbeat_ms: number } = JSON.parse(atob(entry.d));
            record.owner = "a-different-tab";
            record.heartbeat_ms = Date.now() - (fault === "stale owner" ? 300_000 : 0);
            const encoded = JSON.stringify(record);
            entry.m = record.heartbeat_ms;
            entry.l = encoded.length;
            entry.d = btoa(encoded);
            const replacement = JSON.stringify(entry);
            localStorage.setItem(key, replacement);
            return replacement;
        }, { key: lockKey, fault });

        await command(page, "Shape tool");
        await page.mouse.click(650, 400);
        await frames(page);
        await expect.poll(() => warnings.length, { timeout: 10_000 }).toBe(1);
        await expect(page.locator("#thinoda-recovery-availability")).toHaveText(
            "Crash recovery is unavailable. Save or download your work.",
        );
        await page.waitForTimeout(3_000);
        const afterEdit = await page.evaluate((key) => localStorage.getItem(key), snapshot.key);
        const archive = await saveAs(page, `recovery-${fault.replaceAll(" ", "-")}.thinoda`);
        const content = JSON.parse(zipEntry(archive, "content.json").toString("utf8")) as {
            sheets: { canvas: { objects: { kind: { shape?: unknown } }[] } }[];
        };
        expect(content.sheets[0].canvas.objects.some((object) => object.kind.shape)).toBe(true);
        await page.waitForTimeout(1_000);
        expect.soft(afterEdit, "the newer edit must not overwrite the retained snapshot").toBe(snapshot.value);
        expect.soft(await page.evaluate((key) => localStorage.getItem(key), snapshot.key),
            "saving must not remove the retained snapshot").toBe(snapshot.value);
        expect(await page.evaluate((key) => localStorage.getItem(key), lockKey)).toBe(changedLock);
        expect(warnings).toHaveLength(1);
    });
}

test("a quota failure retains the previous snapshot and retries queued edits", async ({ page }) => {
    const warnings = recoveryWarnings(page);
    const snapshot = await startBoard(page);
    await page.evaluate((snapshotKey) => {
        const original = Storage.prototype.setItem;
        Object.defineProperty(window, "__restoreRecoveryStorage", {
            configurable: true,
            value: () => { Storage.prototype.setItem = original; },
        });
        Storage.prototype.setItem = function (key, value) {
            if (key === snapshotKey) throw new DOMException("synthetic quota", "QuotaExceededError");
            return original.call(this, key, value);
        };
    }, snapshot.key);
    await addSticky(page, "After quota retry", 650);
    await expect.poll(() => warnings.length, { timeout: 10_000 }).toBe(1);
    await expect(page.locator("#thinoda-recovery-availability")).toHaveText(
        "Crash recovery is unavailable. Save or download your work.",
    );
    expect(await page.evaluate((key) => localStorage.getItem(key), snapshot.key)).toBe(snapshot.value);
    await page.waitForTimeout(1_500);
    expect(warnings).toHaveLength(1);
    await page.evaluate(() => {
        const restore = Reflect.get(window, "__restoreRecoveryStorage") as () => void;
        restore();
    });
    await waitForSnapshot(page, "After quota retry");
    await expect(page.locator("#thinoda-recovery-availability")).toBeEmpty();
});

test("failed cleanup of a saved snapshot is retried without another edit", async ({ page }) => {
    const snapshot = await startBoard(page);
    await page.evaluate((snapshotKey) => {
        const original = Storage.prototype.removeItem;
        const failures = { count: 0 };
        Reflect.set(window, "__recoveryRemovalFailures", failures);
        Reflect.set(window, "__restoreRecoveryRemoval", () => { Storage.prototype.removeItem = original; });
        Storage.prototype.removeItem = function (key) {
            if (this === localStorage && key === snapshotKey) {
                failures.count++;
                throw new DOMException("synthetic removal failure", "InvalidStateError");
            }
            return original.call(this, key);
        };
    }, snapshot.key);

    const archive = await saveAs(page, "saved-recovery-board.thinoda");
    expect(zipEntry(archive, "content.json").toString("utf8")).toContain("Before takeover");
    await expect.poll(
        () => page.evaluate(() => (Reflect.get(window, "__recoveryRemovalFailures") as { count: number }).count),
        { timeout: 10_000 },
    ).toBeGreaterThan(0);
    await expect(page.locator("#thinoda-recovery-cleanup")).toHaveText(
        "A recovery copy could not be removed. It may reappear after restart.",
    );
    const retained = await page.evaluate((key) => localStorage.getItem(key), snapshot.key);
    if (retained === null) throw new Error("Failed cleanup removed the recovery snapshot");
    expect(JSON.parse(retained).d).toBe(JSON.parse(snapshot.value).d);
    await page.evaluate(() => (Reflect.get(window, "__restoreRecoveryRemoval") as () => void)());
    await expect.poll(
        () => page.evaluate((key) => localStorage.getItem(key), snapshot.key),
        { timeout: 10_000 },
    ).toBeNull();
    await expect(page.locator("#thinoda-recovery-cleanup")).toBeEmpty();
});

test("failed cleanup remains visible and retries after the saved document closes", async ({ page }) => {
    const snapshot = await startBoard(page);
    await page.evaluate((snapshotKey) => {
        const original = Storage.prototype.removeItem;
        const failures = { count: 0 };
        Reflect.set(window, "__recoveryRemovalFailures", failures);
        Reflect.set(window, "__restoreRecoveryRemoval", () => { Storage.prototype.removeItem = original; });
        Storage.prototype.removeItem = function (key) {
            if (this === localStorage && key === snapshotKey) {
                failures.count++;
                throw new DOMException("synthetic removal failure", "InvalidStateError");
            }
            return original.call(this, key);
        };
    }, snapshot.key);

    const archive = await saveAs(page, "closed-recovery-board.thinoda");
    expect(zipEntry(archive, "content.json").toString("utf8")).toContain("Before takeover");
    await expect.poll(
        () => page.evaluate(() => (Reflect.get(window, "__recoveryRemovalFailures") as { count: number }).count),
        { timeout: 10_000 },
    ).toBeGreaterThan(0);
    await expect(page.locator("#thinoda-recovery-cleanup")).toHaveText(
        "A recovery copy could not be removed. It may reappear after restart.",
    );
    await command(page, "Close Tab");
    await page.evaluate(() => {
        Reflect.set(window, "__closeCheckPrompts", 0);
        window.prompt = () => {
            Reflect.set(window, "__closeCheckPrompts", (Reflect.get(window, "__closeCheckPrompts") as number) + 1);
            return "unexpected.thinoda";
        };
    });
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+Shift+s");
    await page.waitForTimeout(500);
    expect(await page.evaluate(() => Reflect.get(window, "__closeCheckPrompts"))).toBe(0);
    const retained = await page.evaluate((key) => localStorage.getItem(key), snapshot.key);
    if (retained === null) throw new Error("Failed cleanup removed the recovery snapshot");
    expect(JSON.parse(retained).d).toBe(JSON.parse(snapshot.value).d);
    await expect(page.locator("#thinoda-recovery-cleanup")).not.toBeEmpty();

    await page.evaluate(() => (Reflect.get(window, "__restoreRecoveryRemoval") as () => void)());
    await expect.poll(
        () => page.evaluate((key) => localStorage.getItem(key), snapshot.key),
        { timeout: 10_000 },
    ).toBeNull();
    await expect(page.locator("#thinoda-recovery-cleanup")).toBeEmpty();
});

test("a real browser quota failure preserves the previous snapshot and allows a full download", async ({ page }) => {
    const snapshot = await startBoard(page);
    await page.evaluate((snapshotKey) => {
        const original = Storage.prototype.setItem;
        const failures: string[] = [];
        Reflect.set(window, "__realRecoveryQuotaFailures", failures);
        Storage.prototype.setItem = function (key, value) {
            try {
                return original.call(this, key, value);
            } catch (error) {
                if (key === snapshotKey && error instanceof DOMException) {
                    if (failures.length === 0) {
                        Reflect.set(window, "__snapshotAtQuotaFailure", this.getItem(key));
                    }
                    failures.push(error.name);
                }
                throw error;
            }
        };
    }, snapshot.key);

    const image = await largeJpeg(page);
    await imagePicker(page, { name: "recovery-large.jpg", mimeType: "image/jpeg", buffer: image });
    await expect.poll(
        () => page.evaluate(() => (Reflect.get(window, "__realRecoveryQuotaFailures") as string[])
            .includes("QuotaExceededError")),
        { timeout: 60_000 },
    ).toBe(true);
    await expect(page.locator("#thinoda-recovery-availability")).toHaveText(
        "Crash recovery is unavailable. Save or download your work.",
    );
    const retained = await page.evaluate((key) => ({
        current: localStorage.getItem(key),
        atFailure: Reflect.get(window, "__snapshotAtQuotaFailure") as string | null,
    }), snapshot.key);
    if (retained.current === null) throw new Error("Quota failure removed the previous snapshot");
    expect(retained.current).toBe(retained.atFailure);
    expect(JSON.parse(retained.current).d).toBe(JSON.parse(snapshot.value).d);

    const archive = await saveAs(page, "large-recovery-board.thinoda");
    const content = zipEntry(archive, "content.json").toString("utf8");
    expect(content).toContain("Before takeover");
    const manifest = JSON.parse(zipEntry(archive, "manifest.json").toString("utf8")) as {
        resources: { id: string; name: string }[];
    };
    const resource = manifest.resources.find((entry) => entry.name === "recovery-large.jpg");
    if (!resource) throw new Error("Large image missing from the downloaded archive");
    expect(zipEntry(archive, `resources/${resource.id}`)).toEqual(image);
});

for (const fault of ["missing key", "changed length"] as const) {
    test(`an incomplete browser storage listing (${fault}) retains snapshots and retries`, async ({ page }) => {
        const snapshot = await startBoard(page);
        await page.evaluate((fault) => {
            const originalKey = Storage.prototype.key;
            const originalLength = Object.getOwnPropertyDescriptor(Storage.prototype, "length");
            const lengthGetter = originalLength?.get;
            if (!originalLength || !lengthGetter) throw new Error("Missing browser storage length getter");
            const failures = { count: 0 };
            Reflect.set(window, "__recoveryListingFaults", failures);
            Reflect.set(window, "__restoreRecoveryListing", () => {
                Storage.prototype.key = originalKey;
                Object.defineProperty(Storage.prototype, "length", originalLength);
            });
            if (fault === "missing key") {
                Storage.prototype.key = function (index) {
                    const key = originalKey.call(this, index);
                    if (this === localStorage && key?.includes("/recovery/")) {
                        failures.count++;
                        return null;
                    }
                    return key;
                };
            } else {
                let firstRead = true;
                Object.defineProperty(Storage.prototype, "length", {
                    ...originalLength,
                    get() {
                        const length: number = lengthGetter.call(this);
                        if (this !== localStorage) return length;
                        if (firstRead) {
                            firstRead = false;
                            return length;
                        }
                        firstRead = true;
                        failures.count++;
                        return length + 1;
                    },
                });
            }
        }, fault);

        await expect.poll(
            () => page.evaluate(() => (Reflect.get(window, "__recoveryListingFaults") as { count: number }).count),
            { timeout: 10_000 },
        ).toBeGreaterThan(0);
        const alert = page.locator("#thinoda-recovery-availability");
        await expect(alert).toHaveText("Crash recovery is unavailable. Save or download your work.");
        const retained = await page.evaluate((key) => localStorage.getItem(key), snapshot.key);
        if (retained === null) throw new Error("Incomplete listing removed the previous snapshot");
        expect(JSON.parse(retained).d).toBe(JSON.parse(snapshot.value).d);
        await page.evaluate(() => (Reflect.get(window, "__restoreRecoveryListing") as () => void)());
        await expect(alert).toBeEmpty({ timeout: 10_000 });
    });
}

test("a page restored from the back-forward cache resumes its own recovery session", async ({ page }) => {
    const snapshot = await startBoard(page);
    const lockKey = snapshot.key.replace(/\/docs\/[^/]+\.json$/, "/lock");
    const transition = await page.evaluate((key) => {
        const before = localStorage.getItem(key);
        window.dispatchEvent(new PageTransitionEvent("pagehide", { persisted: true }));
        const hidden = localStorage.getItem(key);
        window.dispatchEvent(new PageTransitionEvent("pageshow", { persisted: true }));
        return { before, hidden, resumed: localStorage.getItem(key) };
    }, lockKey);
    expect(transition.before).not.toBeNull();
    expect(transition.hidden).toBeNull();
    expect(transition.resumed).not.toBeNull();
    expect(transition.resumed).not.toBe(transition.before);
    await addSticky(page, "After page restore", 650);
    await waitForSnapshot(page, "After page restore");
});

test("a restored page cannot resume over a stale competing recovery claim", async ({ page }) => {
    const warnings = recoveryWarnings(page);
    const snapshot = await startBoard(page);
    const lockKey = snapshot.key.replace(/\/docs\/[^/]+\.json$/, "/lock");
    const competing = await page.evaluate((key) => {
        const value = localStorage.getItem(key);
        if (!value) throw new Error("Missing recovery lease");
        window.dispatchEvent(new PageTransitionEvent("pagehide", { persisted: true }));
        const entry: { m: number; l: number; d: string } = JSON.parse(value);
        const record = { owner: "competing-tab", heartbeat_ms: Date.now() - 300_000 };
        const encoded = JSON.stringify(record);
        entry.m = record.heartbeat_ms;
        entry.l = encoded.length;
        entry.d = btoa(encoded);
        const replacement = JSON.stringify(entry);
        localStorage.setItem(key, replacement);
        window.dispatchEvent(new PageTransitionEvent("pageshow", { persisted: true }));
        return replacement;
    }, lockKey);
    await addSticky(page, "After denied resume", 650);
    await expect.poll(() => warnings.length, { timeout: 10_000 }).toBe(1);
    await page.waitForTimeout(3_000);
    expect(await page.evaluate((key) => localStorage.getItem(key), lockKey)).toBe(competing);
    expect(await page.evaluate((key) => localStorage.getItem(key), snapshot.key)).toBe(snapshot.value);
});

test("another tab does not retire an empty suspended session", async ({ page, context }) => {
    const url = process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/";
    await page.goto(url);
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    const sessionKey = await page.evaluate(() => {
        const keys = Object.keys(localStorage)
            .filter((key) => /^thinoda\.vfs:.*\/recovery\/[^/]+\/session\.json$/.test(key));
        if (keys.length !== 1) throw new Error("Missing the active recovery session");
        window.dispatchEvent(new PageTransitionEvent("pagehide", { persisted: true }));
        return keys[0];
    });
    const other = await context.newPage();
    await other.goto(url);
    await expect(other.locator("#loading")).toBeHidden({ timeout: 120_000 });
    expect(await page.evaluate((key) => localStorage.getItem(key), sessionKey)).not.toBeNull();
    await page.evaluate(() => window.dispatchEvent(new PageTransitionEvent("pageshow", { persisted: true })));
    await page.bringToFront();
    await newBoard(page);
    await waitForSnapshot(page, "Before takeover");
    await other.close();
});

test("a recovery startup failure warns without blocking document downloads", async ({ page }, testInfo) => {
    const warnings = recoveryWarnings(page);
    await page.addInitScript(() => {
        const original = Storage.prototype.setItem;
        Storage.prototype.setItem = function (key, value) {
            if (key.startsWith("thinoda.vfs:") && key.includes("/recovery/")) {
                throw new DOMException("synthetic quota", "QuotaExceededError");
            }
            return original.call(this, key, value);
        };
    });
    await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    await expect.poll(() => warnings.length, { timeout: 10_000 }).toBeGreaterThan(0);
    const alert = page.locator("#thinoda-recovery-availability");
    await expect(alert).toHaveAttribute("role", "alert");
    await expect(alert).toHaveText("Crash recovery is unavailable. Save or download your work.");
    await frames(page);
    await page.screenshot({ path: testInfo.outputPath("recovery-warning-desktop.png") });
    await page.setViewportSize({ width: 390, height: 844 });
    await frames(page);
    await page.screenshot({ path: testInfo.outputPath("recovery-warning-mobile.png") });
    await page.setViewportSize({ width: 1252, height: 1278 });
    await frames(page);
    const initialWarnings = warnings.length;
    await newBoard(page);
    await page.waitForTimeout(1_500);
    expect(warnings).toHaveLength(initialWarnings);
    const archive = await saveAs(page, "no-recovery.thinoda");
    expect(zipEntry(archive, "content.json").toString("utf8")).toContain("Before takeover");
});

test("a corrupt orphan lease is preserved and reported on the next launch", async ({ page }) => {
    const messages: string[] = [];
    page.on("console", (message) => messages.push(message.text()));
    page.on("dialog", (dialog) => dialog.accept());
    const snapshot = await startBoard(page);
    const lockKey = snapshot.key.replace(/\/docs\/[^/]+\.json$/, "/lock");
    await page.evaluate((key) => localStorage.setItem(key, "corrupt lease"), lockKey);
    await page.reload();
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    await expect.poll(
        () => messages.some((message) => message.includes("Could not inspect a recovery session")),
        { timeout: 10_000 },
    ).toBe(true);
    const alert = page.locator("#thinoda-recovery-discovery");
    await expect(alert).toHaveAttribute("role", "alert");
    await expect(alert).toHaveText(
        "Some recovery data could not be checked. Keep existing storage and save your work.",
    );
    expect(await page.evaluate((key) => localStorage.getItem(key), lockKey)).toBe("corrupt lease");
    expect(await page.evaluate((key) => localStorage.getItem(key), snapshot.key)).toBe(snapshot.value);
});

test("unreadable recovery data can be kept for a later launch", async ({ page }, testInfo) => {
    page.on("dialog", (dialog) => dialog.accept());
    const snapshot = await startBoard(page);
    const corrupted = await page.evaluate((key) => {
        const value = localStorage.getItem(key);
        if (!value) throw new Error("Missing recovery snapshot");
        const entry: { l: number; d: string } = JSON.parse(value);
        const bytes = "unreadable recovery data";
        entry.l = bytes.length;
        entry.d = btoa(bytes);
        const replacement = JSON.stringify(entry);
        localStorage.setItem(key, replacement);
        return replacement;
    }, snapshot.key);

    await page.reload();
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    await expect(page.locator("#thinoda-recovery-discovery")).toHaveText(
        "Some recovery data could not be checked. Keep existing storage and save your work.",
    );
    await frames(page);
    await page.screenshot({ path: testInfo.outputPath("incomplete-recovery-desktop.png") });
    await page.setViewportSize({ width: 390, height: 844 });
    await frames(page);
    await page.screenshot({ path: testInfo.outputPath("incomplete-recovery-mobile.png") });
    // Three title controls, Restore, Download raw, Restore All, Keep recovery data.
    await recoveryTabs(page, 7);
    await page.keyboard.press("Enter");
    await frames(page);
    await page.setViewportSize({ width: 1252, height: 1278 });
    await frames(page);
    await newBoard(page);
    const archive = await saveAs(page, "after-keeping-recovery.thinoda");
    expect(zipEntry(archive, "content.json").toString("utf8")).toContain("Before takeover");
    expect(await page.evaluate((key) => localStorage.getItem(key), snapshot.key)).toBe(corrupted);
    await page.reload();
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    await expect(page.locator("#thinoda-recovery-discovery")).toHaveText(
        "Some recovery data could not be checked. Keep existing storage and save your work.",
    );
    expect(await page.evaluate((key) => localStorage.getItem(key), snapshot.key)).toBe(corrupted);
});

test("raw recovery downloads preserve each corrupt stored value and other documents", async ({ page }) => {
    const first = {
        key: `${rescueBase}a-broken-wrapper.json`,
        // Include a large value and unpaired surrogates, which JS-to-Rust strings cannot preserve.
        value: `invalid storage ${"x".repeat(1024 * 1024)}\ud800\u0000\udc00`,
    };
    const corruptSnapshot = Buffer.from([0xff, 0x00, 0x80, 0x7b, 0x0a]);
    const second = {
        key: `${rescueBase}b-broken-snapshot.json`,
        value: JSON.stringify({ m: 1, l: corruptSnapshot.length, d: corruptSnapshot.toString("base64") }),
    };
    await openRescueFixture(page, [first, second]);
    const alert = page.locator("#thinoda-recovery-download");
    await expect(alert).toHaveAttribute("role", "alert");
    await page.evaluate((protectedKeys) => {
        const original = URL.createObjectURL;
        URL.createObjectURL = function (blob) {
            Reflect.set(window, "__rescueMediaType", blob.type);
            return original.call(this, blob);
        };
        const set = Storage.prototype.setItem;
        const remove = Storage.prototype.removeItem;
        const mutations: string[] = [];
        Reflect.set(window, "__rescueMutations", mutations);
        Storage.prototype.setItem = function (key, value) {
            if (this === localStorage && protectedKeys.includes(key)) mutations.push(`set:${key}`);
            return set.call(this, key, value);
        };
        Storage.prototype.removeItem = function (key) {
            if (this === localStorage && protectedKeys.includes(key)) mutations.push(`remove:${key}`);
            return remove.call(this, key);
        };
    }, [first.key, second.key, sourceKey]);

    // Navigate the modal with the keyboard: title controls, first Restore, first raw download.
    await recoveryTabs(page, 5);
    const firstRequest = page.waitForEvent("download", { timeout: 30_000 });
    await page.keyboard.press("Enter");
    const firstDownload = await firstRequest;
    expect(firstDownload.suggestedFilename()).toBe("thinoda-recovery-record.utf16le");
    expect(await firstDownload.failure()).toBeNull();
    expect(await readFile(await firstDownload.path())).toEqual(Buffer.from(first.value, "utf16le"));
    await expect(alert).toHaveText(rescueRequested);
    await expect(alert).toHaveAttribute("lang", "en");
    expect(await page.evaluate(() => Reflect.get(window, "__rescueMediaType"))).toBe("application/octet-stream");

    // The next two controls are the second Restore and the second raw download.
    await recoveryTabs(page, 2);
    const secondRequest = page.waitForEvent("download", { timeout: 30_000 });
    await page.keyboard.press("Enter");
    const secondDownload = await secondRequest;
    expect(secondDownload.suggestedFilename()).toBe("thinoda-recovery-record.utf16le");
    expect(await secondDownload.failure()).toBeNull();
    expect(await readFile(await secondDownload.path())).toEqual(Buffer.from(second.value, "utf16le"));

    for (const { key, value } of [first, second]) {
        expect(await page.evaluate((key) => localStorage.getItem(key), key)).toBe(value);
    }
    expect(await page.evaluate((key) => localStorage.getItem(key), sourceKey)).toBe(sourceValue);
    expect(await page.evaluate(() => Reflect.get(window, "__rescueMutations"))).toEqual([]);

    // Keep both originals and release the orphan lease; they remain offerable after reload.
    await recoveryTabs(page, 2);
    await page.keyboard.press("Enter");
    await expect(alert).toBeEmpty();
    await page.reload();
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    await expect(page.locator("#thinoda-recovery-discovery")).toHaveText(
        "Some recovery data could not be checked. Keep existing storage and save your work.",
    );
    for (const { key, value } of [first, second]) {
        expect(await page.evaluate((key) => localStorage.getItem(key), key)).toBe(value);
    }
});

test("raw recovery preserves every JavaScript UTF-16 code unit without substitution", async ({ page }) => {
    const units = Array.from({ length: 0x10000 }, (_, unit) => unit);
    // Include isolated and repeated surrogates, a valid pair, NUL, and a trailing lone surrogate.
    units.push(0xd800, 0x0000, 0xdc00, 0xd800, 0xd800, 0xdc00, 0xdc00, 0xfeff, 0xd800);
    const value = Array.from({ length: Math.ceil(units.length / 1024) }, (_, index) =>
        String.fromCharCode(...units.slice(index * 1024, (index + 1) * 1024))).join("");
    const key = `${rescueBase}all-code-units.json`;
    const emptyKey = `${rescueBase}empty.json`;
    await openRescueFixture(page, [{ key, value }, { key: emptyKey, value: "" }]);

    const storedUnits = () => page.evaluate((key) => {
        const stored = localStorage.getItem(key);
        if (stored === null) throw new Error("Missing recovery record");
        return Array.from({ length: stored.length }, (_, index) => stored.charCodeAt(index));
    }, key);
    expect(await storedUnits()).toEqual(units);
    await page.evaluate(({ key, emptyKey }) => {
        const original = localStorage.getItem(key);
        const set = Storage.prototype.setItem;
        const remove = Storage.prototype.removeItem;
        const clear = Storage.prototype.clear;
        const mutations: string[] = [];
        Reflect.set(window, "__utf16RescueOriginal", original);
        Reflect.set(window, "__utf16RescueMutations", mutations);
        Storage.prototype.setItem = function (name, value) {
            if (this === localStorage && (name === key || name === emptyKey)) mutations.push("set");
            return set.call(this, name, value);
        };
        Storage.prototype.removeItem = function (name) {
            if (this === localStorage && (name === key || name === emptyKey)) mutations.push("remove");
            return remove.call(this, name);
        };
        Storage.prototype.clear = function () {
            if (this === localStorage) mutations.push("clear");
            return clear.call(this);
        };
    }, { key, emptyKey });

    await recoveryTabs(page, 5);
    const request = page.waitForEvent("download", { timeout: 30_000 });
    await page.keyboard.press("Enter");
    const download = await request;
    expect(download.suggestedFilename()).toBe("thinoda-recovery-record.utf16le");
    expect(await download.failure()).toBeNull();
    const actual = await readFile(await download.path());
    const expected = Buffer.alloc(units.length * 2);
    units.forEach((unit, index) => expected.writeUInt16LE(unit, index * 2));
    expect(actual.length).toBe(expected.length);
    expect(actual.findIndex((byte, index) => byte !== expected[index]), "first mismatched byte").toBe(-1);
    await recoveryTabs(page, 2);
    const emptyRequest = page.waitForEvent("download", { timeout: 30_000 });
    await page.keyboard.press("Enter");
    const emptyDownload = await emptyRequest;
    expect(await emptyDownload.failure()).toBeNull();
    expect(await readFile(await emptyDownload.path())).toHaveLength(0);
    expect(await storedUnits()).toEqual(units);
    expect(await page.evaluate((key) => localStorage.getItem(key), emptyKey)).toBe("");
    expect(await page.evaluate((key) => ({
        unchanged: localStorage.getItem(key) === Reflect.get(window, "__utf16RescueOriginal"),
        mutations: Reflect.get(window, "__utf16RescueMutations"),
    }), key)).toEqual({ unchanged: true, mutations: [] });
    expect(await page.evaluate((key) => localStorage.getItem(key), sourceKey)).toBe(sourceValue);
});

test("raw recovery refuses ambiguous storage keys without changing either record", async ({ page }) => {
    const records = [
        { key: `${rescueBase}bad-\ud800.json`, value: "unpaired surrogate key" },
        { key: `${rescueBase}bad-\ufffd.json`, value: "replacement character key" },
    ];
    await openRescueFixture(page, records);
    const downloads: string[] = [];
    page.on("download", (download) => downloads.push(download.suggestedFilename()));
    await recoveryTabs(page, 5);
    await page.keyboard.press("Enter");
    await expect(page.locator("#thinoda-recovery-download")).toHaveText(rescueError);
    expect(downloads).toEqual([]);
    for (const { key, value } of records) {
        expect(await page.evaluate((key) => localStorage.getItem(key), key)).toBe(value);
    }
    expect(await page.evaluate((key) => localStorage.getItem(key), sourceKey)).toBe(sourceValue);
});

test("raw recovery failures report an error, keep originals, and allow retry", async ({ page }) => {
    const record = { key: `${rescueBase}broken.json`, value: '\ud800\u0000{"m":1,"l":4,"d":"%%%"}\udc00\ud800' };
    await openRescueFixture(page, [record]);
    const alert = page.locator("#thinoda-recovery-download");
    const downloads: string[] = [];
    page.on("download", (download) => downloads.push(download.suggestedFilename()));
    await page.evaluate((protectedKeys) => {
        const set = Storage.prototype.setItem;
        const remove = Storage.prototype.removeItem;
        const clear = Storage.prototype.clear;
        const mutations: string[] = [];
        Reflect.set(window, "__rescueMutations", mutations);
        Storage.prototype.setItem = function (key, value) {
            if (this === localStorage && protectedKeys.includes(key)) mutations.push(`set:${key}`);
            return set.call(this, key, value);
        };
        Storage.prototype.removeItem = function (key) {
            if (this === localStorage && protectedKeys.includes(key)) mutations.push(`remove:${key}`);
            return remove.call(this, key);
        };
        Storage.prototype.clear = function () {
            if (this === localStorage) mutations.push("clear");
            return clear.call(this);
        };
    }, [record.key, sourceKey]);
    await recoveryTabs(page, 5);

    async function expectFailure() {
        await page.evaluate(() => {
            const element = document.getElementById("thinoda-recovery-download");
            if (element) element.textContent = "";
        });
        await page.keyboard.press("Enter");
        await expect(alert).toHaveText(rescueError);
        expect(downloads).toEqual([]);
    }

    await page.evaluate((key) => {
        const original = Storage.prototype.getItem;
        Reflect.set(window, "__restoreRescueRead", () => { Storage.prototype.getItem = original; });
        Storage.prototype.getItem = function (name) {
            if (this === localStorage && name === key) throw new DOMException("read denied", "SecurityError");
            return original.call(this, name);
        };
    }, record.key);
    await expectFailure();
    await page.evaluate(() => (Reflect.get(window, "__restoreRescueRead") as () => void)());
    expect(await page.evaluate((key) => localStorage.getItem(key), record.key)).toBe(record.value);

    await page.evaluate(() => {
        const original = window.Blob;
        Reflect.set(window, "__restoreRescueBlob", () => { window.Blob = original; });
        window.Blob = new Proxy(original, {
            construct() { throw new DOMException("allocation denied", "QuotaExceededError"); },
        });
    });
    await expectFailure();
    await page.evaluate(() => (Reflect.get(window, "__restoreRescueBlob") as () => void)());
    expect(await page.evaluate((key) => localStorage.getItem(key), record.key)).toBe(record.value);

    await page.evaluate(() => {
        const original = URL.createObjectURL;
        Reflect.set(window, "__restoreRescueUrl", () => { URL.createObjectURL = original; });
        URL.createObjectURL = () => { throw new DOMException("download denied", "SecurityError"); };
    });
    await expectFailure();
    await page.evaluate(() => (Reflect.get(window, "__restoreRescueUrl") as () => void)());
    expect(await page.evaluate((key) => localStorage.getItem(key), record.key)).toBe(record.value);

    await page.evaluate(() => {
        const original = Element.prototype.setAttribute;
        Reflect.set(window, "__restoreRescueAttribute", () => { Element.prototype.setAttribute = original; });
        Element.prototype.setAttribute = function (name, value) {
            if (this instanceof HTMLAnchorElement && name === "download"
                && value === "thinoda-recovery-record.utf16le") {
                throw new DOMException("link denied", "SecurityError");
            }
            return original.call(this, name, value);
        };
    });
    await expectFailure();
    await page.evaluate(() => (Reflect.get(window, "__restoreRescueAttribute") as () => void)());
    expect(await page.evaluate((key) => localStorage.getItem(key), record.key)).toBe(record.value);

    await page.evaluate(() => {
        const original = HTMLElement.prototype.click;
        Reflect.set(window, "__restoreRescueClick", () => { HTMLElement.prototype.click = original; });
        HTMLElement.prototype.click = function () {
            if (this instanceof HTMLAnchorElement && this.download === "thinoda-recovery-record.utf16le") {
                throw new DOMException("click denied", "SecurityError");
            }
            return original.call(this);
        };
    });
    await expectFailure();
    await page.evaluate(() => (Reflect.get(window, "__restoreRescueClick") as () => void)());
    expect(await page.evaluate((key) => localStorage.getItem(key), record.key)).toBe(record.value);

    const lockKey = record.key.replace(/\/docs\/[^/]+\.json$/, "/lock");
    const lease = await page.evaluate((key) => {
        const original = localStorage.getItem(key);
        if (!original) throw new Error("Missing orphan lease");
        const entry: { m: number; l: number; d: string } = JSON.parse(original);
        const lock = JSON.parse(atob(entry.d)) as { owner: string; heartbeat_ms: number };
        lock.owner = "other-tab";
        const bytes = JSON.stringify(lock);
        entry.l = bytes.length;
        entry.d = btoa(bytes);
        const competing = JSON.stringify(entry);
        localStorage.setItem(key, competing);
        return { original, competing };
    }, lockKey);
    await expectFailure();
    expect(await page.evaluate((key) => localStorage.getItem(key), lockKey)).toBe(lease.competing);
    expect(await page.evaluate((key) => localStorage.getItem(key), record.key)).toBe(record.value);
    expect(await page.evaluate((key) => localStorage.getItem(key), sourceKey)).toBe(sourceValue);
    await page.evaluate(({ key, value }) => localStorage.setItem(key, value), {
        key: lockKey, value: lease.original,
    });

    const retry = page.waitForEvent("download", { timeout: 30_000 });
    await page.keyboard.press("Enter");
    const download = await retry;
    expect(await download.failure()).toBeNull();
    expect(await readFile(await download.path())).toEqual(Buffer.from(record.value, "utf16le"));
    await expect(alert).toHaveText(rescueRequested);
    expect(await page.evaluate((key) => localStorage.getItem(key), record.key)).toBe(record.value);
    expect(await page.evaluate((key) => localStorage.getItem(key), sourceKey)).toBe(sourceValue);
    expect(await page.evaluate(() => Reflect.get(window, "__rescueMutations"))).toEqual([]);
});

test("a recovery load that fails after discovery offers raw download without discarding", async ({ page }) => {
    page.on("dialog", (dialog) => dialog.accept());
    const snapshot = await startBoard(page);
    await page.reload();
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    await expect(page.locator("#thinoda-recovery-discovery")).toBeEmpty();
    await frames(page);

    const corrupt = "changed after discovery";
    await page.evaluate(({ key, value }) => localStorage.setItem(key, value), {
        key: snapshot.key, value: corrupt,
    });
    await recoveryTabs(page, 4);
    await page.keyboard.press("Enter");
    await expect(page.locator("#thinoda-recovery-discovery")).toHaveText(
        "Some recovery data could not be checked. Keep existing storage and save your work.",
    );
    expect(await page.evaluate((key) => localStorage.getItem(key), snapshot.key)).toBe(corrupt);

    // The failed restore changes the dialog contents, so its focus order restarts.
    await recoveryTabs(page, 5);
    const request = page.waitForEvent("download", { timeout: 30_000 });
    await page.keyboard.press("Enter");
    const download = await request;
    expect(await download.failure()).toBeNull();
    expect(await readFile(await download.path())).toEqual(Buffer.from(corrupt, "utf16le"));
    expect(await page.evaluate((key) => localStorage.getItem(key), snapshot.key)).toBe(corrupt);
});

test.describe("raw recovery on a narrow German viewport", () => {
    test.use({ locale: "de-DE", viewport: { width: 390, height: 844 } });

    test("keeps the modal keyboard-operable and announces failures in German", async ({ page }) => {
        const record = { key: `${rescueBase}broken.json`, value: "invalid storage" };
        await openRescueFixture(page, [record]);
        await expect(page.locator("html")).toHaveAttribute("lang", "de");
        await page.evaluate(() => {
            const original = URL.createObjectURL;
            Reflect.set(window, "__restoreRescueUrl", () => { URL.createObjectURL = original; });
            URL.createObjectURL = () => { throw new DOMException("download denied", "SecurityError"); };
        });
        await recoveryTabs(page, 5);
        await page.keyboard.press("Enter");
        const alert = page.locator("#thinoda-recovery-download");
        await expect(alert).toHaveAttribute("lang", "de");
        await expect(alert).toHaveText(
            "Der Rohdaten-Download ist fehlgeschlagen. Bewahren Sie die Wiederherstellungsdaten auf, prüfen Sie den Browserspeicher und Downloads, schließen Sie andere Tabs und versuchen Sie es erneut.",
        );
        expect(await page.evaluate((key) => localStorage.getItem(key), record.key)).toBe(record.value);
        await page.evaluate(() => (Reflect.get(window, "__restoreRescueUrl") as () => void)());

        const request = page.waitForEvent("download", { timeout: 30_000 });
        await page.keyboard.press("Enter");
        const download = await request;
        expect(await download.failure()).toBeNull();
        expect(await readFile(await download.path())).toEqual(Buffer.from(record.value, "utf16le"));
        await expect(alert).toHaveText(
            "Download angefordert. Prüfen Sie die Browser-Downloads; die ursprünglichen Wiederherstellungsdaten bleiben erhalten.",
        );
        expect(await page.evaluate((key) => localStorage.getItem(key), sourceKey)).toBe(sourceValue);
    });
});
