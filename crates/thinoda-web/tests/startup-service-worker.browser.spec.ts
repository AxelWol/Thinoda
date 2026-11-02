/**
 * Run against a freshly built production shell over localhost or HTTPS.
 * Each test gets an isolated browser context with service workers enabled.
 */
import { expect, test } from "@playwright/test";

test.use({
    viewport: { width: 1024, height: 768 },
    deviceScaleFactor: 1,
    serviceWorkers: "allow",
    screenshot: "only-on-failure",
});
test.setTimeout(180_000);

const url = process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/";

test("offline shell installation starts after the initial Wasm load", async ({ page, context }) => {
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
        await page.goto(url, { waitUntil: "commit", timeout: 120_000 });
        await wasmRequested;
        await page.waitForLoadState("load", { timeout: 30_000 });
        await expect(page.locator("#loading")).toBeVisible();
        await page.waitForTimeout(250);
        expect(await page.evaluate(async () => !!(await navigator.serviceWorker.getRegistration())))
            .toBe(false);
    } finally {
        release();
    }

    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    await page.evaluate(async () => { await navigator.serviceWorker.ready; });
    await context.setOffline(true);
    await page.reload({ waitUntil: "commit", timeout: 120_000 });
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    expect(await page.evaluate(() => !!navigator.serviceWorker.controller)).toBe(true);
    await page.locator("#thinoda_canvas").focus();
    await page.evaluate(() => new Promise<void>(resolve => {
        requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
    }));
    await page.keyboard.press("Control+Shift+p");
    await expect(page.locator("input[type='text']")).toBeFocused();
});

for (const scenario of ["wasm", "module"] as const) {
    test(`a failed ${scenario} bootstrap still allows offline shell installation`, async ({ page }) => {
        const resource = scenario === "wasm"
            ? /\.wasm(?:[?#].*)?$/
            : /\/thinoda-web(?:-[a-f0-9]+)?\.js(?:[?#].*)?$/;
        await page.route(resource, route => route.fulfill({
            status: 503,
            contentType: scenario === "wasm" ? "application/wasm" : "text/javascript",
            headers: { "Cache-Control": "no-store" },
            body: "unavailable",
        }));
        await page.addInitScript(() => localStorage.setItem("startup-preserved", "yes"));
        await page.goto(url, { waitUntil: "commit", timeout: 120_000 });
        await expect(page.locator("#loading")).toHaveAttribute("role", "alert", { timeout: 120_000 });
        await expect(page.locator("#loading")).toContainText("Thinoda could not load");
        await page.evaluate(async () => { await navigator.serviceWorker.ready; });
        expect(await page.evaluate(() => localStorage.getItem("startup-preserved"))).toBe("yes");
    });
}
