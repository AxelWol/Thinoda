"use strict";

const { mkdirSync } = require("node:fs");
const { join, resolve } = require("node:path");
const { defineConfig } = require("@playwright/test");

const browserName = process.env.THINODA_BENCH_BROWSER || "chromium";
if (!["chromium", "firefox", "webkit"].includes(browserName)) {
    throw new Error("THINODA_BENCH_BROWSER must be chromium, firefox, or webkit");
}
const artifacts = resolve(__dirname, "..", "..", "..", "..", "target", "board-embedded-playwright");
const downloads = join(artifacts, "downloads");
mkdirSync(downloads, { recursive: true });

module.exports = defineConfig({
    testDir: __dirname,
    testMatch: "board-embedded.browser.spec.ts",
    outputDir: join(artifacts, "results"),
    workers: 1,
    reporter: "line",
    use: {
        browserName,
        headless: true,
        locale: "en-US",
        viewport: { width: 1252, height: 900 },
        deviceScaleFactor: 1,
        serviceWorkers: "block",
        acceptDownloads: true,
        launchOptions: {
            executablePath: process.env.THINODA_BROWSER_EXECUTABLE || undefined,
            downloadsPath: downloads,
            ...(browserName === "chromium" ? { args: ["--enable-unsafe-swiftshader"] } : {}),
        },
    },
});
