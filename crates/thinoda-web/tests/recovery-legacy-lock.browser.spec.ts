import { expect, test, type Page } from "@playwright/test";
import { command, frames, imagePicker, jpeg, saveAs, zipEntry } from "./board-browser-helpers";

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

function recoveryWarnings(page: Page) {
    const warnings: string[] = [];
    page.on("console", (message) => {
        if (message.text().includes("crash recovery unavailable:")) warnings.push(message.text());
    });
    return warnings;
}

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
    const retained = await page.evaluate((key) => localStorage.getItem(key), snapshot.key);
    if (retained === null) throw new Error("Failed cleanup removed the recovery snapshot");
    expect(JSON.parse(retained).d).toBe(JSON.parse(snapshot.value).d);
    await page.evaluate(() => (Reflect.get(window, "__restoreRecoveryRemoval") as () => void)());
    await expect.poll(
        () => page.evaluate((key) => localStorage.getItem(key), snapshot.key),
        { timeout: 10_000 },
    ).toBeNull();
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

    let image = await jpeg(page, 1950, 0.98);
    for (const quality of [0.95, 0.9, 0.85]) {
        if (image.length <= 8 * 1024 * 1024) break;
        image = await jpeg(page, 1950, quality);
    }
    expect(image.length).toBeGreaterThan(4 * 1024 * 1024);
    expect(image.length).toBeLessThanOrEqual(8 * 1024 * 1024);
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
    await page.mouse.click(170, 546);
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
