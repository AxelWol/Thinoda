"use strict";

const assert = require("node:assert/strict");
const { createHash } = require("node:crypto");
const { readFile, writeFile } = require("node:fs/promises");
const { basename, dirname, join, resolve } = require("node:path");
const { chromium } = require("@playwright/test");
const { IMAGE_HEIGHT, IMAGE_WIDTH, assertRoundTrip, validateBoard } = require("./board-fixture.cjs");
const { decodeImages, markerPixels, waitForMarker } = require("./board-browser.cjs");

const args = process.argv.slice(2);
const embeddedImages = args.length === 5 && args[4] === "--embedded-images";
if (args.length !== (embeddedImages ? 5 : 4)) {
    throw new Error("Usage: node compare-board-frames.cjs FIXTURES BASELINE_URL CANDIDATE_URL OUTPUT_JSON [--embedded-images]");
}
const [fixtures, baseline, candidate, output] = args;

if (embeddedImages) {
    for (const url of [baseline, candidate]) {
        const parsed = new URL(url);
        assert(["http:", "https:"].includes(parsed.protocol)
            && ["127.0.0.1", "localhost", "[::1]"].includes(parsed.hostname),
        "embedded-image benchmarks require loopback URLs");
    }
}

async function run(browser, url, fixturePath, expected, runNumber) {
    const strokes = expected.strokes;
    const context = await browser.newContext({
        viewport: { width: 1252, height: 900 },
        deviceScaleFactor: 1,
        locale: "en-US",
        serviceWorkers: "block",
        acceptDownloads: true,
    });
    const page = await context.newPage();
    const origin = new URL(url).origin;
    await page.route("**/*", route => {
        if (new URL(route.request().url()).origin === origin) return route.continue();
        return route.abort("blockedbyclient");
    });
    try {
        await page.goto(url, { waitUntil: "load", timeout: 120_000 });
        await page.locator("#loading").waitFor({ state: "hidden", timeout: 120_000 });
        const canvas = page.locator("#thinoda_canvas");
        await canvas.drop({ files: fixturePath });
        await page.waitForTimeout(3_000);
        if (embeddedImages) await waitForMarker(page);
        await page.evaluate(() => { window.prompt = () => "performance-check.thinoda"; });
        const pending = page.waitForEvent("download", { timeout: 120_000 });
        await canvas.focus();
        await page.keyboard.press("Control+Shift+s");
        const download = await pending;
        assert.equal(download.suggestedFilename(), "performance-check.thinoda");
        const saved = await readFile(await download.path());
        const actual = validateBoard(saved, { strokes, embeddedImages: expected.embeddedImages });
        assertRoundTrip(expected, actual);
        await page.mouse.move(550, 420);
        await page.waitForTimeout(500);
        const before = await page.screenshot();
        const visibleMarkerPixels = embeddedImages ? await markerPixels(page, before) : null;
        if (embeddedImages) assert(visibleMarkerPixels >= 30, "embedded image marker is not visible on the board");
        const raw = await page.evaluate(async () => {
            const canvas = document.querySelector("#thinoda_canvas");
            if (!(canvas instanceof HTMLCanvasElement)) throw new Error("Missing canvas");
            const rect = canvas.getBoundingClientRect();
            const intervals = [];
            let previous;
            for (let index = 0; index < 81; index++) {
                await new Promise(resolve => requestAnimationFrame(resolve));
                const now = performance.now();
                if (index >= 11) intervals.push(now - previous);
                previous = now;
                canvas.dispatchEvent(new WheelEvent("wheel", {
                    bubbles: true,
                    cancelable: true,
                    deltaY: Math.floor(index / 10) % 2 ? -12 : 12,
                    clientX: rect.left + rect.width * 0.45,
                    clientY: rect.top + rect.height * 0.5,
                }));
            }
            return intervals;
        });
        await page.waitForTimeout(100);
        const after = await page.screenshot();
        const sorted = [...raw].sort((a, b) => a - b);
        const hash = data => createHash("sha256").update(data).digest("hex");
        const browserDecodedImages = embeddedImages ? await decodeImages(page, actual) : 0;
        const browserInfo = await page.evaluate(() => {
            const gl = document.createElement("canvas").getContext("webgl");
            const extension = gl?.getExtension("WEBGL_debug_renderer_info");
            return {
                userAgent: navigator.userAgent,
                webglRenderer: extension && gl
                    ? gl.getParameter(extension.UNMASKED_RENDERER_WEBGL)
                    : null,
            };
        });
        const result = {
            url,
            strokes,
            run: runNumber,
            ...browserInfo,
            samples: raw.length,
            p50_ms: sorted[Math.floor((sorted.length - 1) * 0.5)],
            p95_ms: sorted[Math.floor((sorted.length - 1) * 0.95)],
            max_ms: sorted.at(-1),
            pixelsChanged: hash(before) !== hash(after),
            intervals_ms: raw,
            fixture: {
                file: basename(fixturePath),
                mode: embeddedImages ? "embedded-png" : "placeholders",
                sha256: expected.archiveSha256,
                archiveBytes: expected.archiveBytes,
                imageCards: expected.imageCards,
                embeddedResources: expected.embeddedResources,
                encodedImageBytes: expected.encodedImageBytes,
                imageDimensions: embeddedImages ? [IMAGE_WIDTH, IMAGE_HEIGHT] : null,
                nominalRgbaBytes: expected.nominalRgbaBytes,
                savedSha256: actual.archiveSha256,
                savedArchiveBytes: actual.archiveBytes,
                browserDecodedImages,
                visibleMarkerPixels,
            },
        };
        assert(result.pixelsChanged, "wheel sequence did not visibly change the canvas");
        console.log(JSON.stringify({ ...result, intervals_ms: undefined }));
        return result;
    } finally {
        await context.close();
    }
}

(async () => {
    const browser = await chromium.launch({
        headless: true,
        args: ["--enable-unsafe-swiftshader"],
        downloadsPath: dirname(resolve(output)),
    });
    const results = [];
    const recordedAtUtc = new Date().toISOString();
    try {
        for (const strokes of [1000, 10000]) {
            const suffix = embeddedImages ? "-embedded" : "";
            const fixturePath = join(fixtures, `board-${strokes}${suffix}.thinoda`);
            const expected = validateBoard(await readFile(fixturePath), { strokes, embeddedImages });
            for (const [runNumber, versions] of [
                [1, ["baseline", "candidate"]],
                [2, ["candidate", "baseline"]],
            ]) {
                for (const version of versions) {
                    const url = version === "baseline" ? baseline : candidate;
                    const result = await run(browser, url, fixturePath, expected, runNumber);
                    results.push({ version, ...result });
                    await writeFile(output, JSON.stringify({
                        recordedAtUtc,
                        browser: browser.version(),
                        methodology: {
                            viewport: [1252, 900],
                            serviceWorkers: "blocked",
                            samples: "70 requestAnimationFrame intervals after 10 warmup intervals, with synthetic wheel events",
                            resourceValidation: "native ZIP and PNG checks; saved references, metadata and bytes match the source",
                            strokeValidation: "ordered IDs and f32-exact geometry, pressure and style fingerprints match the source",
                            comparison: baseline === candidate
                                ? "same URL: smoke and characterization only, not an optimization comparison"
                                : "separate baseline and candidate URLs",
                            limitations: [
                                "nominalRgbaBytes is arithmetic, not measured memory",
                                "browser decoding is independent of Thinoda; visible marker pixels prove only that some image content was painted",
                                "screenshot differences may include UI changes and are not proof of wheel movement",
                                "animation-frame intervals are not GPU presentation or input-to-display latency",
                                "no physical-device, screen-reader, quota-recovery or F5 acceptance is established",
                            ],
                        },
                        results,
                    }, null, 2));
                }
            }
        }
    } finally {
        await browser.close();
    }
})().catch(error => {
    console.error(error);
    process.exitCode = 1;
});
