import { expect, test, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { crc32 } from "node:zlib";
import { colorBounds, command, frames, saveAs, withPdfViewer, zipEntry } from "./board-browser-helpers";

type Point = { x: number; y: number };
type Frame = { min: Point; size: Point };
type Endpoint = { point: Point } | { object: number };
type BoardSheet = {
    title: string;
    root: number;
    root_mode: string;
    floating: number[];
    topics: Record<string, {
        id: number; title: string; position?: Point; card?: { kind: string; color?: string };
    }>;
    zones: { id: number; name: string; frame: Frame; roots?: number[]; objects?: number[]; keep_empty?: boolean }[];
    canvas: { objects: {
        id: number;
        kind: {
            shape?: { shape: string; label?: string };
            connector?: { from: Endpoint; to: Endpoint; label?: string; color?: string; width?: number };
        };
        rect: Frame;
        style: { fill?: string };
    }[] };
    presentation: { slides: {
        id: number; title: string; source: string | { zone: number } | { branch: number };
        frame: Frame; scope: string; auto_frame: boolean;
    }[] };
    next_id: number;
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

function storedZip(files: { name: string; data: Buffer }[]): Buffer {
    const local: Buffer[] = [];
    const central: Buffer[] = [];
    let offset = 0;
    for (const { name, data } of files) {
        const filename = Buffer.from(name);
        const checksum = crc32(data);
        const header = Buffer.alloc(30);
        header.writeUInt32LE(0x04034b50, 0);
        header.writeUInt16LE(20, 4);
        header.writeUInt16LE(0x800, 6);
        header.writeUInt32LE(checksum, 14);
        header.writeUInt32LE(data.length, 18);
        header.writeUInt32LE(data.length, 22);
        header.writeUInt16LE(filename.length, 26);
        local.push(header, filename, data);

        const directory = Buffer.alloc(46);
        directory.writeUInt32LE(0x02014b50, 0);
        directory.writeUInt16LE(20, 4);
        directory.writeUInt16LE(20, 6);
        directory.writeUInt16LE(0x800, 8);
        directory.writeUInt32LE(checksum, 16);
        directory.writeUInt32LE(data.length, 20);
        directory.writeUInt32LE(data.length, 24);
        directory.writeUInt16LE(filename.length, 28);
        directory.writeUInt32LE(offset, 42);
        central.push(directory, filename);
        offset += header.length + filename.length + data.length;
    }
    const centralSize = central.reduce((sum, part) => sum + part.length, 0);
    const end = Buffer.alloc(22);
    end.writeUInt32LE(0x06054b50, 0);
    end.writeUInt16LE(files.length, 8);
    end.writeUInt16LE(files.length, 10);
    end.writeUInt32LE(centralSize, 12);
    end.writeUInt32LE(offset, 16);
    return Buffer.concat([...local, ...central, end]);
}

async function isolationFixture(): Promise<Buffer> {
    const base = await readFile(join(__dirname, "fixtures", "affinity-diagram.thinoda"));
    const workbook: { sheets: BoardSheet[] } = JSON.parse(zipEntry(base, "content.json").toString("utf8"));
    const sheet = workbook.sheets[0];
    const frame: Frame = { min: { x: 0, y: 0 }, size: { x: 500, y: 350 } };
    const rect = (x: number, y: number, width: number, height: number): Frame => ({
        min: { x, y }, size: { x: width, y: height },
    });
    sheet.title = "Scene isolation";
    sheet.root_mode = "hidden";
    sheet.topics = {
        "1": { id: 1, title: "" },
        "2": { id: 2, title: "Source root", position: { x: 70, y: 65 } },
    };
    sheet.floating = [2];
    sheet.zones = [
        { id: 3, name: "Source zone", frame, roots: [2], objects: [5, 7, 8] },
        { id: 4, name: "Foreign zone", frame, objects: [6], keep_empty: true },
    ];
    sheet.canvas = { objects: [
        { id: 5, kind: { shape: { shape: "rectangle", label: "Source object" } },
            rect: rect(40, 200, 125, 90), style: { fill: "#00C87A" } },
        { id: 6, kind: { shape: { shape: "rectangle", label: "Foreign object" } },
            rect: rect(290, 40, 125, 90), style: { fill: "#E32367" } },
        { id: 7, kind: { connector: {
            from: { point: { x: 200, y: 50 } }, to: { object: 6 }, label: "Foreign connector",
            color: "#FB7A00", width: 16,
        } }, rect: rect(200, 50, 0, 0), style: {} },
        { id: 8, kind: { connector: {
            from: { point: { x: 200, y: 100 } }, to: { object: 7 }, label: "Indirect connector",
            color: "#9F33E5", width: 16,
        } }, rect: rect(200, 100, 0, 0), style: {} },
        { id: 9, kind: { shape: { shape: "rectangle", label: "Unassigned object" } },
            rect: rect(290, 200, 125, 90), style: { fill: "#2255D9" } },
    ] };
    sheet.presentation = { slides: [
        { id: 10, title: "Zone", source: { zone: 3 }, frame, scope: "source", auto_frame: false },
        { id: 11, title: "Branch", source: { branch: 2 }, frame, scope: "source", auto_frame: false },
        { id: 12, title: "Context", source: { zone: 3 }, frame, scope: "context", auto_frame: false },
        { id: 13, title: "View", source: "viewport", frame, scope: "source", auto_frame: false },
    ] };
    sheet.next_id = 14;
    const manifest: { format: string; version: number; resources: unknown[] } =
        JSON.parse(zipEntry(base, "manifest.json").toString("utf8"));
    manifest.version = 9;
    return storedZip([
        { name: "manifest.json", data: Buffer.from(JSON.stringify(manifest)) },
        { name: "content.json", data: Buffer.from(JSON.stringify(workbook)) },
    ]);
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

test("source-only Zone and branch scenes stay isolated in Pitch and presentation exports", async ({ page }) => {
    test.setTimeout(240_000);
    await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    await page.evaluate(() => {
        Object.defineProperty(window, "showOpenFilePicker", { configurable: true, value: undefined });
    });
    const chooser = page.waitForEvent("filechooser");
    await command(page, "Open");
    const fixture = await isolationFixture();
    await test.info().attach("scene-isolation.thinoda", {
        body: fixture, contentType: "application/octet-stream",
    });
    await (await chooser).setFiles({
        name: "scene-isolation.thinoda",
        mimeType: "application/octet-stream",
        buffer: fixture,
    });
    const colors = [
        [0, 200, 122], [227, 35, 103], [34, 85, 217], [251, 122, 0], [159, 51, 229],
    ] as const;
    const visibility = [
        [true, false, true, false, false],
        [false, false, true, false, false],
        [true, true, true, true, true],
        [true, true, true, true, true],
    ] as const;
    await expect.poll(async () => {
        const counts = await colorBounds(page, await page.screenshot(), colors, 5);
        return Math.min(...counts.map(count => count.count));
    }, { timeout: 30_000 }).toBeGreaterThan(100);
    const baseline = sheetFrom(await saveAs(page, "scene-isolation-baseline.thinoda"));
    expect(baseline.presentation.slides.map(slide => slide.source)).toEqual([
        { zone: 3 }, { branch: 2 }, { zone: 3 }, "viewport",
    ]);

    await command(page, "Pitch");
    await page.waitForTimeout(500);
    for (const [index, visible] of visibility.entries()) {
        if (index > 0) {
            await page.keyboard.press("ArrowRight");
            await frames(page);
        }
        const counts = await colorBounds(page, await page.screenshot(), colors, 5);
        for (const [color, shouldShow] of visible.entries()) {
            if (shouldShow) expect(counts[color].count, `slide ${index + 1}, color ${color}`).toBeGreaterThan(100);
            else expect(counts[color].count, `slide ${index + 1}, color ${color}`).toBeLessThan(20);
        }
    }
    await page.keyboard.press("Escape");
    await frames(page);

    await command(page, "Edit Presentation");
    await page.mouse.move(1100, 900);
    await page.mouse.wheel(0, 2200);
    await frames(page);
    await page.evaluate(() => { window.prompt = () => "scene-isolation.pdf"; });
    await clickCanvasButton(page, 150, 931);
    const pendingPdf = page.waitForEvent("download", { timeout: 60_000 });
    await clickCanvasButton(page, 850, 711);
    const pdfDownload = await pendingPdf;
    expect(pdfDownload.suggestedFilename()).toBe("scene-isolation.pdf");
    const pdf = await readFile(await pdfDownload.path());
    expect(pdf.subarray(0, 5).toString("ascii")).toBe("%PDF-");
    expect(pdf.toString("latin1").match(/\/Type\s*\/Page\b/g)).toHaveLength(4);
    await withPdfViewer(pdf, async viewer => {
        await viewer.setViewportSize({ width: 1252, height: 900 });
        for (const [index, visible] of visibility.entries()) {
            const url = `http://127.0.0.1:9/scene-slide-${index + 1}.pdf`;
            await viewer.route(url, route => route.fulfill({
                status: 200, contentType: "application/pdf", body: pdf,
            }));
            const response = await viewer.goto(
                `${url}#page=${index + 1}&view=Fit&toolbar=0&navpanes=0`,
            );
            expect(response?.status()).toBe(200);
            await expect.poll(async () => {
                const counts = await colorBounds(page, await viewer.screenshot(), colors, 5);
                return counts.map(({ count }) => {
                    if (count > 100) return true;
                    if (count < 20) return false;
                    return count;
                });
            }, { timeout: 30_000, message: `PDF slide ${index + 1} visibility` }).toEqual(visible);
        }
    });

    await page.evaluate(() => { window.prompt = () => "scene-isolation-images.zip"; });
    await clickCanvasButton(page, 260, 931);
    const pendingImages = page.waitForEvent("download", { timeout: 60_000 });
    await clickCanvasButton(page, 850, 711);
    const imagesDownload = await pendingImages;
    expect(imagesDownload.suggestedFilename()).toBe("scene-isolation-images.zip");
    const images = await readFile(await imagesDownload.path());
    for (const [index, visible] of visibility.entries()) {
        const image = zipEntry(images, `slides/slide-${String(index + 1).padStart(3, "0")}.png`);
        expect(image.subarray(0, 8)).toEqual(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]));
        const counts = await colorBounds(page, image, colors, 5);
        for (const [color, shouldShow] of visible.entries()) {
            if (shouldShow) expect(counts[color].count, `image ${index + 1}, color ${color}`).toBeGreaterThan(100);
            else expect(counts[color].count, `image ${index + 1}, color ${color}`).toBeLessThan(20);
        }
    }
    await clickCanvasButton(page, 192, 963);
    expect(sheetFrom(await saveAs(page, "scene-isolation-after-slide-exports.thinoda"))).toEqual(baseline);

    await page.evaluate(() => {
        window.__powerPointPrompts = [];
        window.prompt = (message, suggested) => {
            window.__powerPointPrompts?.push({ message, suggested });
            return "scene-isolation.pptx";
        };
    });
    await startPowerPointPreflight(page);
    const pending = page.waitForEvent("download", { timeout: 60_000 });
    await clickCanvasButton(page, 850, 711);
    const download = await pending;
    expect(download.suggestedFilename()).toBe("scene-isolation.pptx");
    const pptx = await readFile(await download.path());
    for (const [index, visible] of [
        [true, false, false, false, true],
        [false, false, false, false, true],
        [true, true, true, true, true],
    ].entries()) {
        const slide = zipEntry(pptx, `ppt/slides/slide${index + 1}.xml`).toString("utf8");
        for (const [position, id] of [5, 6, 7, 8, 9].entries()) {
            expect(slide.includes(`name="Canvas object ${id}"`), `slide ${index + 1}, object ${id}`)
                .toBe(visible[position]);
        }
    }
    const viewport = zipEntry(pptx, "ppt/media/slide4-viewport.png");
    const viewportColors = await colorBounds(page, viewport, colors, 5);
    expect(Math.min(...viewportColors.map(color => color.count))).toBeGreaterThan(100);
    expect(sheetFrom(await saveAs(page, "scene-isolation-after-export.thinoda"))).toEqual(baseline);
});
