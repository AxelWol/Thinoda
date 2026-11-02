"use strict";

const { defineConfig } = require("@playwright/test");

module.exports = defineConfig({
    testDir: "./tests",
    testMatch: [
        "board-export.browser.spec.ts",
        "board-workflows.browser.spec.ts",
        "image-card.browser.spec.ts",
        "language.browser.spec.ts",
        "recovery-legacy-lock.browser.spec.ts",
        "startup.browser.spec.ts",
        "startup-service-worker.browser.spec.ts",
    ],
    workers: 1,
    reporter: "line",
    use: {
        browserName: "chromium",
        headless: true,
        locale: "en-US",
        launchOptions: {
            executablePath: process.env.THINODA_BROWSER_EXECUTABLE || undefined,
            args: ["--enable-unsafe-swiftshader"],
        },
    },
    webServer: process.env.THINODA_WEB_URL ? undefined : {
        command: `${process.platform === "win32" ? "python" : "python3"} tests/serve-browser-tests.py`,
        url: "http://127.0.0.1:8080/",
        reuseExistingServer: false,
        timeout: 30_000,
    },
});
