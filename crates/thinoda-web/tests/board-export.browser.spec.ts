/**
 * Production-browser coverage for exporting a mixed rootless board.
 * The document is created in the test's isolated browser context, and the
 * downloaded SVG, PNG, JPEG and PDF are inspected rather than inferred from the canvas.
 */
import { expect, test, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { inflateSync } from "node:zlib";
import { colorBounds, command, drag, frames, saveAs, withPdfViewer, zipEntry } from "./board-browser-helpers";

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

type BoardRect = { min: { x: number; y: number }; size: { x: number; y: number } };

type ExportPromptState = {
    name: string | null;
    prompts: { message?: string; suggested?: string }[];
};

declare global {
    interface Window {
        __boardExportTest?: ExportPromptState;
    }
}

type BoardSheet = {
    root: number;
    root_mode: string;
    floating: number[];
    board: { grid: string };
    topics: Record<string, {
        id: number;
        title: string;
        position?: { x: number; y: number };
        card?: { kind: string; color?: string; size: { x: number; y: number } };
    }>;
    zones: {
        name: string;
        keep_empty?: boolean;
        color?: string;
        frame: BoardRect;
    }[];
    canvas: {
        objects: {
            id: number;
            kind: {
                shape?: { shape: string; label?: string };
                connector?: { from: unknown; to: unknown; label?: string };
                ink?: { points: { x: number; y: number }[] };
            };
            rect: BoardRect;
            style: { fill?: string; text_color?: string };
        }[];
    };
};

function sheetFrom(zip: Buffer): BoardSheet {
    const content: { sheets: BoardSheet[] } = JSON.parse(zipEntry(zip, "content.json").toString("utf8"));
    return content.sheets[0];
}

async function setExportPrompt(page: Page, response: string | null) {
    await page.evaluate((name) => {
        const state: ExportPromptState = { name, prompts: [] };
        window.__boardExportTest = state;
        window.prompt = (message, suggested) => {
            state.prompts.push({ message, suggested });
            return state.name;
        };
    }, response);
}

async function exportPrompts(page: Page) {
    return page.evaluate(() => {
        if (!window.__boardExportTest) throw new Error("Export prompt was not configured");
        return window.__boardExportTest.prompts;
    });
}

async function startSvgPreflight(page: Page) {
    await command(page, "SVG");
    // The grid-loss warning must block both the filename prompt and the
    // underlying command palette until the user dismisses or confirms it.
    await page.waitForTimeout(300);
    expect(await exportPrompts(page)).toEqual([]);
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+Shift+p");
    await frames(page);
    await expect(page.locator("input[type='text']")).not.toBeFocused();
}

async function confirmVisualPreflight(page: Page) {
    // egui paints this dialog into the canvas, without DOM button nodes.
    // At the fixed viewport above, Confirm is the right-hand action in the
    // centered board-loss dialog; require its interactive cursor before clicking.
    await page.mouse.move(865, 715);
    await expect.poll(() => page.locator("#thinoda_canvas").evaluate(
        (canvas) => (canvas as HTMLCanvasElement).style.cursor,
    )).toBe("pointer");
    await page.mouse.click(865, 715);
    await frames(page);
}

async function confirmEditableImagePreflight(page: Page) {
    // The mandatory disclosure is painted by egui rather than exposed as DOM buttons.
    await page.mouse.move(865, 740);
    await expect.poll(() => page.locator("#thinoda_canvas").evaluate(
        (canvas) => (canvas as HTMLCanvasElement).style.cursor,
    )).toBe("pointer");
    await page.mouse.click(865, 740);
    await frames(page);
}

for (const image of [
    { extension: "svg", mime: "image/svg+xml", input: "editable.svg", name: "editable.drawio.svg" },
    { extension: "png", mime: "image/png", input: "editable.PNG", name: "editable.drawio.png" },
] as const) {
    test(`editable draw.io ${image.extension} requires consent and reopens as a diagram`, async ({ page }) => {
        await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
        await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
        await page.locator("#thinoda_canvas").focus();
        await page.keyboard.press("Control+n");
        await frames(page);
        await command(page, "Hide central topic");
        await command(page, "Shape tool");
        await page.mouse.click(650, 400);
        await frames(page);
        await command(page, "Quick Style: Warning");
        await page.locator("#thinoda_canvas").focus();
        await page.keyboard.press("F2");
        await frames(page);
        await expect(page.locator("input[type='text']")).toBeFocused();
        await page.locator("input[type='text']").pressSequentially("Editable browser shape");
        await page.keyboard.press("Enter");
        await frames(page);

        const baseline = sheetFrom(await saveAs(page, `editable-${image.extension}-before.thinoda`));
        expect(baseline.root_mode).toBe("hidden");
        expect(baseline.canvas.objects.some(
            (object) => object.kind.shape?.label === "Editable browser shape",
        )).toBe(true);

        const downloads: string[] = [];
        page.on("download", (download) => downloads.push(download.suggestedFilename()));
        const label = `draw.io ${image.extension.toUpperCase()} (editable)`;

        await setExportPrompt(page, image.input);
        await command(page, label);
        await page.waitForTimeout(300);
        expect(await exportPrompts(page)).toEqual([]);
        await page.keyboard.press("Escape");
        await frames(page);
        expect(await exportPrompts(page)).toEqual([]);
        expect(downloads).toEqual([]);

        await setExportPrompt(page, null);
        await command(page, label);
        await confirmEditableImagePreflight(page);
        await expect.poll(async () => (await exportPrompts(page)).length).toBe(1);
        expect(await exportPrompts(page)).toEqual([{
            message: "File name",
            suggested: expect.stringMatching(new RegExp(`\\.drawio\\.${image.extension}$`, "i")),
        }]);
        expect(downloads).toEqual([]);

        await setExportPrompt(page, image.input);
        await command(page, label);
        const pending = page.waitForEvent("download", { timeout: 30_000 });
        await confirmEditableImagePreflight(page);
        const download = await pending;
        expect(download.suggestedFilename()).toBe(image.name);
        expect(downloads).toEqual([image.name]);
        const bytes = await readFile(await download.path());

        let embedded: string;
        if (image.extension === "svg") {
            const result = await page.evaluate((source) => {
                const doc = new DOMParser().parseFromString(source, "image/svg+xml");
                return {
                    error: doc.querySelector("parsererror")?.textContent ?? null,
                    root: doc.documentElement.localName,
                    content: doc.documentElement.getAttribute("content"),
                };
            }, bytes.toString("utf8"));
            expect(result.error).toBeNull();
            expect(result.root).toBe("svg");
            expect(result.content).not.toBeNull();
            embedded = result.content!;
        } else {
            expect(bytes.subarray(0, 8)).toEqual(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]));
            const chunks: { kind: string; data: Buffer }[] = [];
            for (let offset = 8; offset < bytes.length;) {
                const length = bytes.readUInt32BE(offset);
                const kind = bytes.toString("ascii", offset + 4, offset + 8);
                chunks.push({ kind, data: bytes.subarray(offset + 8, offset + 8 + length) });
                offset += length + 12;
            }
            expect(chunks[0].kind).toBe("IHDR");
            expect(chunks[1].kind).toBe("tEXt");
            expect(chunks[1].data.subarray(0, 7)).toEqual(Buffer.from("mxfile\0"));
            expect(chunks.findIndex((chunk) => chunk.kind === "IDAT")).toBeGreaterThan(1);
            embedded = decodeURIComponent(chunks[1].data.subarray(7).toString("ascii"));
        }
        expect(embedded).toContain("<mxfile");
        expect(embedded.match(/<diagram\b/g)).toHaveLength(1);
        expect(embedded).toContain("Editable browser shape");
        const dimensions = await page.evaluate(async ({ data, mime }) => {
            const image = new Image();
            image.src = `data:${mime};base64,${data}`;
            await image.decode();
            return [image.naturalWidth, image.naturalHeight];
        }, { data: bytes.toString("base64"), mime: image.mime });
        expect(dimensions.every((size) => size > 0)).toBe(true);
        expect(sheetFrom(await saveAs(page, `editable-${image.extension}-after.thinoda`))).toEqual(baseline);

        await page.locator("#thinoda_canvas").focus();
        await page.keyboard.press("Control+n");
        await frames(page);
        const shapePixels = async () => (
            await colorBounds(
                page,
                await page.screenshot({ clip: { x: 400, y: 300, width: 500, height: 350 } }),
                [[255, 237, 213]],
                5,
            )
        )[0].count;
        // Exclude the similarly colored Quick Style swatch in the side panel.
        await expect.poll(shapePixels).toBeLessThan(200);
        await page.evaluate(() => {
            Object.defineProperty(window, "showOpenFilePicker", { configurable: true, value: undefined });
        });
        const chooser = page.waitForEvent("filechooser");
        await command(page, "Open");
        await (await chooser).setFiles({ name: image.name, mimeType: image.mime, buffer: bytes });
        // The import reports approximated formatting and requires explicit consent.
        // Its egui dialog is painted on the canvas, without DOM button nodes.
        await page.mouse.move(670, 907);
        await expect.poll(() => page.locator("#thinoda_canvas").evaluate(
            (canvas) => (canvas as HTMLCanvasElement).style.cursor,
        ), { timeout: 30_000 }).toBe("pointer");
        await page.mouse.click(670, 907);
        await frames(page);
        await expect.poll(shapePixels, { timeout: 30_000 }).toBeGreaterThan(2_000);
        const reopened = sheetFrom(await saveAs(page, `editable-${image.extension}-reopened.thinoda`));
        expect(reopened.root_mode).toBe("hidden");
        expect(reopened.canvas.objects.some(
            (object) => object.kind.shape?.label === "Editable browser shape",
        )).toBe(true);
    });
}

test("mixed rootless board SVG, PNG and JPEG exports require consent and preserve their objects", async ({ page }, testInfo) => {
    // This end-to-end journey takes almost three minutes on the CI software renderer.
    test.setTimeout(300_000);
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
    await expect(page.locator("input[type='text']")).toBeFocused();
    await page.locator("input[type='text']").pressSequentially("Export idea");
    await page.keyboard.press("Enter");
    await frames(page);
    await command(page, "Quick Style: Warning");

    await command(page, "Shape tool");
    await page.mouse.click(650, 400);
    await frames(page);
    await command(page, "Quick Style: Warning");
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("F2");
    await frames(page);
    await expect(page.locator("input[type='text']")).toBeFocused();
    await page.locator("input[type='text']").pressSequentially("Export shape");
    await page.keyboard.press("Enter");
    await frames(page);

    await command(page, "Connector tool");
    await drag(page, [260, 400], [610, 400]);
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("F2");
    await frames(page);
    await expect(page.locator("input[type='text']")).toBeFocused();
    await page.locator("input[type='text']").pressSequentially("Export link");
    await page.keyboard.press("Enter");
    await frames(page);

    await command(page, "Frame tool");
    await drag(page, [300, 750], [850, 1000]);
    await command(page, "Pen");
    await drag(page, [450, 850], [600, 900], 12);
    // SVG preserves the objects but omits editing-grid settings, exercising the
    // board export preflight on an otherwise visually faithful export.
    await command(page, "Line grid");

    const baseline = sheetFrom(await saveAs(page, "f6-export-baseline.thinoda"));
    expect(baseline.root_mode).toBe("hidden");
    expect(baseline.board.grid).toBe("lines");
    const sticky = Object.values(baseline.topics).find((topic) => topic.card?.kind === "sticky");
    const shape = baseline.canvas.objects.find((object) => object.kind.shape);
    const connector = baseline.canvas.objects.find((object) => object.kind.connector);
    const ink = baseline.canvas.objects.find((object) => object.kind.ink);
    const frame = baseline.zones.find((zone) => zone.keep_empty);
    expect(sticky).toBeDefined();
    expect(shape).toBeDefined();
    expect(connector).toBeDefined();
    expect(ink).toBeDefined();
    expect(frame).toBeDefined();
    if (!sticky || !shape || !connector || !ink || !frame) throw new Error("Incomplete synthetic board");
    expect(sticky.title).toBe("Export idea");
    expect(sticky.card?.color).toBe("#FFEDD5");
    expect(baseline.floating).toContain(sticky.id);
    expect(shape.kind.shape).toMatchObject({ shape: "rectangle", label: "Export shape" });
    expect(shape.style).toMatchObject({ fill: "#FFEDD5", text_color: "#7C2D12" });
    expect(connector.kind.connector).toMatchObject({
        from: { topic: sticky.id },
        to: { object: shape.id },
        label: "Export link",
    });
    expect(ink.kind.ink!.points.length).toBeGreaterThan(1);
    expect(frame.name).toBe("Frame");

    const downloads: string[] = [];
    page.on("download", (download) => downloads.push(download.suggestedFilename()));

    // Dismissing the loss warning must not open the filename prompt, download,
    // or mutate the source board.
    await setExportPrompt(page, "f6-should-not-download.svg");
    await startSvgPreflight(page);
    await page.keyboard.press("Escape");
    await frames(page);
    await page.waitForTimeout(300);
    expect(await exportPrompts(page)).toEqual([]);
    expect(downloads).toEqual([]);
    expect(sheetFrom(await saveAs(page, "f6-after-cancel.thinoda"))).toEqual(baseline);
    expect(downloads).toEqual(["f6-after-cancel.thinoda"]);

    // Consent followed by canceling the filename prompt also must not download.
    await setExportPrompt(page, null);
    await startSvgPreflight(page);
    await confirmVisualPreflight(page);
    await expect.poll(async () => (await exportPrompts(page)).length).toBe(1);
    expect(await exportPrompts(page)).toEqual([{
        message: "File name",
        suggested: expect.stringMatching(/\.svg$/i),
    }]);
    await page.waitForTimeout(300);
    expect(downloads).toEqual(["f6-after-cancel.thinoda"]);

    await setExportPrompt(page, "f6-rootless-board.svg");
    await startSvgPreflight(page);
    const pending = page.waitForEvent("download", { timeout: 30_000 });
    await confirmVisualPreflight(page);
    const download = await pending;
    expect(download.suggestedFilename()).toBe("f6-rootless-board.svg");
    expect(await exportPrompts(page)).toHaveLength(1);
    expect(downloads).toEqual(["f6-after-cancel.thinoda", "f6-rootless-board.svg"]);

    const source = (await readFile(await download.path())).toString("utf8");
    const svg = await page.evaluate(({ source, rootId, stickyId, shapeId, connectorId, inkId, frameStroke }) => {
        const doc = new DOMParser().parseFromString(source, "image/svg+xml");
        const root = doc.documentElement;
        const group = (id: number) => doc.getElementById(`object-${id}`);
        const attrs = (element: Element | null, names: string[]) => element
            ? Object.fromEntries(names.map((name) => [name, element.getAttribute(name)]))
            : null;
        const frameRect = Array.from(doc.querySelectorAll("rect"))
            .find((rect) => rect.getAttribute("stroke")?.toUpperCase() === frameStroke) ?? null;
        return {
            parseError: doc.querySelector("parsererror")?.textContent ?? null,
            name: root.localName,
            namespace: root.namespaceURI,
            viewBox: (root.getAttribute("viewBox") ?? "").trim().split(/\s+/).map(Number),
            hiddenRootPresent: doc.getElementById(`topic-${rootId}-line-0`) !== null,
            stickyText: doc.getElementById(`topic-${stickyId}-line-0`)?.textContent,
            cards: Array.from(doc.querySelectorAll("rect.card"))
                .map((card) => attrs(card, ["x", "y", "width", "height", "fill"])),
            shapeText: group(shapeId)?.querySelector("text")?.textContent,
            shapePath: attrs(group(shapeId)?.querySelector("path") ?? null, ["d", "fill"]),
            connectorText: group(connectorId)?.querySelector("text")?.textContent,
            connectorPath: attrs(group(connectorId)?.querySelector("path") ?? null, ["d", "fill", "stroke"]),
            inkPath: attrs(group(inkId)?.querySelector("path") ?? null, ["d", "fill", "stroke", "stroke-linecap"]),
            frameText: Array.from(doc.querySelectorAll("text")).some((text) => text.textContent === "Frame"),
            frameRect: attrs(frameRect, ["x", "y", "width", "height"]),
        };
    }, {
        source,
        rootId: baseline.root,
        stickyId: sticky.id,
        shapeId: shape.id,
        connectorId: connector.id,
        inkId: ink.id,
        frameStroke: frameStrokeHex(frame),
    });

    expect(svg.parseError).toBeNull();
    expect(svg.name).toBe("svg");
    expect(svg.namespace).toBe("http://www.w3.org/2000/svg");
    expect(svg.viewBox).toHaveLength(4);
    expect(svg.viewBox.every(Number.isFinite)).toBe(true);
    expect(svg.viewBox[2]).toBeGreaterThan(0);
    expect(svg.viewBox[3]).toBeGreaterThan(0);
    expect(svg.hiddenRootPresent).toBe(false);
    expect(svg.stickyText).toBe("Export idea");
    expect(svg.cards).toEqual([{
        x: expect.any(String),
        y: expect.any(String),
        width: expect.any(String),
        height: expect.any(String),
        fill: "#FFEDD5",
    }]);
    expect(svg.shapeText).toBe("Export shape");
    expect(svg.shapePath).toMatchObject({ d: expect.stringMatching(/^M .+ Z$/), fill: "#FFEDD5" });
    expect(svg.connectorText).toBe("Export link");
    expect(svg.connectorPath).toMatchObject({
        d: expect.stringMatching(/^M .+ L /),
        fill: "none",
        stroke: expect.stringMatching(/^#2e7cf6$/i),
    });
    expect(svg.inkPath).toMatchObject({
        d: expect.stringMatching(/^M .+ L /),
        fill: "none",
        stroke: "#000000",
        "stroke-linecap": "round",
    });
    expect(svg.frameText).toBe(true);
    expect(svg.frameRect).toEqual({
        x: expect.any(String),
        y: expect.any(String),
        width: expect.any(String),
        height: expect.any(String),
    });
    expect(Number(svg.frameRect!.x)).toBeCloseTo(frame.frame.min.x, 2);
    expect(Number(svg.frameRect!.y)).toBeCloseTo(frame.frame.min.y, 2);
    expect(Number(svg.frameRect!.width)).toBeCloseTo(frame.frame.size.x, 2);
    expect(Number(svg.frameRect!.height)).toBeCloseTo(frame.frame.size.y, 2);

    const cardRect: BoardRect = {
        min: { x: Number(svg.cards[0]!.x), y: Number(svg.cards[0]!.y) },
        size: { x: Number(svg.cards[0]!.width), y: Number(svg.cards[0]!.height) },
    };
    expect(cardRect.size.x).toBeCloseTo(sticky.card!.size.x, 2);
    expect(cardRect.size.y).toBeCloseTo(sticky.card!.size.y, 2);
    for (const [name, rect] of [
        ["sticky", cardRect],
        ["shape", shape.rect],
        ["ink", ink.rect],
        ["frame", frame.frame],
    ] as const) {
        expect(rect.min.x, `${name} left`).toBeGreaterThanOrEqual(svg.viewBox[0] - 0.02);
        expect(rect.min.y, `${name} top`).toBeGreaterThanOrEqual(svg.viewBox[1] - 0.02);
        expect(rect.min.x + rect.size.x, `${name} right`)
            .toBeLessThanOrEqual(svg.viewBox[0] + svg.viewBox[2] + 0.02);
        expect(rect.min.y + rect.size.y, `${name} bottom`)
            .toBeLessThanOrEqual(svg.viewBox[1] + svg.viewBox[3] + 0.02);
    }

    // Decode the actual download as an image as well as parsing its XML: this
    // catches a syntactically valid SVG that cannot paint the styled objects.
    const raster = await page.evaluate(async (markup) => {
        const url = URL.createObjectURL(new Blob([markup], { type: "image/svg+xml" }));
        try {
            const image = new Image();
            image.src = url;
            await image.decode();
            const canvas = document.createElement("canvas");
            canvas.width = image.naturalWidth;
            canvas.height = image.naturalHeight;
            const context = canvas.getContext("2d");
            if (!context) throw new Error("Cannot render downloaded SVG");
            context.drawImage(image, 0, 0);
            const { data } = context.getImageData(0, 0, canvas.width, canvas.height);
            let styledPixels = 0;
            for (let i = 0; i < data.length; i += 4) {
                if (data[i] === 255 && data[i + 1] === 237 && data[i + 2] === 213 && data[i + 3] === 255) {
                    styledPixels++;
                }
            }
            return { width: canvas.width, height: canvas.height, styledPixels };
        } finally {
            URL.revokeObjectURL(url);
        }
    }, source);
    expect(raster.width).toBeGreaterThan(0);
    expect(raster.height).toBeGreaterThan(0);
    expect(raster.styledPixels).toBeGreaterThan(1_000);

    await setExportPrompt(page, "f6-rootless-board.png");
    await command(page, "Export PNG");
    await page.waitForTimeout(300);
    expect(await exportPrompts(page)).toEqual([]);
    const pendingPng = page.waitForEvent("download", { timeout: 30_000 });
    await confirmVisualPreflight(page);
    const pngDownload = await pendingPng;
    expect(pngDownload.suggestedFilename()).toBe("f6-rootless-board.png");
    expect(await exportPrompts(page)).toEqual([{
        message: "File name",
        suggested: expect.stringMatching(/\.png$/i),
    }]);
    const pngPath = testInfo.outputPath("downloaded-board.png");
    await pngDownload.saveAs(pngPath);
    const png = await readFile(pngPath);
    await testInfo.attach("downloaded PNG", { path: pngPath, contentType: "image/png" });
    expect(png.subarray(0, 8)).toEqual(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]));
    const rendered = await inspectRasterScene(page, png, frame, ink.kind.ink!.points, "png");
    expect(rendered.warnings).toHaveLength(2);
    expect(rendered.frame.count).toBeGreaterThan(500);
    expect(rendered.labelPixels.every((count) => count > 10)).toBe(true);
    expect(rendered.connectorPixels).toBeGreaterThan(30);
    expect(rendered.connectorHits).toEqual([true, true]);
    expect(rendered.inkHits).toEqual([true, true, true]);

    await setExportPrompt(page, "f6-rootless-board.jpg");
    await command(page, "Export JPEG");
    await page.waitForTimeout(300);
    expect(await exportPrompts(page)).toEqual([]);
    const pendingJpeg = page.waitForEvent("download", { timeout: 30_000 });
    await confirmVisualPreflight(page);
    const jpegDownload = await pendingJpeg;
    expect(jpegDownload.suggestedFilename()).toBe("f6-rootless-board.jpg");
    expect(await exportPrompts(page)).toEqual([{
        message: "File name",
        suggested: expect.stringMatching(/\.jpe?g$/i),
    }]);
    const jpegPath = testInfo.outputPath("downloaded-board.jpg");
    await jpegDownload.saveAs(jpegPath);
    const jpeg = await readFile(jpegPath);
    await testInfo.attach("downloaded JPEG", { path: jpegPath, contentType: "image/jpeg" });
    expect(jpeg.subarray(0, 3)).toEqual(Buffer.from([0xff, 0xd8, 0xff]));
    const renderedJpeg = await inspectRasterScene(page, jpeg, frame, ink.kind.ink!.points, "jpeg");
    expect(renderedJpeg.transparentPixels).toBe(0);
    expect(renderedJpeg.warnings).toHaveLength(2);
    expect(renderedJpeg.frame.count).toBeGreaterThan(500);
    expect(renderedJpeg.labelPixels.every((count) => count > 10)).toBe(true);
    expect(renderedJpeg.connectorPixels).toBeGreaterThan(30);
    expect(renderedJpeg.connectorHits).toEqual([true, true]);
    expect(renderedJpeg.inkHits).toEqual([true, true, true]);

    expect(sheetFrom(await saveAs(page, "f6-after-export.thinoda"))).toEqual(baseline);
});

function inspectVectorPdf(bytes: Buffer) {
    const pdf = bytes.toString("latin1");
    expect(bytes.length).toBeGreaterThan(1_000);
    expect(pdf).toMatch(/^%PDF-\d\.\d/);
    const trailer = /startxref\s+(\d+)\s+%%EOF\s*$/.exec(pdf);
    expect(trailer).not.toBeNull();
    if (!trailer) throw new Error("Missing PDF trailer");
    const xref = Number(trailer[1]);
    expect(xref).toBeGreaterThan(0);
    expect(xref).toBeLessThan(bytes.length);
    expect(pdf.slice(xref, xref + 40)).toMatch(/^(?:xref\b|\d+\s+\d+\s+obj\b)/);

    expect(pdf).toMatch(/\/Type\s*\/Catalog\b/);
    expect(pdf).toMatch(/\/Count\s+1\b/);
    const pages = Array.from(pdf.matchAll(/\/Type\s*\/Page\b/g));
    expect(pages).toHaveLength(1);
    const pageStart = pages[0]!.index!;
    const pageEnd = pdf.indexOf("endobj", pageStart);
    expect(pageEnd).toBeGreaterThan(pageStart);
    const page = pdf.slice(pageStart, pageEnd);
    const mediaBox = /\/MediaBox\s*\[([^\]]+)\]/.exec(page)?.[1]
        .trim().split(/\s+/).map(Number);
    expect(mediaBox).toHaveLength(4);
    if (!mediaBox || mediaBox.length !== 4) throw new Error("Missing PDF MediaBox");
    expect(mediaBox.every(Number.isFinite)).toBe(true);
    const width = mediaBox[2] - mediaBox[0];
    const height = mediaBox[3] - mediaBox[1];
    expect(width).toBeGreaterThan(0);
    expect(height).toBeGreaterThan(0);

    // This board contains no images. Inspect the page's actual content stream
    // as well as ruling out image XObjects, so a raster-only PDF cannot pass.
    expect(pdf).not.toMatch(/\/Subtype\s*\/Image\b/);
    const contents = /\/Contents\s+(\d+)\s+(\d+)\s+R\b/.exec(page);
    expect(contents).not.toBeNull();
    if (!contents) throw new Error("Missing PDF page contents");
    const stream = new RegExp(
        `(?:^|\\r?\\n)${contents[1]}\\s+${contents[2]}\\s+obj\\s*<<([\\s\\S]*?)>>\\s*stream\\r?\\n`,
    ).exec(pdf);
    expect(stream).not.toBeNull();
    if (!stream) throw new Error("Missing PDF content stream");
    const length = /\/Length\s+(\d+)\b/.exec(stream[1]);
    expect(length).not.toBeNull();
    if (!length) throw new Error("Missing PDF stream length");
    const start = stream.index + stream[0].length;
    const end = start + Number(length[1]);
    expect(end).toBeLessThanOrEqual(bytes.length);
    const encoded = bytes.subarray(start, end);
    const filtered = /\/Filter\b/.test(stream[1]);
    if (filtered && !/\/FlateDecode\b/.test(stream[1])) {
        throw new Error("Unsupported PDF content stream filter");
    }
    const commands = (filtered ? inflateSync(encoded) : encoded).toString("latin1");
    expect(commands).toMatch(/(?:^|\s)(?:m|l|c|re)(?=\s|$)/);
    expect(commands).toMatch(/(?:^|\s)(?:f|f\*|B|B\*|b|b\*)(?=\s|$)/);
    expect(commands).toMatch(/(?:^|\s)(?:S|s|B|B\*)(?=\s|$)/);
    return { width, height };
}

async function inspectRasterScene(
    page: Page,
    screenshot: Buffer,
    zone: { frame: BoardRect; color?: string },
    points: { x: number; y: number }[],
    rasterFormat: "viewer" | "png" | "jpeg" = "viewer",
) {
    const frame = zone.frame;
    const tint = frameTint(zone);
    return page.evaluate(async ({ screenshot, frame, points, rasterFormat, tint }) => {
        const image = new Image();
        image.src = `data:image/${rasterFormat === "jpeg" ? "jpeg" : "png"};base64,${screenshot}`;
        await image.decode();
        const canvas = document.createElement("canvas");
        canvas.width = image.naturalWidth;
        canvas.height = image.naturalHeight;
        const context = canvas.getContext("2d");
        if (!context) throw new Error("Cannot inspect the rendered scene");
        context.drawImage(image, 0, 0);
        const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
        const emptyBounds = () => ({
            minX: canvas.width, minY: canvas.height, maxX: -1, maxY: -1, count: 0,
        });
        const include = (bounds: ReturnType<typeof emptyBounds>, x: number, y: number) => {
            bounds.minX = Math.min(bounds.minX, x);
            bounds.minY = Math.min(bounds.minY, y);
            bounds.maxX = Math.max(bounds.maxX, x);
            bounds.maxY = Math.max(bounds.maxY, y);
            bounds.count++;
        };
        const paper = emptyBounds();
        const framePixels = emptyBounds();
        const columns = Array.from({ length: canvas.width }, emptyBounds);
        let transparentPixels = 0;
        const jpegExport = rasterFormat === "jpeg";
        const frameRows = new Uint32Array(canvas.height);
        const frameColumns = new Uint32Array(canvas.width);
        const near = (value: number, expected: number) =>
            Math.abs(value - expected) <= (jpegExport ? 8 : 0);
        // Rasterizers may round the translucent frame tint one unit differently.
        const nearTint = (value: number, expected: number) =>
            Math.abs(value - expected) <= (jpegExport ? 8 : 1);
        for (let y = 0; y < canvas.height; y++) {
            for (let x = 0; x < canvas.width; x++) {
                const i = (y * canvas.width + x) * 4;
                const [r, g, b] = [pixels[i], pixels[i + 1], pixels[i + 2]];
                if (pixels[i + 3] !== 255) transparentPixels++;
                if (near(r, 255) && near(g, 255) && near(b, 255)) include(paper, x, y);
                if (nearTint(r, tint[0]) && nearTint(g, tint[1]) && nearTint(b, tint[2])) {
                    include(framePixels, x, y);
                    frameRows[y]++;
                    frameColumns[x]++;
                }
                if (near(r, 255) && near(g, 237) && near(b, 213)) include(columns[x], x, y);
            }
        }
        if (framePixels.count) {
            // Compression and antialiased strokes in the frame's hue create
            // isolated tint-coloured pixels outside the frame.
            const denseExtent = (counts: Uint32Array) => {
                const cutoff = Math.max(1, Math.floor(counts.reduce((max, value) => Math.max(max, value), 0) * 0.6));
                let first = -1;
                let last = -1;
                for (let i = 0; i < counts.length; i++) {
                    if (counts[i] >= cutoff) {
                        if (first < 0) first = i;
                        last = i;
                    }
                }
                if (first < 0) throw new Error("Cannot locate the exported JPEG frame");
                return [first, last];
            };
            [framePixels.minX, framePixels.maxX] = denseExtent(frameColumns);
            [framePixels.minY, framePixels.maxY] = denseExtent(frameRows);
        }
        const warnings: ReturnType<typeof emptyBounds>[] = [];
        let run: ReturnType<typeof emptyBounds> | null = null;
        for (const column of columns) {
            if (column.count) {
                if (!run) run = emptyBounds();
                run.minX = Math.min(run.minX, column.minX);
                run.minY = Math.min(run.minY, column.minY);
                run.maxX = Math.max(run.maxX, column.maxX);
                run.maxY = Math.max(run.maxY, column.maxY);
                run.count += column.count;
            } else if (run) {
                if (run.count > 500) warnings.push(run);
                run = null;
            }
        }
        if (run && run.count > 500) warnings.push(run);
        const dark = (x: number, y: number) => {
            const i = (y * canvas.width + x) * 4;
            return pixels[i] < 80 && pixels[i + 1] < 80 && pixels[i + 2] < 80;
        };
        const automaticConnector = (x: number, y: number) => {
            const i = (y * canvas.width + x) * 4;
            const [r, g, b] = [pixels[i], pixels[i + 1], pixels[i + 2]];
            const tolerance = jpegExport ? 85 : 20;
            return Math.abs(r - 46) <= tolerance && Math.abs(g - 124) <= tolerance
                && Math.abs(b - 246) <= tolerance && b - r >= 80 && b - g >= 40;
        };
        const labelPixels = warnings.map((bounds) => {
            let count = 0;
            for (let y = bounds.minY; y <= bounds.maxY; y++) {
                for (let x = bounds.minX; x <= bounds.maxX; x++) {
                    const i = (y * canvas.width + x) * 4;
                    if (pixels[i] >= 80 && pixels[i] <= 160
                        && pixels[i + 1] < 80 && pixels[i + 2] < 80) count++;
                }
            }
            return count;
        });
        let connectorPixels = 0;
        if (warnings.length === 2) {
            for (let y = Math.max(warnings[0].minY, warnings[1].minY);
                y <= Math.min(warnings[0].maxY, warnings[1].maxY); y++) {
                for (let x = warnings[0].maxX + 1; x < warnings[1].minX; x++) {
                    if (automaticConnector(x, y)) connectorPixels++;
                }
            }
        }
        const connectorHits = [0.2, 0.8].map((t) => {
            if (warnings.length !== 2) return false;
            const x = Math.round(warnings[0].maxX + t * (warnings[1].minX - warnings[0].maxX));
            const y = Math.round((warnings[0].minY + warnings[0].maxY
                + warnings[1].minY + warnings[1].maxY) / 4);
            for (let dy = -4; dy <= 4; dy++) {
                for (let dx = -4; dx <= 4; dx++) {
                    if (x + dx < 0 || x + dx >= canvas.width || y + dy < 0 || y + dy >= canvas.height) {
                        continue;
                    }
                    if (automaticConnector(x + dx, y + dy)) return true;
                }
            }
            return false;
        });
        const inkHits = [0.2, 0.5, 0.8].map((t) => {
            if (!framePixels.count || points.length < 2) return false;
            const first = points[0];
            const last = points[points.length - 1];
            const x = Math.round(framePixels.minX + (first.x + t * (last.x - first.x) - frame.min.x)
                / frame.size.x * (framePixels.maxX - framePixels.minX + 1));
            const y = Math.round(framePixels.minY + (first.y + t * (last.y - first.y) - frame.min.y)
                / frame.size.y * (framePixels.maxY - framePixels.minY + 1));
            for (let dy = -4; dy <= 4; dy++) {
                for (let dx = -4; dx <= 4; dx++) {
                    if (x + dx >= 0 && x + dx < canvas.width && y + dy >= 0
                        && y + dy < canvas.height && dark(x + dx, y + dy)) return true;
                }
            }
            return false;
        });
        return {
            paper, frame: framePixels, warnings, labelPixels, connectorPixels,
            connectorHits, inkHits, transparentPixels,
        };
    }, { screenshot: screenshot.toString("base64"), frame, points, rasterFormat, tint });
}

function frameStrokeHex(zone: { color?: string }) {
    return (zone.color ?? "#5D7A91").slice(0, 7).toUpperCase();
}

// Exported frames use their stroke colour at alpha 0x14 over the white paper.
function frameTint(zone: { color?: string }): [number, number, number] {
    const hex = frameStrokeHex(zone);
    return [1, 3, 5].map((i) => {
        const channel = parseInt(hex.slice(i, i + 2), 16);
        return Math.round(255 - (255 - channel) * 0x14 / 255);
    }) as [number, number, number];
}

async function renderedPdfPixels(
    page: Page,
    viewer: Page,
    zone: { frame: BoardRect; color?: string },
    points: { x: number; y: number }[],
) {
    return inspectRasterScene(page, await viewer.screenshot(), zone, points);
}

test.describe("PDF export", () => {
    test("downloads a framed vector PDF without changing a mixed rootless board", async ({ page }) => {
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
        await expect(page.locator("input[type='text']")).toBeFocused();
        await page.locator("input[type='text']").pressSequentially("Export idea");
        await page.keyboard.press("Enter");
        await frames(page);
        await command(page, "Quick Style: Warning");

        await command(page, "Shape tool");
        await page.mouse.click(650, 400);
        await frames(page);
        await command(page, "Quick Style: Warning");
        await page.locator("#thinoda_canvas").focus();
        await page.keyboard.press("F2");
        await frames(page);
        await expect(page.locator("input[type='text']")).toBeFocused();
        await page.locator("input[type='text']").pressSequentially("Export shape");
        await page.keyboard.press("Enter");
        await frames(page);

        await command(page, "Connector tool");
        await drag(page, [260, 400], [610, 400]);
        await page.locator("#thinoda_canvas").focus();
        await page.keyboard.press("F2");
        await frames(page);
        await expect(page.locator("input[type='text']")).toBeFocused();
        await page.locator("input[type='text']").pressSequentially("Export link");
        await page.keyboard.press("Enter");
        await frames(page);

        await command(page, "Frame tool");
        await drag(page, [300, 750], [850, 1000]);
        await command(page, "Pen");
        await drag(page, [450, 850], [600, 900], 12);
        await command(page, "Line grid");

        const baseline = sheetFrom(await saveAs(page, "f6-pdf-before.thinoda"));
        expect(baseline.root_mode).toBe("hidden");
        expect(baseline.board.grid).toBe("lines");
        const sticky = Object.values(baseline.topics).find((topic) => topic.card?.kind === "sticky");
        const shape = baseline.canvas.objects.find((object) => object.kind.shape);
        const connector = baseline.canvas.objects.find((object) => object.kind.connector);
        const ink = baseline.canvas.objects.find((object) => object.kind.ink);
        const frame = baseline.zones.find((zone) => zone.keep_empty);
        if (!sticky?.card || !sticky.position || !shape?.kind.shape
            || !connector?.kind.connector || !ink?.kind.ink || !frame) {
            throw new Error("Incomplete synthetic PDF board");
        }
        expect(sticky.title).toBe("Export idea");
        expect(sticky.card.color).toBe("#FFEDD5");
        expect(baseline.floating).toContain(sticky.id);
        expect(shape.kind.shape).toMatchObject({ shape: "rectangle", label: "Export shape" });
        expect(shape.style.fill).toBe("#FFEDD5");
        expect(connector.kind.connector).toMatchObject({
            from: { topic: sticky.id },
            to: { object: shape.id },
            label: "Export link",
        });
        expect(ink.kind.ink.points.length).toBeGreaterThan(1);
        expect(frame.name).toBe("Frame");

        await setExportPrompt(page, "f6-rootless-board.pdf");
        // "PDF" alone also matches FreeMind / Freeplane in the fuzzy palette.
        await command(page, "Export PDF");
        await page.waitForTimeout(300);
        expect(await exportPrompts(page)).toEqual([]);
        const pending = page.waitForEvent("download", { timeout: 30_000 });
        await confirmVisualPreflight(page); // PDF uses the same grid-loss dialog.
        const download = await pending;
        expect(download.suggestedFilename()).toBe("f6-rootless-board.pdf");
        expect(await exportPrompts(page)).toEqual([{
            message: "File name",
            suggested: expect.stringMatching(/\.pdf$/i),
        }]);
        const bytes = await readFile(await download.path());
        const pdf = inspectVectorPdf(bytes);
        expect(sheetFrom(await saveAs(page, "f6-pdf-after.thinoda"))).toEqual(baseline);

        const stickyRect: BoardRect = {
            min: {
                x: sticky.position.x - sticky.card.size.x / 2,
                y: sticky.position.y - sticky.card.size.y / 2,
            },
            size: sticky.card.size,
        };
        const knownRects = [stickyRect, shape.rect, ink.rect, frame.frame];
        const spanX = Math.max(...knownRects.map((r) => r.min.x + r.size.x))
            - Math.min(...knownRects.map((r) => r.min.x));
        const spanY = Math.max(...knownRects.map((r) => r.min.y + r.size.y))
            - Math.min(...knownRects.map((r) => r.min.y));
        expect(pdf.width).toBeGreaterThanOrEqual(spanX - 0.1);
        expect(pdf.height).toBeGreaterThanOrEqual(spanY - 0.1);
        expect(pdf.width).toBeLessThan(spanX + 600);
        expect(pdf.height).toBeLessThan(spanY + 600);

        await withPdfViewer(bytes, async (viewer) => {
            await expect.poll(async () => {
                const image = await renderedPdfPixels(page, viewer, frame, ink.kind.ink!.points);
                return image.warnings.length === 2 && image.frame.count > 500;
            }, { timeout: 30_000, intervals: [500, 1_000, 2_000] }).toBe(true);
            const image = await renderedPdfPixels(page, viewer, frame, ink.kind.ink.points);
            expect(image.paper.count).toBeGreaterThan(1_000);
            expect(image.warnings).toHaveLength(2);
            expect(image.labelPixels.every((count) => count > 10)).toBe(true);
            expect(image.connectorPixels).toBeGreaterThan(30);
            expect(image.connectorHits).toEqual([true, true]);
            expect(image.inkHits).toEqual([true, true, true]);

            const paperWidth = image.paper.maxX - image.paper.minX + 1;
            const paperHeight = image.paper.maxY - image.paper.minY + 1;
            for (const [pixels, rect] of [
                [image.warnings[0], stickyRect],
                [image.warnings[1], shape.rect],
                [image.frame, frame.frame],
            ] as const) {
                expect(pixels.minX).toBeGreaterThan(image.paper.minX);
                expect(pixels.minY).toBeGreaterThan(image.paper.minY);
                expect(pixels.maxX).toBeLessThan(image.paper.maxX);
                expect(pixels.maxY).toBeLessThan(image.paper.maxY);
                expect(Math.abs((pixels.maxX - pixels.minX + 1) / paperWidth - rect.size.x / pdf.width))
                    .toBeLessThan(0.02);
                expect(Math.abs((pixels.maxY - pixels.minY + 1) / paperHeight - rect.size.y / pdf.height))
                    .toBeLessThan(0.02);
            }
        });
    });
});
