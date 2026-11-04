"use strict";

const http = require("node:http");
const { readFile } = require("node:fs/promises");
const path = require("node:path");
const { createRequire } = require("node:module");

const root = path.resolve(__dirname, "..", "..", "..", "..");
const webRequire = createRequire(path.join(root, "crates", "thinoda-web", "package.json"));
const browsers = webRequire("@playwright/test");
const assets = new Map([
    ["/thinoda-web/tests/recovery-db.browser.html", path.join(__dirname, "crates", "thinoda-web", "tests", "recovery-db.browser.html")],
    ["/thinoda-web/tests/recovery-db.browser.js", path.join(__dirname, "crates", "thinoda-web", "tests", "recovery-db.browser.js")],
    ["/thinoda-platform/src/recovery_db.js", path.join(__dirname, "crates", "thinoda-platform", "src", "recovery_db.js")],
    ["/crates/thinoda-platform/src/browser_storage.js", path.join(root, "crates", "thinoda-platform", "src", "browser_storage.js")],
]);

async function main() {
    const [browserName = "chromium", filter] = process.argv.slice(2);
    if (!["chromium", "firefox", "webkit"].includes(browserName) || process.argv.length > 4) {
        throw new Error("Usage: node run-browser-tests.cjs [chromium|firefox|webkit] [filter]");
    }

    const server = http.createServer(async (request, response) => {
        const pathname = new URL(request.url, "http://127.0.0.1").pathname;
        const asset = assets.get(pathname);
        if (!asset) {
            response.writeHead(404).end();
            return;
        }
        try {
            const contents = await readFile(asset);
            response.writeHead(200, {
                "Content-Type": asset.endsWith(".html") ? "text/html; charset=utf-8" : "text/javascript; charset=utf-8",
                "Cache-Control": "no-store",
            });
            response.end(contents);
        } catch (error) {
            console.error(error);
            response.writeHead(500).end();
        }
    });
    await new Promise((resolve, reject) => {
        server.once("error", reject);
        server.listen(0, "127.0.0.1", resolve);
    });

    let browser;
    try {
        const origin = `http://127.0.0.1:${server.address().port}`;
        const target = new URL("/thinoda-web/tests/recovery-db.browser.html", origin);
        if (filter) target.searchParams.set("filter", filter);
        browser = await browsers[browserName].launch({ headless: true });
        const context = await browser.newContext();
        await context.route("**/*", route => {
            const url = new URL(route.request().url());
            return url.origin === origin ? route.continue() : route.abort();
        });
        const page = await context.newPage();
        const response = await page.goto(target.href);
        if (!response?.ok()) {
            throw new Error(`Could not load recovery harness: ${response?.status() ?? "no response"}`);
        }
        try {
            await page.waitForFunction(
                () => ["pass", "fail"].includes(document.body.dataset.result),
                null,
                { timeout: 240_000 },
            );
        } catch (error) {
            const progress = await page.locator("#results").innerText();
            throw new Error(`Recovery harness did not finish:\n${progress}`, { cause: error });
        }
        const result = await page.locator("body").getAttribute("data-result");
        const output = await page.locator("#results").innerText();
        if (result !== "pass") throw new Error(output);
        console.log(`${browserName}: ${output.split("\n").at(-1)}`);
    } finally {
        if (browser) await browser.close();
        await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
    }
}

main().catch(error => {
    console.error(error);
    process.exitCode = 1;
});
