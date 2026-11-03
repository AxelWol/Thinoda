"use strict";

const assert = require("node:assert/strict");
const { createHash } = require("node:crypto");
const { readFile, writeFile } = require("node:fs/promises");
const { join } = require("node:path");
const { inflateRawSync } = require("node:zlib");
const { chromium } = require("@playwright/test");

const [fixtures, baseline, candidate, output] = process.argv.slice(2);
if (!fixtures || !baseline || !candidate || !output) {
    throw new Error("Usage: node compare-board-frames.cjs FIXTURES BASELINE_URL CANDIDATE_URL OUTPUT_JSON");
}

function entry(zip, name) {
    const end = zip.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
    assert(end >= 0, "missing ZIP directory");
    const count = zip.readUInt16LE(end + 10);
    let offset = zip.readUInt32LE(end + 16);
    for (let i = 0; i < count; i++) {
        assert.equal(zip.readUInt32LE(offset), 0x02014b50);
        const method = zip.readUInt16LE(offset + 10);
        const size = zip.readUInt32LE(offset + 20);
        const nameLength = zip.readUInt16LE(offset + 28);
        const extraLength = zip.readUInt16LE(offset + 30);
        const commentLength = zip.readUInt16LE(offset + 32);
        const localOffset = zip.readUInt32LE(offset + 42);
        if (zip.toString("utf8", offset + 46, offset + 46 + nameLength) === name) {
            assert.equal(zip.readUInt32LE(localOffset), 0x04034b50);
            const start = localOffset + 30 + zip.readUInt16LE(localOffset + 26)
                + zip.readUInt16LE(localOffset + 28);
            const bytes = zip.subarray(start, start + size);
            if (method === 0) return bytes;
            if (method === 8) return inflateRawSync(bytes);
            throw new Error(`Unsupported ZIP compression method ${method}`);
        }
        offset += 46 + nameLength + extraLength + commentLength;
    }
    throw new Error(`Missing ZIP entry ${name}`);
}

async function run(browser, url, strokes, runNumber) {
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
        await canvas.drop({ files: join(fixtures, `board-${strokes}.thinoda`) });
        await page.waitForTimeout(3_000);
        await page.evaluate(() => { window.prompt = () => "performance-check.thinoda"; });
        const pending = page.waitForEvent("download", { timeout: 120_000 });
        await canvas.focus();
        await page.keyboard.press("Control+Shift+s");
        const download = await pending;
        const saved = await readFile(await download.path());
        const sheet = JSON.parse(entry(saved, "content.json").toString("utf8")).sheets[0];
        assert.equal(sheet.title, `Performance ${strokes}`);
        assert.equal(sheet.root_mode, "hidden");
        assert.equal(sheet.canvas.objects.length, strokes);
        assert(sheet.canvas.objects.every(object => object.kind?.ink?.points?.length === 64));
        assert.equal(Object.values(sheet.topics).filter(topic => topic.card?.kind === "image").length, 50);
        await page.mouse.move(550, 420);
        await page.waitForTimeout(500);
        const before = await page.screenshot();
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
    });
    const results = [];
    try {
        for (const strokes of [1000, 10000]) {
            for (const [runNumber, versions] of [
                [1, ["baseline", "candidate"]],
                [2, ["candidate", "baseline"]],
            ]) {
                for (const version of versions) {
                    const url = version === "baseline" ? baseline : candidate;
                    const result = await run(browser, url, strokes, runNumber);
                    results.push({ version, ...result });
                    await writeFile(output, JSON.stringify({
                        browser: browser.version(),
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
