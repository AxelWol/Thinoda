import { execFileSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { expect, test, type Page } from "@playwright/test";

const { assertRoundTrip, validateBoard } = require("./board-fixture.cjs");
const { decodeImages, markerPixels } = require("./board-browser.cjs");
const generator = resolve(__dirname, "make-board-fixtures.py");
const source = resolve(__dirname, "..", "fixtures", "affinity-diagram.thinoda");
const python = process.env.PYTHON || (process.platform === "win32" ? "python" : "python3");
const code = [
    "import importlib.util,pathlib,sys",
    "s=importlib.util.spec_from_file_location('board_fixture',sys.argv[1])",
    "m=importlib.util.module_from_spec(s)",
    "s.loader.exec_module(m)",
    "sys.stdout.buffer.write(m.build_archive(pathlib.Path(sys.argv[2]),int(sys.argv[3]),sys.argv[4]=='1')[0])",
].join(";");

test.setTimeout(240_000);

function fixture(strokes: number, embeddedImages: boolean): Buffer {
    return execFileSync(python, ["-B", "-c", code, generator, source, String(strokes), embeddedImages ? "1" : "0"], {
        maxBuffer: 8 * 1024 * 1024,
    });
}

async function open(page: Page, bytes: Buffer, name: string) {
    const url = process.env.THINODA_WEB_URL;
    if (!url) throw new Error("Set THINODA_WEB_URL to a loopback production-build server");
    const parsed = new URL(url);
    if (!["http:", "https:"].includes(parsed.protocol)
        || !["127.0.0.1", "localhost", "[::1]"].includes(parsed.hostname)) {
        throw new Error("The browser fixture test requires a loopback URL");
    }
    await page.route("**/*", route => new URL(route.request().url()).origin === parsed.origin
        ? route.continue()
        : route.abort("blockedbyclient"));
    await page.goto(url, { waitUntil: "load", timeout: 120_000 });
    await page.locator("#loading").waitFor({ state: "hidden", timeout: 120_000 });
    await page.locator("#thinoda_canvas").drop({
        files: { name, mimeType: "application/octet-stream", buffer: bytes },
    });
}

async function save(page: Page): Promise<Buffer> {
    await page.evaluate(() => { window.prompt = () => "embedded-benchmark.thinoda"; });
    const pending = page.waitForEvent("download", { timeout: 120_000 });
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+Shift+s");
    const download = await pending;
    expect(download.suggestedFilename()).toBe("embedded-benchmark.thinoda");
    return readFile(await download.path());
}

for (const strokes of [1000, 10000]) {
    test(`the default ${strokes}-stroke placeholder board remains unembedded`, async ({ page }) => {
        const bytes = fixture(strokes, false);
        const expected = validateBoard(bytes, { strokes, embeddedImages: false });
        await open(page, bytes, `board-${strokes}.thinoda`);
        await page.waitForTimeout(3_000);
        const saved = validateBoard(await save(page), { strokes, embeddedImages: false });
        assertRoundTrip(expected, saved);
        expect(await markerPixels(page, await page.screenshot())).toBeLessThan(30);
    });

    test(`${strokes} strokes and 50 embedded images render and survive a browser save`, async ({ page }) => {
        const bytes = fixture(strokes, true);
        const expected = validateBoard(bytes, { strokes, embeddedImages: true });
        await open(page, bytes, `board-${strokes}-embedded.thinoda`);
        await expect.poll(async () => markerPixels(page, await page.screenshot()), {
            message: "The image marker must be visible in the actual board, not only in the archive",
            timeout: 60_000,
            intervals: [1000, 2000, 3000],
        }).toBeGreaterThanOrEqual(30);
        const saved = validateBoard(await save(page), { strokes, embeddedImages: true });
        assertRoundTrip(expected, saved);
        expect(await decodeImages(page, saved)).toBe(50);
    });
}
