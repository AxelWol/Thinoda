"use strict";

const path = require("node:path");
const { defineConfig } = require("@playwright/test");

const browser = process.env.THINODA_PROBE_BROWSER || "chromium";
const dist = path.resolve(__dirname, "..", "..", "..", "target", "accesskit-probe");
const url = process.env.THINODA_ACCESSKIT_PROBE_URL || "http://127.0.0.1:18871/";

module.exports = defineConfig({
    testDir: ".",
    testMatch: "accesskit-probe.browser.spec.ts",
    outputDir: path.resolve(__dirname, "..", "..", "..", "target", "accesskit-probe-results"),
    workers: 1,
    reporter: "line",
    use: {
        baseURL: url,
        browserName: browser,
        headless: true,
        viewport: { width: 1024, height: 768 },
        deviceScaleFactor: 1,
        serviceWorkers: "block",
        launchOptions: {
            executablePath: process.env.THINODA_BROWSER_EXECUTABLE || undefined,
            args: browser === "chromium" ? ["--enable-unsafe-swiftshader"] : [],
        },
    },
    webServer: process.env.THINODA_ACCESSKIT_PROBE_URL ? undefined : {
        command: `${process.platform === "win32" ? "python" : "python3"} -m http.server 18871 --bind 127.0.0.1 --directory "${dist}"`,
        url,
        reuseExistingServer: false,
        stderr: "ignore",
        timeout: 30_000,
    },
});
