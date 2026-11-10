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
    await page.addInitScript(() => {
        document.addEventListener("DOMContentLoaded", () => {
            const loading = document.getElementById("loading");
            if (!loading) return;
            const observer = new MutationObserver(() => {
                if (!loading.isConnected) {
                    sessionStorage.setItem("thinoda-test-loader-release", JSON.stringify({
                        renderer: loading.getAttribute("data-renderer-ready"),
                        application: loading.getAttribute("data-application-ready"),
                    }));
                    observer.disconnect();
                }
            });
            observer.observe(document.documentElement, { childList: true, subtree: true });
        });
    });
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
        await expect(loader).toHaveAttribute("lang", "en");
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
    expect(await page.evaluate(() => JSON.parse(
        sessionStorage.getItem("thinoda-test-loader-release") ?? "null",
    ))).toEqual({ renderer: "true", application: "true" });
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

for (const scenario of [
    { name: "quota", error: "QuotaExceededError", message: "storage quota was exceeded" },
    { name: "permission", error: "SecurityError", message: "browser denied access to storage" },
] as const) {
    test(`a ${scenario.name} storage failure gives a private, actionable diagnosis`, async ({ page }) => {
        const key = "thinoda.vfs:/Documents/private-document.thinoda";
        const value = '{"m":1,"l":3,"d":"YWJj"}';
        await page.addInitScript(({ key, value, error }) => {
            localStorage.setItem(key, value);
            Object.defineProperty(window, "indexedDB", {
                configurable: true,
                value: {
                    open() { throw new DOMException("private document contents", error); },
                },
            });
        }, { key, value, error: scenario.error });
        await page.goto(url, { waitUntil: "commit", timeout: 120_000 });
        const alert = page.locator("#loading");
        await expect(alert).toHaveAttribute("role", "alert", { timeout: 120_000 });
        await expect(alert).toContainText(scenario.message);
        await expect(alert).not.toContainText("private");
        await expect(alert).toContainText("did not delete legacy data");
        expect(await page.evaluate(key => localStorage.getItem(key), key)).toBe(value);
    });
}

test("a blocked database upgrade identifies the other tab and can be retried", async ({ page }) => {
    let release!: () => void;
    let requested!: () => void;
    const key = "thinoda.vfs:/Documents/blocked-upgrade.thinoda";
    const value = '{"m":1,"l":3,"d":"YWJj"}';
    await page.addInitScript(({ key, value }) => {
        if (!sessionStorage.getItem("thinoda-blocked-upgrade-seeded")) {
            localStorage.setItem(key, value);
            sessionStorage.setItem("thinoda-blocked-upgrade-seeded", "yes");
        }
    }, { key, value });
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
        await page.evaluate(() => new Promise<void>((resolve, reject) => {
            const request = indexedDB.open("thinoda.documents.v1", 1);
            request.onupgradeneeded = () => {
                request.result.createObjectStore("documents", { keyPath: "id" });
            };
            request.onerror = () => reject(request.error);
            request.onsuccess = () => {
                Reflect.set(window, "__thinoda_test_open_database", request.result);
                resolve();
            };
        }));
    } finally {
        release();
    }

    const alert = page.locator("#loading");
    await expect(alert).toHaveAttribute("role", "alert", { timeout: 120_000 });
    await expect(alert).toContainText("another browser tab");
    await expect(alert).toContainText("close other Thinoda tabs");
    expect(await page.evaluate(key => localStorage.getItem(key), key)).toBe(value);
    await page.evaluate(() => {
        const database = Reflect.get(window, "__thinoda_test_open_database") as IDBDatabase;
        database.close();
    });
    await page.unroute(/\.wasm(?:[?#].*)?$/);
    await Promise.all([
        page.waitForNavigation({ waitUntil: "commit", timeout: 120_000 }),
        alert.getByRole("link", { name: "Reload Thinoda" }).click(),
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
    await page.addInitScript(() => {
        if (sessionStorage.getItem("thinoda-module-retry-seeded") !== "yes") {
            localStorage.setItem("thinoda-module-retry-preserved", "yes");
            sessionStorage.setItem("thinoda-module-retry-seeded", "yes");
        }
    });
    await page.route(module, route => route.fulfill({
        status: 503,
        contentType: "text/javascript",
        headers: { "Cache-Control": "no-store" },
        body: "unavailable",
    }));
    const original = new URL(url);
    original.searchParams.set("keep", "value");
    original.hash = "section";
    await page.goto(original.href, { waitUntil: "commit", timeout: 120_000 });
    const loader = page.locator("#loading");
    await expect(loader).toHaveAttribute("role", "alert", { timeout: 120_000 });
    await expect(loader).toContainText("Thinoda could not load");
    const reload = loader.getByRole("link", { name: "Reload Thinoda" });
    const retryUrl = new URL((await reload.getAttribute("href")) ?? "");
    expect(retryUrl.searchParams.get("__thinoda_retry")).toBeTruthy();
    expect(retryUrl.searchParams.get("keep")).toBe("value");
    expect(retryUrl.hash).toBe("#section");
    await page.unroute(module);
    await Promise.all([
        page.waitForNavigation({ waitUntil: "commit", timeout: 120_000 }),
        reload.click(),
    ]);
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    expect(new URL(page.url()).searchParams.has("__thinoda_retry")).toBe(false);
    expect(new URL(page.url()).searchParams.get("keep")).toBe("value");
    expect(new URL(page.url()).hash).toBe("#section");
    expect(await page.evaluate(() => localStorage.getItem("thinoda-module-retry-preserved"))).toBe("yes");
    expect(await page.evaluate(() => sessionStorage.getItem("thinoda-bootstrap-retry"))).toBeNull();
    await openCommandPalette(page);
});

test("browser reload retries a failed app module without clearing data", async ({ page }) => {
    const module = /\/thinoda-web(?:-[a-f0-9]+)?\.js(?:[?#].*)?$/;
    await page.addInitScript(() => {
        if (sessionStorage.getItem("thinoda-reload-seeded") !== "yes") {
            localStorage.setItem("thinoda-reload-preserved", "yes");
            sessionStorage.setItem("thinoda-reload-seeded", "yes");
        }
    });
    await page.route(module, route => route.fulfill({
        status: 503,
        contentType: "text/javascript",
        headers: { "Cache-Control": "no-store" },
        body: "unavailable",
    }));
    await page.goto(url, { waitUntil: "commit", timeout: 120_000 });
    await expect(page.locator("#loading")).toHaveAttribute("role", "alert", { timeout: 120_000 });
    expect(await page.evaluate(() => sessionStorage.getItem("thinoda-bootstrap-retry"))).toBeTruthy();
    await page.unroute(module);
    await page.reload({ waitUntil: "commit", timeout: 120_000 });
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    expect(await page.evaluate(() => sessionStorage.getItem("thinoda-bootstrap-retry"))).toBeNull();
    expect(await page.evaluate(() => localStorage.getItem("thinoda-reload-preserved"))).toBe("yes");
    await openCommandPalette(page);
});

test("the initial app module must pass integrity verification", async ({ page }) => {
    const module = /\/thinoda-web(?:-[a-f0-9]+)?\.js$/;
    let intercepted = 0;
    await page.route(module, route => {
        intercepted++;
        return route.fulfill({
            status: 200,
            contentType: "text/javascript",
            headers: { "Cache-Control": "no-store" },
            body: "window.__thinoda_tampered_module = true; throw new Error('tampered');",
        });
    });

    await page.goto(url, { waitUntil: "commit", timeout: 120_000 });
    const loader = page.locator("#loading");
    await expect(loader).toHaveAttribute("role", "alert", { timeout: 120_000 });
    expect(intercepted).toBeGreaterThan(0);
    expect(await page.evaluate(() => Reflect.get(window, "__thinoda_tampered_module"))).toBeUndefined();

    await page.unroute(module);
    const reload = loader.getByRole("link", { name: "Reload Thinoda" });
    await Promise.all([
        page.waitForNavigation({ waitUntil: "commit", timeout: 120_000 }),
        reload.click(),
    ]);
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    await openCommandPalette(page);
});

test("a retried app module must still pass integrity verification", async ({ page }) => {
    const module = /\/thinoda-web(?:-[a-f0-9]+)?\.js$/;
    await page.route(module, route => route.fulfill({
        status: 503,
        contentType: "text/javascript",
        headers: { "Cache-Control": "no-store" },
        body: "unavailable",
    }));
    await page.goto(url, { waitUntil: "commit", timeout: 120_000 });
    await expect(page.locator("#loading")).toHaveAttribute("role", "alert", { timeout: 120_000 });
    await page.unroute(module);

    const retriedModule = (requestUrl: URL) =>
        module.test(requestUrl.pathname) && requestUrl.searchParams.has("__thinoda_retry");
    let intercepted = 0;
    await page.route(retriedModule, route => {
        intercepted++;
        return route.fulfill({
            status: 200,
            contentType: "text/javascript",
            headers: { "Cache-Control": "no-store" },
            body: "window.__thinoda_tampered_module = true; export default async function () {}",
        });
    });
    const reload = page.getByRole("link", { name: "Reload Thinoda" });
    const firstRetry = new URL((await reload.getAttribute("href")) ?? "")
        .searchParams.get("__thinoda_retry");
    await Promise.all([
        page.waitForNavigation({ waitUntil: "commit", timeout: 120_000 }),
        reload.click(),
    ]);
    await expect(page.locator("#loading")).toHaveAttribute("role", "alert", { timeout: 120_000 });
    expect(intercepted).toBeGreaterThan(0);
    expect(await page.evaluate(() => Reflect.get(window, "__thinoda_tampered_module"))).toBeUndefined();
    const secondRetry = new URL((await reload.getAttribute("href")) ?? "")
        .searchParams.get("__thinoda_retry");
    expect(secondRetry).toBeTruthy();
    expect(secondRetry).not.toBe(firstRetry);

    await page.unroute(retriedModule);
    await Promise.all([
        page.waitForNavigation({ waitUntil: "commit", timeout: 120_000 }),
        reload.click(),
    ]);
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    await openCommandPalette(page);
});
