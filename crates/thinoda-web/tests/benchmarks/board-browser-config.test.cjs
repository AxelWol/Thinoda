"use strict";

const assert = require("node:assert/strict");
const test = require("node:test");

test("browser selection defaults to Chromium and only Chromium receives its launch flag", () => {
    const configPath = require.resolve("./board-embedded.playwright.config.cjs");
    const previousBrowser = process.env.THINODA_BENCH_BROWSER;
    const previousExecutable = process.env.THINODA_BROWSER_EXECUTABLE;
    delete process.env.THINODA_BROWSER_EXECUTABLE;
    try {
        function loadConfig(browser) {
            if (browser === undefined) delete process.env.THINODA_BENCH_BROWSER;
            else process.env.THINODA_BENCH_BROWSER = browser;
            delete require.cache[configPath];
            return require(configPath);
        }
        for (const browser of [undefined, "chromium", "firefox", "webkit"]) {
            const config = loadConfig(browser);
            assert.equal(config.use.browserName, browser || "chromium");
            if (!browser || browser === "chromium") {
                assert.deepEqual(config.use.launchOptions.args, ["--enable-unsafe-swiftshader"]);
            } else {
                assert.equal(config.use.launchOptions.args, undefined);
            }
        }
        assert.throws(() => loadConfig("unknown"), /THINODA_BENCH_BROWSER/);
    } finally {
        delete require.cache[configPath];
        if (previousBrowser === undefined) delete process.env.THINODA_BENCH_BROWSER;
        else process.env.THINODA_BENCH_BROWSER = previousBrowser;
        if (previousExecutable === undefined) delete process.env.THINODA_BROWSER_EXECUTABLE;
        else process.env.THINODA_BROWSER_EXECUTABLE = previousExecutable;
    }
});
