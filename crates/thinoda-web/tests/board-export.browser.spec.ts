/**
 * Production-browser coverage for exporting a mixed rootless board.
 * The document is created in the test's isolated browser context, and the
 * downloaded SVG is inspected rather than inferred from the canvas.
 */
import { expect, test, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { command, frames, saveAs, zipEntry } from "./board-browser-helpers";

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
        card?: { kind: string; color?: string; size: { x: number; y: number } };
    }>;
    zones: {
        name: string;
        keep_empty?: boolean;
        frame: BoardRect;
    }[];
    canvas: {
        objects: {
            id: number;
            kind: {
                shape?: { shape: string; label?: string };
                connector?: { from: unknown; to: unknown; label?: string };
                ink?: { points: unknown[] };
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

async function confirmSvgPreflight(page: Page) {
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

test("mixed rootless board SVG export requires consent and downloads its objects", async ({ page }) => {
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
    await page.mouse.move(260, 400);
    await page.mouse.down();
    await page.mouse.move(610, 400, { steps: 8 });
    await page.mouse.up();
    await frames(page);
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("F2");
    await frames(page);
    await expect(page.locator("input[type='text']")).toBeFocused();
    await page.locator("input[type='text']").pressSequentially("Export link");
    await page.keyboard.press("Enter");
    await frames(page);

    await command(page, "Frame tool");
    await page.mouse.move(300, 750);
    await page.mouse.down();
    await page.mouse.move(850, 1000, { steps: 8 });
    await page.mouse.up();
    await frames(page);
    await command(page, "Pen");
    await page.mouse.move(450, 850);
    await page.mouse.down();
    await page.mouse.move(600, 900, { steps: 12 });
    await page.mouse.up();
    await frames(page);
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
    await confirmSvgPreflight(page);
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
    await confirmSvgPreflight(page);
    const download = await pending;
    expect(download.suggestedFilename()).toBe("f6-rootless-board.svg");
    expect(await exportPrompts(page)).toHaveLength(1);
    expect(downloads).toEqual(["f6-after-cancel.thinoda", "f6-rootless-board.svg"]);

    const source = (await readFile(await download.path())).toString("utf8");
    const svg = await page.evaluate(({ source, rootId, stickyId, shapeId, connectorId, inkId }) => {
        const doc = new DOMParser().parseFromString(source, "image/svg+xml");
        const root = doc.documentElement;
        const group = (id: number) => doc.getElementById(`object-${id}`);
        const attrs = (element: Element | null, names: string[]) => element
            ? Object.fromEntries(names.map((name) => [name, element.getAttribute(name)]))
            : null;
        const frameRect = Array.from(doc.querySelectorAll("rect"))
            .find((rect) => rect.getAttribute("stroke") === "#5D7A91") ?? null;
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
        stroke: expect.stringMatching(/^#[0-9a-f]{6}([0-9a-f]{2})?$/i),
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

    expect(sheetFrom(await saveAs(page, "f6-after-export.thinoda"))).toEqual(baseline);
});
