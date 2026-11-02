/**
 * Browser regression for the production startup shell.
 *
 * Serve a freshly built bundle and set THINODA_WEB_URL before running this
 * file with Playwright's bundled test runner. Each test uses an isolated
 * browser context.
 */
import { expect, test, type Page } from "@playwright/test";

test.use({
    viewport: { width: 1024, height: 768 },
    deviceScaleFactor: 1,
    serviceWorkers: "block",
    screenshot: "only-on-failure",
});
test.setTimeout(180_000);

const url = process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/";

async function openCommandPalette(page: Page) {
    const canvas = page.locator("#thinoda_canvas");
    const input = page.locator("input[type='text']");
    await canvas.focus();
    await page.evaluate(() => new Promise<void>(resolve => {
        requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
    }));
    await page.keyboard.press("Control+Shift+p");
    // eframe keeps a transparent IME input in the DOM even before the
    // palette opens, so its existence or visibility is not an interaction.
    await expect(input).toBeFocused({ timeout: 30_000 });
}

test("the pending loader is accessible and honors reduced motion", async ({ page }) => {
    let release!: () => void;
    let requested!: () => void;
    const gate = new Promise<void>(resolve => { release = resolve; });
    const wasmRequested = new Promise<void>(resolve => { requested = resolve; });
    await page.route(/\.wasm(?:[?#].*)?$/, async route => {
        requested();
        await gate;
        await route.continue();
    });

    try {
        await page.emulateMedia({ reducedMotion: "no-preference" });
        await page.goto(url, { waitUntil: "commit", timeout: 120_000 });
        const loader = page.locator("#loading");
        await expect(loader).toBeVisible();
        await expect(loader).toHaveAttribute("role", "status");
        await expect(loader).toHaveAttribute("aria-live", "polite");
        await expect(loader.locator("#loading-message")).toHaveText("Loading Thinoda…");
        await wasmRequested;
        const image = loader.locator("img");
        expect(await image.evaluate(element => getComputedStyle(element).animationName))
            .toBe("loading-pulse");
        await page.emulateMedia({ reducedMotion: "reduce" });
        expect(await image.evaluate(element => getComputedStyle(element).animationName))
            .toBe("none");
    } finally {
        release();
    }

    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    await openCommandPalette(page);
});

test("storage failure is announced and reload preserves legacy data", async ({ page }) => {
    const key = "thinoda.vfs:/Documents/startup-test.thinoda";
    const value = '{"m":1,"l":3,"d":"YWJj"}';
    await page.addInitScript(({ key, value }) => {
        if (sessionStorage.getItem("thinoda-startup-test-seeded") !== "yes") {
            localStorage.setItem(key, value);
            sessionStorage.setItem("thinoda-startup-test-seeded", "yes");
        }
        if (sessionStorage.getItem("thinoda-startup-test-retry") !== "yes") {
            Object.defineProperty(window, "indexedDB", { configurable: true, value: undefined });
        }
    }, { key, value });

    await page.goto(url, { waitUntil: "commit", timeout: 120_000 });
    const loader = page.locator("#loading");
    await expect(loader).toHaveAttribute("role", "alert", { timeout: 120_000 });
    await expect(loader).toHaveAttribute("aria-live", "assertive");
    await expect(loader).not.toHaveAttribute("aria-busy", "true");
    await expect(loader).toContainText("could not verify browser document storage");
    const reload = loader.getByRole("link", { name: "Reload Thinoda" });
    await expect(reload).toBeVisible();
    expect(await page.evaluate(key => localStorage.getItem(key), key)).toBe(value);

    await page.evaluate(() => sessionStorage.setItem("thinoda-startup-test-retry", "yes"));
    await Promise.all([
        page.waitForNavigation({ waitUntil: "commit", timeout: 120_000 }),
        reload.click(),
    ]);
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    expect(await page.evaluate(key => localStorage.getItem(key), key)).toBe(value);
    await openCommandPalette(page);
});

for (const scenario of ["unavailable", "invalid"] as const) {
    test(`an ${scenario} Wasm response announces a recoverable failure`, async ({ page }) => {
        const wasm = /\.wasm(?:[?#].*)?$/;
        await page.route(wasm, route => route.fulfill({
            status: scenario === "unavailable" ? 503 : 200,
            contentType: "application/wasm",
            headers: { "Cache-Control": "no-store" },
            body: "invalid Wasm",
        }));
        await page.addInitScript(() => {
            window.addEventListener("TrunkApplicationStarted", () => {
                sessionStorage.setItem("thinoda-startup-signaled", "yes");
            });
            if (sessionStorage.getItem("thinoda-startup-preserve-seeded") !== "yes") {
                localStorage.setItem("thinoda-startup-preserved", "yes");
                sessionStorage.setItem("thinoda-startup-preserve-seeded", "yes");
            }
        });
        await page.goto(url, { waitUntil: "commit", timeout: 120_000 });

        const loader = page.locator("#loading");
        await expect(loader).toHaveAttribute("role", "alert", { timeout: 120_000 });
        await expect(loader).toHaveAttribute("aria-live", "assertive");
        await expect(loader).toContainText("Thinoda could not load");
        const reload = loader.getByRole("link", { name: "Reload Thinoda" });
        await expect(reload).toBeVisible();
        expect(await page.evaluate(() => localStorage.getItem("thinoda-startup-preserved")))
            .toBe("yes");
        expect(await page.evaluate(() => sessionStorage.getItem("thinoda-startup-signaled")))
            .toBeNull();
        await page.unroute(wasm);
        await reload.focus();
        await expect(reload).toBeFocused();
        await Promise.all([
            page.waitForNavigation({ waitUntil: "commit", timeout: 120_000 }),
            reload.press("Enter"),
        ]);
        await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
        expect(await page.evaluate(() => localStorage.getItem("thinoda-startup-preserved")))
            .toBe("yes");
        expect(await page.evaluate(() => sessionStorage.getItem("thinoda-startup-signaled")))
            .toBe("yes");
        await openCommandPalette(page);
    });
}

test("an unavailable app module exposes a reload action", async ({ page }) => {
    const module = /\/thinoda-web(?:-[a-f0-9]+)?\.js(?:[?#].*)?$/;
    await page.route(module, route => route.fulfill({
        status: 503,
        contentType: "text/javascript",
        headers: { "Cache-Control": "no-store" },
        body: "unavailable",
    }));
    await page.goto(url, { waitUntil: "commit", timeout: 120_000 });
    const loader = page.locator("#loading");
    await expect(loader).toHaveAttribute("role", "alert", { timeout: 120_000 });
    await expect(loader).toContainText("Thinoda could not load");
    const reload = loader.getByRole("link", { name: "Reload Thinoda" });
    await page.unroute(module);
    await Promise.all([
        page.waitForNavigation({ waitUntil: "commit", timeout: 120_000 }),
        reload.click(),
    ]);
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    await openCommandPalette(page);
});
