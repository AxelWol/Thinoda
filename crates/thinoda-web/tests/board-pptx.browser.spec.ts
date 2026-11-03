import { expect, test, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { command, frames, saveAs, zipEntry } from "./board-browser-helpers";

type BoardSheet = {
    root: number;
    root_mode: string;
    floating: number[];
    topics: Record<string, { id: number; title: string; card?: { kind: string; color?: string } }>;
    canvas: { objects: { kind: { shape?: { shape: string } }; style: { fill?: string } }[] };
    presentation: { slides: { source: string }[] };
};

declare global {
    interface Window {
        __powerPointPrompts?: { message?: string; suggested?: string }[];
    }
}

test.use({
    viewport: { width: 1252, height: 1278 },
    deviceScaleFactor: 1,
    locale: "en-US",
    colorScheme: "light",
    serviceWorkers: "block",
    acceptDownloads: true,
    screenshot: "only-on-failure",
});
test.setTimeout(180_000);

function sheetFrom(zip: Buffer): BoardSheet {
    const content: { sheets: BoardSheet[] } = JSON.parse(zipEntry(zip, "content.json").toString("utf8"));
    return content.sheets[0];
}

async function clickCanvasButton(page: Page, x: number, y: number) {
    await page.mouse.move(x, y);
    await expect.poll(() => page.locator("#thinoda_canvas").evaluate(
        (canvas) => (canvas as HTMLCanvasElement).style.cursor,
    )).toBe("pointer");
    await page.mouse.click(x, y);
    await frames(page);
}

async function startPowerPointPreflight(page: Page) {
    await command(page, "PowerPoint");
    await page.waitForTimeout(500);
    // The accessible semantics of these egui dialogs are not exposed in the
    // browser; fixed coordinates are scoped to this test's fixed viewport.
    await clickCanvasButton(page, 882, 876);
    await page.waitForTimeout(300);
    expect(await page.evaluate(() => window.__powerPointPrompts)).toEqual([]);
}

test("captured rootless board PowerPoint export requires consent and preserves its viewport", async ({ page }) => {
    await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+n");
    await frames(page);
    await command(page, "Hide central topic");
    await page.keyboard.press("Escape");
    await frames(page);

    await command(page, "Sticky note");
    await page.mouse.click(220, 400);
    await frames(page);
    await page.locator("input[type='text']").pressSequentially("PowerPoint note");
    await page.keyboard.press("Enter");
    await frames(page);
    await command(page, "Quick Style: Warning");

    await command(page, "Shape tool");
    await page.mouse.click(650, 400);
    await frames(page);
    await command(page, "Quick Style: Warning");

    await command(page, "Edit Presentation");
    await page.waitForTimeout(500);
    await page.mouse.click(482, 403);
    await page.waitForTimeout(500);
    await page.mouse.click(133, 963);
    await frames(page);

    const baseline = sheetFrom(await saveAs(page, "before-powerpoint.thinoda"));
    expect(baseline.root_mode).toBe("hidden");
    expect(baseline.presentation.slides[0].source).toBe("viewport");
    expect(Object.values(baseline.topics).some(
        topic => topic.title === "PowerPoint note" && topic.card?.color === "#FFEDD5",
    )).toBe(true);
    expect(baseline.canvas.objects.some(
        object => object.kind.shape?.shape === "rectangle" && object.style.fill === "#FFEDD5",
    )).toBe(true);

    await page.evaluate(() => {
        window.__powerPointPrompts = [];
        window.prompt = (message, suggested) => {
            window.__powerPointPrompts?.push({ message, suggested });
            return "rootless-viewport.pptx";
        };
    });
    const downloads: string[] = [];
    page.on("download", download => downloads.push(download.suggestedFilename()));
    await startPowerPointPreflight(page);
    await clickCanvasButton(page, 765, 711);
    await page.waitForTimeout(300);
    expect(downloads).toEqual([]);
    expect(await page.evaluate(() => window.__powerPointPrompts)).toEqual([]);

    await startPowerPointPreflight(page);
    const pending = page.waitForEvent("download", { timeout: 60_000 });
    await clickCanvasButton(page, 850, 711);
    const download = await pending;
    expect(download.suggestedFilename()).toBe("rootless-viewport.pptx");
    expect(await page.evaluate(() => window.__powerPointPrompts)).toEqual([{
        message: "File name",
        suggested: expect.stringMatching(/\.pptx$/i),
    }]);
    const filename = await download.path();
    if (!filename) throw new Error("PowerPoint download has no local path");
    const pptx = await readFile(filename);
    const slide = zipEntry(pptx, "ppt/slides/slide1.xml").toString("utf8");
    const relationships = zipEntry(pptx, "ppt/slides/_rels/slide1.xml.rels").toString("utf8");
    expect(slide).toContain("Flattened viewport scene");
    expect(relationships).toContain("../media/slide1-viewport.png");
    const image = zipEntry(pptx, "ppt/media/slide1-viewport.png");
    expect(image.subarray(0, 8)).toEqual(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]));
    const raster = await page.evaluate(async (base64) => {
        const image = new Image();
        image.src = `data:image/png;base64,${base64}`;
        await image.decode();
        const canvas = document.createElement("canvas");
        canvas.width = image.naturalWidth;
        canvas.height = image.naturalHeight;
        const context = canvas.getContext("2d");
        if (!context) throw new Error("Cannot inspect the viewport image");
        context.drawImage(image, 0, 0);
        const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
        const columns = new Array<number>(canvas.width).fill(0);
        for (let y = 0; y < canvas.height; y++) {
            for (let x = 0; x < canvas.width; x++) {
                const offset = (y * canvas.width + x) * 4;
                if (pixels[offset] === 255 && pixels[offset + 1] === 237 && pixels[offset + 2] === 213) {
                    columns[x]++;
                }
            }
        }
        let groups = 0;
        let inside = false;
        for (const column of columns) {
            const filled = column > 3;
            if (filled && !inside) groups++;
            inside = filled;
        }
        return { width: canvas.width, height: canvas.height, groups, pixels: columns.reduce((a, b) => a + b, 0) };
    }, image.toString("base64"));
    expect(raster.width).toBeGreaterThan(100);
    expect(raster.height).toBeGreaterThan(100);
    expect(raster.pixels).toBeGreaterThan(1_000);
    expect(raster.groups).toBeGreaterThanOrEqual(2);
    expect(sheetFrom(await saveAs(page, "after-powerpoint.thinoda"))).toEqual(baseline);
});
