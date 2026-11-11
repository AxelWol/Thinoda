/**
 * Browser transport coverage using an isolated, in-page clipboard.
 * The synthetic events never read or replace the system clipboard.
 */
import { expect, test, type Page } from "@playwright/test";
import { colorBounds, command, drag, frames, imagePicker, jpeg, saveAs, zipEntry } from "./board-browser-helpers";

test.use({
    viewport: { width: 1252, height: 1278 },
    deviceScaleFactor: 1,
    serviceWorkers: "block",
    screenshot: "only-on-failure",
});
test.setTimeout(180_000);

type TestClipboard = {
    items: ClipboardItem[];
    writes: number;
    writeAttempts: number;
    plainWrites: number;
    plainAttempts: number;
    reads: number;
    textReads: number;
    oversizedBlobReads: number;
    rejectWrites: boolean;
    rejectSvgWrites: boolean;
    rejectPlainWrites: boolean;
    svgSupported: boolean;
    readMode: "normal" | "wrapped" | "missing-html" | "sanitized" | "changed-rich" |
        "changed-text" | "malformed" | "denied" | "missing-read" | "oversized-text" |
        "oversized-html";
    holdWrite: boolean;
    releaseWrite: (() => void) | null;
    holdRead: boolean;
    releaseRead: (() => void) | null;
};

declare global {
    interface Window {
        __thinodaTestClipboard: TestClipboard;
    }
}

type Topic = {
    id: number;
    title: string;
    children?: number[];
    style?: { bold?: boolean; [key: string]: unknown };
    image?: string;
    card?: { kind: string; color?: string };
    inline_text?: {
        runs: { start: number; end: number; style: { bold: boolean } }[];
        paragraphs: { index: number; list: string }[];
    };
};

type BoardObject = {
    id: number;
    kind: {
        shape?: { shape: string; label?: string };
        connector?: { from: unknown; to: unknown; label?: string; waypoints?: { x: number; y: number }[] };
    };
    style: { fill?: string; stroke?: string; text_color?: string };
    rectangle_sides?: { top: boolean; right: boolean; bottom: boolean; left: boolean };
    double_ellipse_margin?: number;
};

type BoardSheet = {
    root: number;
    root_mode: string;
    floating: number[];
    topics: Record<string, Topic>;
    canvas?: { objects: BoardObject[] };
};

type Resource = { id: string; name: string; size: number };

function archive(zip: Buffer) {
    return {
        zip,
        content: JSON.parse(zipEntry(zip, "content.json").toString("utf8")) as {
            sheets: BoardSheet[];
            active_sheet: number;
        },
        manifest: JSON.parse(zipEntry(zip, "manifest.json").toString("utf8")) as {
            version: number; resources: Resource[];
        },
    };
}

async function openBoard(page: Page) {
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+n");
    await frames(page);
    await command(page, "Hide central topic");
    await page.keyboard.press("Escape");
    await frames(page);
}

async function createShape(page: Page, title: string) {
    await command(page, "Shape tool");
    await page.mouse.click(650, 400);
    await frames(page);
    await command(page, "Quick Style: Warning");
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("F2");
    await frames(page);
    await expect(page.locator("input[type='text']")).toBeFocused();
    await page.locator("input[type='text']").pressSequentially(title);
    await page.keyboard.press("Enter");
    await frames(page);
}

async function formattedCardBoard(page: Page) {
    await command(page, "Sticky note");
    await page.mouse.click(220, 400);
    await frames(page);
    const input = page.locator("input[type='text']");
    await expect(input).toBeFocused();
    await input.fill("Styled card");
    await page.keyboard.press("Enter");
    await frames(page);

    const original = archive(await saveAs(page, "clipboard-unformatted.thinoda"));
    const sheet = original.content.sheets[0];
    const card = Object.values(sheet.topics).find((topic) => topic.title === "Styled card");
    if (!card) throw new Error("Missing card for rich clipboard fixture");
    card.inline_text = {
        runs: [{ start: 0, end: 6, style: { bold: true } }],
        paragraphs: [{ index: 0, list: "bulleted" }],
    };
    const encoded = `THINODA-SHEET-5\n${JSON.stringify(sheet)}`;
    await page.locator("#thinoda_canvas").focus();
    await page.evaluate((text) => {
        const data = new DataTransfer();
        data.setData("text/plain", text);
        const event = new ClipboardEvent("paste", {
            bubbles: true, cancelable: true, clipboardData: data,
        });
        Object.defineProperty(event, "clipboardData", { value: data });
        document.dispatchEvent(event);
    }, encoded);
    await frames(page);
    await page.waitForTimeout(300);
    const formatted = archive(await saveAs(page, "clipboard-formatted.thinoda"));
    expect(formatted.content.sheets).toHaveLength(2);
    expect(formatted.content.active_sheet).toBe(1);
    expect(formatted.content.sheets[1].topics[String(card.id)].inline_text).toEqual(card.inline_text);
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+a");
    await frames(page);
    return { formatted, card };
}

async function copyOrCut(page: Page, kind: "copy" | "cut") {
    const previous = await page.evaluate(() => window.__thinodaTestClipboard.writeAttempts);
    await page.locator("#thinoda_canvas").focus();
    const prevented = await page.evaluate((kind) => {
        const event = new ClipboardEvent(kind, {
            bubbles: true, cancelable: true, clipboardData: new DataTransfer(),
        });
        document.dispatchEvent(event);
        return event.defaultPrevented;
    }, kind);
    expect(prevented).toBe(true);
    await expect.poll(() => page.evaluate(() => window.__thinodaTestClipboard.writeAttempts))
        .toBe(previous + 1);
    await frames(page);
    await page.waitForTimeout(300);
}

async function paste(page: Page) {
    const previous = await page.evaluate(() => window.__thinodaTestClipboard.reads);
    await page.locator("#thinoda_canvas").focus();
    const prevented = await page.evaluate(async () => {
        const item = window.__thinodaTestClipboard.items[0];
        const text = await (await item.getType("text/plain")).text();
        const data = new DataTransfer();
        data.setData("text/plain", text);
        const event = new ClipboardEvent("paste", {
            bubbles: true, cancelable: true, clipboardData: data,
        });
        // Firefox otherwise exposes an empty DataTransfer on synthetic paste.
        Object.defineProperty(event, "clipboardData", { value: data });
        document.dispatchEvent(event);
        return event.defaultPrevented;
    });
    expect(prevented).toBe(true);
    await expect.poll(() => page.evaluate(() => window.__thinodaTestClipboard.reads))
        .toBe(previous + 1);
    await frames(page);
    await page.waitForTimeout(300);
}

async function expectWarningToast(page: Page, path: string) {
    const viewport = page.viewportSize();
    if (!viewport) throw new Error("Cannot inspect the warning without a viewport");
    // Save notifications can stack below the warning; exclude canvas and inspector colors.
    const width = Math.floor(viewport.width * 0.75);
    const height = Math.min(320, viewport.height);
    const screenshot = await page.screenshot({
        path,
        clip: {
            x: 0,
            y: viewport.height - height,
            width,
            height,
        },
    });
    const warning = (await colorBounds(page, screenshot, [[224, 142, 0]], 30))[0];
    expect(warning.count).toBeGreaterThan(5);
}

test.beforeEach(async ({ page, context }) => {
    await context.addInitScript(() => {
        const state: TestClipboard = {
            items: [], writes: 0, writeAttempts: 0, plainWrites: 0, plainAttempts: 0,
            reads: 0, textReads: 0, oversizedBlobReads: 0, rejectWrites: false,
            rejectSvgWrites: false, rejectPlainWrites: false, svgSupported: true, readMode: "normal",
            holdWrite: false, releaseWrite: null, holdRead: false, releaseRead: null,
        };
        Object.defineProperty(ClipboardItem, "supports", {
            configurable: true,
            value: (type: string) => type === "image/svg+xml" && state.svgSupported,
        });
        Object.defineProperty(navigator, "clipboard", {
            configurable: true,
            value: {
                async write(items: ClipboardItem[]) {
                    state.writeAttempts++;
                    if (state.holdWrite) {
                        await new Promise<void>((resolve) => { state.releaseWrite = resolve; });
                        state.releaseWrite = null;
                    }
                    if (state.rejectWrites || (state.rejectSvgWrites && items[0].types.includes("image/svg+xml"))) {
                        throw new DOMException("Clipboard denied", "NotAllowedError");
                    }
                    state.items = items;
                    state.writes++;
                },
                async writeText(text: string) {
                    state.plainAttempts++;
                    if (state.rejectPlainWrites) throw new DOMException("Text clipboard denied", "NotAllowedError");
                    state.items = [new ClipboardItem({ "text/plain": new Blob([text], { type: "text/plain" }) })];
                    state.plainWrites++;
                },
                async read() {
                    state.reads++;
                    if (state.holdRead) {
                        await new Promise<void>((resolve) => { state.releaseRead = resolve; });
                        state.releaseRead = null;
                    }
                    if (state.readMode === "denied") {
                        throw new DOMException("Clipboard read denied", "NotAllowedError");
                    }
                    if (state.readMode === "normal") return state.items;
                    const item = state.items[0];
                    if (!item) return [];
                    const text = await item.getType("text/plain");
                    if (state.readMode === "missing-html") {
                        return [new ClipboardItem({ "text/plain": text })];
                    }
                    const html = await (await item.getType("text/html")).text();
                    const changed = state.readMode === "sanitized" ? "<pre>sanitized</pre>"
                        : state.readMode === "changed-rich" ? "<pre data-thinoda-board-v1='6f74686572'></pre>"
                        : state.readMode === "malformed" ? "<pre data-thinoda-board-v1='xz'></pre>"
                        : state.readMode === "wrapped" ? `<html><body>${html}</body></html>`
                        : html;
                    const result = new ClipboardItem({
                        "text/plain": state.readMode === "changed-text"
                            ? new Blob(["different text"], { type: "text/plain" }) : text,
                        "text/html": new Blob([changed], { type: "text/html" }),
                    });
                    if (state.readMode === "oversized-text" || state.readMode === "oversized-html") {
                        const oversizedType = state.readMode === "oversized-text" ? "text/plain" : "text/html";
                        const limit = state.readMode === "oversized-text"
                            ? 16 * 1024 * 1024 : 7 * 16 * 1024 * 1024 + 1024;
                        const getType = result.getType.bind(result);
                        Object.defineProperty(result, "getType", {
                            value: async (type: string) => {
                                const blob = await getType(type);
                                if (type === oversizedType) {
                                    Object.defineProperty(blob, "size", { value: limit + 1 });
                                    Object.defineProperty(blob, "text", {
                                        value: async () => {
                                            state.oversizedBlobReads++;
                                            throw new Error("Oversized blob must not be read");
                                        },
                                    });
                                }
                                return blob;
                            },
                        });
                    }
                    return [result];
                },
                async readText() {
                    state.textReads++;
                    const item = state.items[0];
                    return item ? (await item.getType("text/plain")).text() : "";
                },
            },
        });
        window.__thinodaTestClipboard = state;
    });
    await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    await openBoard(page);
});

test("sheet copy provides matching editable and SVG formats and remains pasteable", async ({ page }) => {
    await createShape(page, "Sheet SVG copy");
    const before = archive(await saveAs(page, "sheet-copy-before.thinoda"));
    await command(page, "Copy Sheet");
    await expect.poll(() => page.evaluate(() => window.__thinodaTestClipboard.writes)).toBe(1);
    const clipboard = await page.evaluate(async () => {
        const item = window.__thinodaTestClipboard.items[0];
        return {
            types: item.types,
            text: await (await item.getType("text/plain")).text(),
            svg: await (await item.getType("image/svg+xml")).text(),
        };
    });
    expect(clipboard.types).toEqual(expect.arrayContaining(["text/plain", "image/svg+xml"]));
    expect(clipboard.text).toMatch(/^THINODA-SHEET-\d+\n/);
    expect(JSON.parse(clipboard.text.slice(clipboard.text.indexOf("\n") + 1)))
        .toEqual(before.content.sheets[before.content.active_sheet]);
    expect(clipboard.svg).toContain("<svg");
    expect(clipboard.svg).toContain("Sheet SVG copy");

    await page.evaluate((text) => {
        const data = new DataTransfer();
        data.setData("text/plain", text);
        const event = new ClipboardEvent("paste", { bubbles: true, cancelable: true, clipboardData: data });
        Object.defineProperty(event, "clipboardData", { value: data });
        document.dispatchEvent(event);
    }, clipboard.text);
    await frames(page);
    const after = archive(await saveAs(page, "sheet-copy-pasted.thinoda"));
    expect(after.content.sheets).toHaveLength(before.content.sheets.length + 1);
    expect(after.content.sheets[after.content.active_sheet])
        .toEqual(before.content.sheets[before.content.active_sheet]);
});

for (const mode of ["unsupported", "rejected"] as const) {
    test(`sheet copy falls back to text when SVG is ${mode}`, async ({ page }) => {
        await createShape(page, "Text fallback");
        await page.evaluate((mode) => {
            const state = window.__thinodaTestClipboard;
            state.svgSupported = mode !== "unsupported";
            state.rejectSvgWrites = mode === "rejected";
        }, mode);
        await command(page, "Copy Sheet");
        await expect.poll(() => page.evaluate(() => window.__thinodaTestClipboard.plainWrites)).toBe(1);
        const clipboard = await page.evaluate(async () => {
            const item = window.__thinodaTestClipboard.items[0];
            return { types: item.types, text: await (await item.getType("text/plain")).text() };
        });
        expect(clipboard.types).toEqual(["text/plain"]);
        expect(clipboard.text).toMatch(/^THINODA-SHEET-\d+\n/);
    });
}

test("sheet copy keeps the previous clipboard when both writes are denied", async ({ page }) => {
    await page.evaluate(() => {
        const state = window.__thinodaTestClipboard;
        state.items = [new ClipboardItem({
            "text/plain": new Blob(["existing clipboard"], { type: "text/plain" }),
        })];
        state.rejectSvgWrites = true;
        state.rejectPlainWrites = true;
    });
    await command(page, "Copy Sheet");
    await expect.poll(() => page.evaluate(() => window.__thinodaTestClipboard.plainAttempts)).toBe(1);
    const text = await page.evaluate(async () =>
        (await window.__thinodaTestClipboard.items[0].getType("text/plain")).text());
    expect(text).toBe("existing clipboard");
});

test("an older failed sheet copy cannot overwrite a newer board copy", async ({ page }) => {
    await createShape(page, "Newer board copy");
    await page.evaluate(() => {
        const state = window.__thinodaTestClipboard;
        state.holdWrite = true;
        state.rejectSvgWrites = true;
    });
    await command(page, "Copy Sheet");
    await expect.poll(() => page.evaluate(() => window.__thinodaTestClipboard.releaseWrite !== null))
        .toBe(true);
    await page.evaluate(() => { window.__thinodaTestClipboard.holdWrite = false; });
    await copyOrCut(page, "copy");
    await page.evaluate(() => { window.__thinodaTestClipboard.releaseWrite?.(); });
    await frames(page);
    await page.waitForTimeout(300);
    const state = await page.evaluate(async () => ({
        plainAttempts: window.__thinodaTestClipboard.plainAttempts,
        types: window.__thinodaTestClipboard.items[0].types,
        text: await (await window.__thinodaTestClipboard.items[0].getType("text/plain")).text(),
    }));
    expect(state.plainAttempts).toBe(0);
    expect(state.types).toEqual(expect.arrayContaining(["text/plain", "text/html"]));
    expect(state.text).toContain("Newer board copy");
});

test("rich mixed-board clipboard remaps connections and carries images across documents", async ({ page }) => {
    await command(page, "Sticky note");
    await page.mouse.click(220, 400);
    await frames(page);
    await expect(page.locator("input[type='text']")).toBeFocused();
    await page.locator("input[type='text']").pressSequentially("Clipboard idea");
    await page.keyboard.press("Enter");
    await frames(page);
    await command(page, "Quick Style: Warning");
    await createShape(page, "Clipboard shape");
    await command(page, "Connector tool");
    await drag(page, [260, 400], [610, 400]);

    const image = await jpeg(page, 64, 0.9);
    await imagePicker(page, { name: "clipboard-image.jpg", mimeType: "image/jpeg", buffer: image });
    await page.waitForTimeout(500);
    const source = archive(await saveAs(page, "clipboard-source.thinoda"));
    const sourceSheet = source.content.sheets[0];
    const sticky = Object.values(sourceSheet.topics).find((topic) => topic.title === "Clipboard idea");
    const imageCard = Object.values(sourceSheet.topics).find((topic) => topic.card?.kind === "image");
    const shape = sourceSheet.canvas?.objects.find((object) => object.kind.shape?.label === "Clipboard shape");
    const connector = sourceSheet.canvas?.objects.find((object) => object.kind.connector);
    if (!sticky || !imageCard || !shape || !connector) throw new Error("Incomplete clipboard source board");
    expect(sourceSheet.root_mode).toBe("hidden");
    expect(sticky.card).toMatchObject({ kind: "sticky", color: "#FFEDD5" });
    expect(shape.style.fill).toBe("#FFEDD5");
    expect(connector.kind.connector).toMatchObject({ from: { topic: sticky.id }, to: { object: shape.id } });
    expect(source.manifest.resources).toHaveLength(1);
    const resource = source.manifest.resources[0];
    expect(imageCard.image).toBe(`thinoda-resource:${resource.id}`);
    expect(zipEntry(source.zip, `resources/${resource.id}`)).toEqual(image);

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Escape");
    await frames(page);
    await page.keyboard.press("Control+a");
    await frames(page);
    await copyOrCut(page, "copy");
    const copied = await page.evaluate(async () => {
        const state = window.__thinodaTestClipboard;
        const item = state.items[0];
        return {
            writes: state.writes,
            plainWrites: state.plainWrites,
            types: item.types,
            text: await (await item.getType("text/plain")).text(),
            html: await (await item.getType("text/html")).text(),
        };
    });
    expect(copied.writes).toBe(1);
    expect(copied.plainWrites).toBe(0);
    expect(copied.types).toEqual(expect.arrayContaining(["text/plain", "text/html"]));
    expect(copied.text).toContain("Clipboard idea");
    expect(copied.text).toContain("Clipboard shape");
    expect(copied.html).toContain("data-thinoda-board-v1=");

    await page.evaluate(({ html }) => {
        window.__thinodaTestClipboard.items = [new ClipboardItem({
            "text/plain": new Blob(["Different text"], { type: "text/plain" }),
            "text/html": new Blob([html], { type: "text/html" }),
        })];
    }, copied);
    await paste(page);
    const rejected = archive(await saveAs(page, "clipboard-board-rejected.thinoda"));
    expect(rejected.content.sheets).toEqual(source.content.sheets);
    await page.evaluate(({ text, html }) => {
        window.__thinodaTestClipboard.items = [new ClipboardItem({
            "text/plain": new Blob([text], { type: "text/plain" }),
            "text/html": new Blob([html], { type: "text/html" }),
        })];
    }, copied);

    await paste(page);
    const duplicated = archive(await saveAs(page, "clipboard-duplicated.thinoda"));
    const duplicateSheet = duplicated.content.sheets[0];
    const clonedSticky = Object.values(duplicateSheet.topics)
        .find((topic) => topic.title === sticky.title && topic.id !== sticky.id);
    const clonedImage = Object.values(duplicateSheet.topics)
        .find((topic) => topic.card?.kind === "image" && topic.id !== imageCard.id);
    const clonedShape = duplicateSheet.canvas?.objects
        .find((object) => object.kind.shape?.label === "Clipboard shape" && object.id !== shape.id);
    const clonedConnector = duplicateSheet.canvas?.objects
        .find((object) => object.kind.connector && object.id !== connector.id);
    if (!clonedSticky || !clonedImage || !clonedShape || !clonedConnector) {
        throw new Error("Mixed board paste did not create all copies");
    }
    expect(clonedConnector.kind.connector).toMatchObject({
        from: { topic: clonedSticky.id }, to: { object: clonedShape.id },
    });
    expect(clonedImage.image).toBe(imageCard.image);
    expect(zipEntry(duplicated.zip, `resources/${resource.id}`)).toEqual(image);

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    const sourceUndo = archive(await saveAs(page, "clipboard-source-undo.thinoda"));
    expect(sourceUndo.content.sheets).toEqual(source.content.sheets);
    expect(sourceUndo.manifest.resources).toEqual(source.manifest.resources);

    await openBoard(page);
    const targetBefore = archive(await saveAs(page, "clipboard-target-before.thinoda"));
    await paste(page);
    const target = archive(await saveAs(page, "clipboard-target.thinoda"));
    const targetSheet = target.content.sheets[0];
    const targetSticky = Object.values(targetSheet.topics).find((topic) => topic.title === sticky.title);
    const targetImage = Object.values(targetSheet.topics).find((topic) => topic.card?.kind === "image");
    const targetShape = targetSheet.canvas?.objects.find((object) => object.kind.shape?.label === "Clipboard shape");
    const targetConnector = targetSheet.canvas?.objects.find((object) => object.kind.connector);
    if (!targetSticky || !targetImage || !targetShape || !targetConnector) {
        throw new Error("Cross-document paste did not preserve the mixed board");
    }
    expect(targetSticky.card).toMatchObject({ kind: "sticky", color: "#FFEDD5" });
    expect(targetShape.style.fill).toBe("#FFEDD5");
    expect(targetConnector.kind.connector).toMatchObject({
        from: { topic: targetSticky.id }, to: { object: targetShape.id },
    });
    expect(target.manifest.resources).toHaveLength(1);
    const targetResource = target.manifest.resources[0];
    expect(targetImage.image).toBe(`thinoda-resource:${targetResource.id}`);
    expect(zipEntry(target.zip, `resources/${targetResource.id}`)).toEqual(image);

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    const targetUndo = archive(await saveAs(page, "clipboard-target-undo.thinoda"));
    expect(targetUndo.content.sheets).toEqual(targetBefore.content.sheets);
    expect(targetUndo.manifest.resources).toEqual(targetBefore.manifest.resources);
});

test("multiple embedded images survive cross-document paste, undo and redo", async ({ page }) => {
    const images = [
        { name: "clipboard-first.jpg", bytes: await jpeg(page, 64, 0.9) },
        { name: "clipboard-second.jpg", bytes: await jpeg(page, 48, 0.85) },
    ];
    for (const image of images) {
        await imagePicker(page, { name: image.name, mimeType: "image/jpeg", buffer: image.bytes });
        await page.waitForTimeout(500);
    }

    const source = archive(await saveAs(page, "clipboard-multiple-source.thinoda"));
    const sourceCards = Object.values(source.content.sheets[0].topics)
        .filter((topic) => topic.card?.kind === "image");
    expect(sourceCards).toHaveLength(2);
    expect(source.manifest.resources).toHaveLength(2);
    for (const image of images) {
        const resource = source.manifest.resources.find((entry) => entry.name === image.name);
        if (!resource) throw new Error(`Missing source resource ${image.name}`);
        expect(sourceCards.some((card) => card.image === `thinoda-resource:${resource.id}`)).toBe(true);
        expect(zipEntry(source.zip, `resources/${resource.id}`)).toEqual(image.bytes);
    }

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Escape");
    await frames(page);
    await page.keyboard.press("Control+a");
    await frames(page);
    await copyOrCut(page, "copy");
    await openBoard(page);
    const before = archive(await saveAs(page, "clipboard-multiple-before.thinoda"));
    await paste(page);
    const pasted = archive(await saveAs(page, "clipboard-multiple-pasted.thinoda"));
    const pastedCards = Object.values(pasted.content.sheets[0].topics)
        .filter((topic) => topic.card?.kind === "image");
    expect(pastedCards).toHaveLength(2);
    expect(pasted.manifest.resources).toEqual(source.manifest.resources);
    for (const image of images) {
        const resource = pasted.manifest.resources.find((entry) => entry.name === image.name);
        if (!resource) throw new Error(`Missing pasted resource ${image.name}`);
        expect(pastedCards.some((card) => card.image === `thinoda-resource:${resource.id}`)).toBe(true);
        expect(zipEntry(pasted.zip, `resources/${resource.id}`)).toEqual(image.bytes);
    }

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    const undone = archive(await saveAs(page, "clipboard-multiple-undo.thinoda"));
    expect(undone.content.sheets).toEqual(before.content.sheets);
    expect(undone.manifest.resources).toEqual(before.manifest.resources);

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+Shift+z");
    await frames(page);
    const redone = archive(await saveAs(page, "clipboard-multiple-redo.thinoda"));
    expect(redone.content.sheets).toEqual(pasted.content.sheets);
    expect(redone.manifest.resources).toEqual(pasted.manifest.resources);
    for (const image of images) {
        const resource = redone.manifest.resources.find((entry) => entry.name === image.name);
        if (!resource) throw new Error(`Missing restored resource ${image.name}`);
        expect(zipEntry(redone.zip, `resources/${resource.id}`)).toEqual(image.bytes);
    }
});

test("identical external text does not reuse an earlier rich board selection", async ({ page }) => {
    await createShape(page, "Clipboard plain replacement");
    const before = archive(await saveAs(page, "clipboard-plain-before.thinoda"));
    await copyOrCut(page, "copy");
    await page.evaluate(async () => {
        const state = window.__thinodaTestClipboard;
        const text = await (await state.items[0].getType("text/plain")).text();
        state.items = [new ClipboardItem({ "text/plain": new Blob([text], { type: "text/plain" }) })];
    });
    await paste(page);
    const after = archive(await saveAs(page, "clipboard-plain-after.thinoda"));
    const beforeSheet = before.content.sheets[0];
    const afterSheet = after.content.sheets[0];
    expect(afterSheet.canvas?.objects).toEqual(beforeSheet.canvas?.objects);
    const added = Object.values(afterSheet.topics)
        .filter((topic) => !beforeSheet.topics[String(topic.id)]);
    expect(added).toHaveLength(1);
    expect(added[0].title).toBe("Clipboard plain replacement");
    expect(added[0].card).toBeUndefined();
    expect(afterSheet.floating).toContain(added[0].id);
});

test("draw.io paste keeps the first page and can be undone", async ({ page }, testInfo) => {
    const first = [
        '<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>',
        '<mxCell id="frame" value="Frame" style="swimlane;startSize=24;collapsible=0;" vertex="1" parent="1">',
        '<mxGeometry x="100" y="200" width="240" height="120" as="geometry"/></mxCell>',
        '<mxCell id="child" value="First page child" style="shape=rectangle;" vertex="1" parent="frame">',
        '<mxGeometry x="20" y="30" width="80" height="40" as="geometry"/></mxCell>',
        '</root></mxGraphModel>',
    ].join("");
    const second = [
        '<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>',
        '<mxCell id="other" value="Second page shape" style="shape=rectangle;" vertex="1" parent="1">',
        '<mxGeometry width="80" height="40" as="geometry"/></mxCell>',
        '</root></mxGraphModel>',
    ].join("");
    const file = `<mxfile><diagram name="First">${first}</diagram><diagram name="Second">${second}</diagram></mxfile>`;
    const before = archive(await saveAs(page, "drawio-paste-before.thinoda"));
    await page.evaluate((text) => {
        window.__thinodaTestClipboard.items = [new ClipboardItem({
            "text/plain": new Blob([text], { type: "text/plain" }),
        })];
    }, encodeURIComponent(file));
    await paste(page);
    await page.screenshot({ path: testInfo.outputPath("drawio-paste-warning.png") });
    const after = archive(await saveAs(page, "drawio-paste-after.thinoda"));
    const objects = after.content.sheets[after.content.active_sheet].canvas?.objects ?? [];
    expect(objects.map((object) => object.kind.shape?.label)).toEqual(["First page child"]);

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    const undone = archive(await saveAs(page, "drawio-paste-undo.thinoda"));
    expect(undone.content.sheets).toEqual(before.content.sheets);
});

test("draw.io paste warns about a detached connector", async ({ page }, testInfo) => {
    const text = [
        '<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>',
        '<mxCell id="shape" value="Target" vertex="1" parent="1">',
        '<mxGeometry x="100" width="100" height="60" as="geometry"/></mxCell>',
        '<mxCell id="edge" edge="1" parent="1" source="missing" target="shape" style="endArrow=none;">',
        '<mxGeometry relative="1" as="geometry"><mxPoint x="7" y="8" as="sourcePoint"/></mxGeometry></mxCell>',
        '</root></mxGraphModel>',
    ].join("");
    const before = archive(await saveAs(page, "drawio-detached-before.thinoda"));
    await page.evaluate((value) => {
        window.__thinodaTestClipboard.items = [new ClipboardItem({
            "text/plain": new Blob([value], { type: "text/plain" }),
        })];
    }, text);
    await paste(page);
    await page.screenshot({ path: testInfo.outputPath("drawio-detached-warning.png") });
    const after = archive(await saveAs(page, "drawio-detached-after.thinoda"));
    const objects = after.content.sheets[after.content.active_sheet].canvas?.objects ?? [];
    expect(objects).toHaveLength(2);
    expect(objects.some((object) => object.kind.shape?.label === "Target")).toBe(true);
    expect(objects.some((object) => object.kind.connector)).toBe(true);

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    const undone = archive(await saveAs(page, "drawio-detached-undo.thinoda"));
    expect(undone.content.sheets).toEqual(before.content.sheets);
});

test("draw.io paste warns about an unsupported chair placeholder", async ({ page }, testInfo) => {
    const text = [
        '<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>',
        '<mxCell id="chair" value="Chair" vertex="1" parent="1" ',
        'style="shape=mxgraph.floorplan.office_chair;fillColor=#ffffff;strokeColor=#000000;">',
        '<mxGeometry x="100" y="100" width="40" height="43" as="geometry"/></mxCell>',
        '</root></mxGraphModel>',
    ].join("");
    const before = archive(await saveAs(page, "drawio-chair-before.thinoda"));
    await page.evaluate((value) => {
        window.__thinodaTestClipboard.items = [new ClipboardItem({
            "text/plain": new Blob([value], { type: "text/plain" }),
        })];
    }, text);
    await paste(page);
    await page.screenshot({ path: testInfo.outputPath("drawio-chair-warning.png") });
    const after = archive(await saveAs(page, "drawio-chair-after.thinoda"));
    const objects = after.content.sheets[after.content.active_sheet].canvas?.objects ?? [];
    expect(objects).toHaveLength(1);
    expect(objects[0].kind.shape).toEqual({ shape: "rectangle", label: "Chair" });

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    const undone = archive(await saveAs(page, "drawio-chair-undo.thinoda"));
    expect(undone.content.sheets).toEqual(before.content.sheets);
});

test("draw.io metadata placeholders warn on paste and can be undone", async ({ page }, testInfo) => {
    const text = [
        '<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>',
        '<object id="shape" label="Width %width%" placeholders="1">',
        '<mxCell style="shape=rectangle;" vertex="1" parent="1">',
        '<mxGeometry width="100" height="60" as="geometry"/></mxCell></object>',
        '</root></mxGraphModel>',
    ].join("");
    const before = archive(await saveAs(page, "drawio-placeholder-before.thinoda"));
    await page.evaluate((value) => {
        window.__thinodaTestClipboard.items = [new ClipboardItem({
            "text/plain": new Blob([value], { type: "text/plain" }),
        })];
    }, text);
    await paste(page);
    await expectWarningToast(page, testInfo.outputPath("drawio-placeholder-warning.png"));
    const after = archive(await saveAs(page, "drawio-placeholder-after.thinoda"));
    const objects = after.content.sheets[after.content.active_sheet].canvas?.objects ?? [];
    expect(objects).toHaveLength(1);
    expect(objects[0].kind.shape?.label).toBe("Width %width%");

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    const undone = archive(await saveAs(page, "drawio-placeholder-undo.thinoda"));
    expect(undone.content.sheets).toEqual(before.content.sheets);
});

test("draw.io paste omits a connector with a malformed endpoint", async ({ page }, testInfo) => {
    const text = [
        '<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>',
        '<mxCell id="shape" value="Target" vertex="1" parent="1">',
        '<mxGeometry x="100" width="100" height="60" as="geometry"/></mxCell>',
        '<mxCell id="edge" edge="1" parent="1" source="missing" target="shape" style="endArrow=none;">',
        '<mxGeometry relative="1" as="geometry"><mxPoint x="invalid" y="8" as="sourcePoint"/></mxGeometry></mxCell>',
        '</root></mxGraphModel>',
    ].join("");
    const before = archive(await saveAs(page, "drawio-malformed-before.thinoda"));
    await page.evaluate((value) => {
        window.__thinodaTestClipboard.items = [new ClipboardItem({
            "text/plain": new Blob([value], { type: "text/plain" }),
        })];
    }, text);
    await paste(page);
    await page.screenshot({ path: testInfo.outputPath("drawio-malformed-warning.png") });
    const after = archive(await saveAs(page, "drawio-malformed-after.thinoda"));
    const objects = after.content.sheets[after.content.active_sheet].canvas?.objects ?? [];
    expect(objects).toHaveLength(1);
    expect(objects[0].kind.shape?.label).toBe("Target");

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    const undone = archive(await saveAs(page, "drawio-malformed-undo.thinoda"));
    expect(undone.content.sheets).toEqual(before.content.sheets);
});

test("draw.io paste keeps a connector with an out-of-range waypoint", async ({ page }, testInfo) => {
    const text = [
        '<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>',
        '<mxCell id="edge" edge="1" parent="1" style="endArrow=none;">',
        '<mxGeometry relative="1" as="geometry"><mxPoint x="0" y="0" as="sourcePoint"/>',
        '<mxPoint x="100" y="0" as="targetPoint"/><Array as="points">',
        '<mxPoint x="50" y="20"/><mxPoint x="1000001" y="5"/></Array></mxGeometry></mxCell>',
        '</root></mxGraphModel>',
    ].join("");
    const before = archive(await saveAs(page, "drawio-range-before.thinoda"));
    await page.evaluate((value) => {
        window.__thinodaTestClipboard.items = [new ClipboardItem({
            "text/plain": new Blob([value], { type: "text/plain" }),
        })];
    }, text);
    await paste(page);
    await page.screenshot({ path: testInfo.outputPath("drawio-range-warning.png") });
    const after = archive(await saveAs(page, "drawio-range-after.thinoda"));
    const objects = after.content.sheets[after.content.active_sheet].canvas?.objects ?? [];
    expect(objects).toHaveLength(1);
    expect(objects[0].kind.connector?.waypoints).toHaveLength(1);

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    const undone = archive(await saveAs(page, "drawio-range-undo.thinoda"));
    expect(undone.content.sheets).toEqual(before.content.sheets);
});

test("draw.io paste warns about unresolved shape appearance", async ({ page }, testInfo) => {
    const text = [
        '<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>',
        '<mxCell id="shape" vertex="1" parent="1" value="Theme" ',
        'style="shape=rectangle;fillColor=light-dark(#ffffff,#111111);strokeColor=inherit;">',
        '<mxGeometry width="100" height="60" as="geometry"/></mxCell>',
        '</root></mxGraphModel>',
    ].join("");
    const before = archive(await saveAs(page, "drawio-appearance-before.thinoda"));
    await page.evaluate((value) => {
        window.__thinodaTestClipboard.items = [new ClipboardItem({
            "text/plain": new Blob([value], { type: "text/plain" }),
        })];
    }, text);
    await paste(page);
    await page.screenshot({ path: testInfo.outputPath("drawio-appearance-warning.png") });
    const after = archive(await saveAs(page, "drawio-appearance-after.thinoda"));
    const objects = after.content.sheets[after.content.active_sheet].canvas?.objects ?? [];
    expect(objects).toHaveLength(1);
    expect(objects[0].kind.shape?.label).toBe("Theme");

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    const undone = archive(await saveAs(page, "drawio-appearance-undo.thinoda"));
    expect(undone.content.sheets).toEqual(before.content.sheets);
});

test("draw.io HTML labels keep readable text through paste, edit, copy and undo", async ({ page }, testInfo) => {
    const text = [
        '<mxGraphModel adaptiveColors="none"><root><mxCell id="0"/><mxCell id="1" parent="0"/>',
        '<mxCell id="shape" vertex="1" parent="1" ',
        'value="Before&lt;div title=&quot;2 &gt; 1&quot;&gt;A &amp;copy;&lt;/div&gt;After" ',
        'style="shape=rectangle;html=1;fillColor=#fff;strokeColor=#000;fontColor=#000;">',
        '<mxGeometry width="120" height="60" as="geometry"/></mxCell>',
        '</root></mxGraphModel>',
    ].join("");
    const before = archive(await saveAs(page, "drawio-html-label-before.thinoda"));
    await page.evaluate((value) => {
        window.__thinodaTestClipboard.items = [new ClipboardItem({
            "text/plain": new Blob([value], { type: "text/plain" }),
        })];
    }, text);
    await paste(page);
    await expectWarningToast(page, testInfo.outputPath("drawio-html-label-warning.png"));
    const after = archive(await saveAs(page, "drawio-html-label-after.thinoda"));
    const objects = after.content.sheets[after.content.active_sheet].canvas?.objects ?? [];
    expect(objects).toHaveLength(1);
    expect(objects[0].kind.shape?.label).toBe("Before\nA \u00a9\nAfter");

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("F2");
    await frames(page);
    const label = page.locator("input[type='text']:focus, textarea:focus");
    await expect(label).toBeFocused();
    await label.fill("Edited \u00a9");
    await page.keyboard.press("Enter");
    await frames(page);

    const previous = await page.evaluate(() => window.__thinodaTestClipboard.plainWrites);
    await command(page, "Copy as draw.io");
    await expect.poll(() => page.evaluate(() => window.__thinodaTestClipboard.plainWrites))
        .toBe(previous + 1);
    const copied = await page.evaluate(async () =>
        (await window.__thinodaTestClipboard.items[0].getType("text/plain")).text());
    expect(decodeURIComponent(copied)).toContain('value="Edited \u00a9"');

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    const restored = archive(await saveAs(page, "drawio-html-label-edit-undo.thinoda"));
    expect(restored.content.sheets[restored.content.active_sheet].canvas?.objects?.[0]
        .kind.shape?.label).toBe("Before\nA \u00a9\nAfter");
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    const undone = archive(await saveAs(page, "drawio-html-label-undo.thinoda"));
    expect(undone.content.sheets).toEqual(before.content.sheets);
});

test("draw.io paste warns about collapsible HTML whitespace without changing text", async ({ page }, testInfo) => {
    const text = [
        '<mxGraphModel adaptiveColors="none"><root><mxCell id="0"/><mxCell id="1" parent="0"/>',
        '<mxCell id="shape" vertex="1" parent="1" ',
        'value="A&lt;span&gt; &lt;/span&gt;&lt;span&gt; B&lt;/span&gt;" ',
        'style="shape=rectangle;html=1;fillColor=#fff;strokeColor=#000;fontColor=#000;">',
        '<mxGeometry width="120" height="60" as="geometry"/></mxCell>',
        '</root></mxGraphModel>',
    ].join("");
    const before = archive(await saveAs(page, "drawio-whitespace-before.thinoda"));
    await page.evaluate((value) => {
        window.__thinodaTestClipboard.items = [new ClipboardItem({
            "text/plain": new Blob([value], { type: "text/plain" }),
        })];
    }, text);
    await paste(page);
    await expectWarningToast(page, testInfo.outputPath("drawio-whitespace-warning.png"));
    const after = archive(await saveAs(page, "drawio-whitespace-after.thinoda"));
    const objects = after.content.sheets[after.content.active_sheet].canvas?.objects ?? [];
    expect(objects).toHaveLength(1);
    expect(objects[0].kind.shape?.label).toBe("A  B");

    const previous = await page.evaluate(() => window.__thinodaTestClipboard.plainWrites);
    await command(page, "Copy as draw.io");
    await expect.poll(() => page.evaluate(() => window.__thinodaTestClipboard.plainWrites))
        .toBe(previous + 1);
    const copied = await page.evaluate(async () =>
        (await window.__thinodaTestClipboard.items[0].getType("text/plain")).text());
    expect(decodeURIComponent(copied)).toContain('value="A  B"');

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    const undone = archive(await saveAs(page, "drawio-whitespace-undo.thinoda"));
    expect(undone.content.sheets).toEqual(before.content.sheets);
});

test("draw.io connector label size is disclosed and copy matches the canvas", async ({ page }, testInfo) => {
    const text = [
        '<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>',
        '<mxCell id="edge" value="Small label" style="endArrow=none;strokeColor=#000000;',
        'fontColor=#000000;fontSize=11;" edge="1" parent="1">',
        '<mxGeometry relative="1" as="geometry"><mxPoint x="0" y="0" as="sourcePoint"/>',
        '<mxPoint x="160" y="0" as="targetPoint"/></mxGeometry></mxCell>',
        '</root></mxGraphModel>',
    ].join("");
    const before = archive(await saveAs(page, "drawio-label-size-before.thinoda"));
    await page.evaluate((value) => {
        window.__thinodaTestClipboard.items = [new ClipboardItem({
            "text/plain": new Blob([value], { type: "text/plain" }),
        })];
    }, text);
    await paste(page);
    await expectWarningToast(page, testInfo.outputPath("drawio-label-size-warning.png"));
    const after = archive(await saveAs(page, "drawio-label-size-after.thinoda"));
    const objects = after.content.sheets[after.content.active_sheet].canvas?.objects ?? [];
    expect(objects).toHaveLength(1);
    expect(objects[0].kind.connector?.label).toBe("Small label");

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("F2");
    await frames(page);
    const label = page.locator("input[type='text']");
    await expect(label).toBeFocused();
    await label.fill("Edited label");
    await page.keyboard.press("Enter");
    await frames(page);

    const previous = await page.evaluate(() => window.__thinodaTestClipboard.plainWrites);
    await command(page, "Copy as draw.io");
    await expect.poll(() => page.evaluate(() => window.__thinodaTestClipboard.plainWrites))
        .toBe(previous + 1);
    const copied = await page.evaluate(async () =>
        (await window.__thinodaTestClipboard.items[0].getType("text/plain")).text());
    const model = decodeURIComponent(copied);
    expect(model).toContain('value="Edited label"');
    expect(model).toContain("fontSize=12;");

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    const restored = archive(await saveAs(page, "drawio-label-size-edit-undo.thinoda"));
    expect(restored.content.sheets[restored.content.active_sheet].canvas?.objects?.[0]
        .kind.connector?.label).toBe("Small label");
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    const undone = archive(await saveAs(page, "drawio-label-size-undo.thinoda"));
    expect(undone.content.sheets).toEqual(before.content.sheets);
});

test("draw.io named text styling is disclosed on paste", async ({ page }, testInfo) => {
    const text = [
        '<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>',
        '<mxCell id="text" value="Named text" vertex="1" parent="1" ',
        'style="text;fontColor=#000000;fontSize=12;">',
        '<mxGeometry width="100" height="60" as="geometry"/></mxCell>',
        '</root></mxGraphModel>',
    ].join("");
    const before = archive(await saveAs(page, "drawio-named-text-before.thinoda"));
    await page.evaluate((value) => {
        window.__thinodaTestClipboard.items = [new ClipboardItem({
            "text/plain": new Blob([value], { type: "text/plain" }),
        })];
    }, text);
    await paste(page);
    await expectWarningToast(page, testInfo.outputPath("drawio-named-text-warning.png"));
    const after = archive(await saveAs(page, "drawio-named-text-after.thinoda"));
    const objects = after.content.sheets[after.content.active_sheet].canvas?.objects ?? [];
    expect(objects).toHaveLength(1);
    expect(objects[0].kind.shape?.label).toBe("Named text");

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    const undone = archive(await saveAs(page, "drawio-named-text-undo.thinoda"));
    expect(undone.content.sheets).toEqual(before.content.sheets);
});

test("draw.io paste and copy preserve an editable final state", async ({ page }, testInfo) => {
    const text = [
        '<mxGraphModel adaptiveColors="none"><root><mxCell id="0"/><mxCell id="1" parent="0"/>',
        '<mxCell id="shape" vertex="1" parent="1" value="" ',
        'style="shape=endState;perimeter=ellipsePerimeter;fillColor=#ed1234;',
        'strokeColor=#2244aa;strokeWidth=2;">',
        '<mxGeometry width="90" height="70" as="geometry"/></mxCell>',
        '</root></mxGraphModel>',
    ].join("");
    const before = archive(await saveAs(page, "drawio-final-state-before.thinoda"));
    await page.evaluate((value) => {
        window.__thinodaTestClipboard.items = [new ClipboardItem({
            "text/plain": new Blob([value], { type: "text/plain" }),
        })];
    }, text);
    await paste(page);
    const viewport = page.viewportSize();
    if (!viewport) throw new Error("Cannot inspect the canvas without a viewport");
    const canvas = await page.screenshot({
        path: testInfo.outputPath("drawio-final-state-paste.png"),
        clip: {
            x: 0,
            y: 100,
            width: Math.floor(viewport.width * 0.75),
            height: viewport.height - 150,
        },
    });
    const fill = (await colorBounds(page, canvas, [[237, 18, 52]], 15))[0];
    expect(fill.count).toBeGreaterThan(2500);
    expect(fill.maxX - fill.minX).toBeGreaterThan(70);
    expect(fill.maxY - fill.minY).toBeGreaterThan(50);
    const after = archive(await saveAs(page, "drawio-final-state-after.thinoda"));
    const objects = after.content.sheets[after.content.active_sheet].canvas?.objects ?? [];
    expect(objects).toHaveLength(1);
    expect(objects[0].kind.shape?.shape).toBe("final-state");
    expect(objects[0].kind.shape?.label ?? "").toBe("");
    expect(after.manifest.version).toBe(24);

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("F2");
    await frames(page);
    const label = page.locator("input[type='text']");
    await expect(label).toBeFocused();
    await label.fill("Finish");
    await page.keyboard.press("Enter");
    await frames(page);
    const edited = archive(await saveAs(page, "drawio-final-state-edited.thinoda"));
    expect(edited.content.sheets[edited.content.active_sheet].canvas?.objects?.[0].kind.shape)
        .toMatchObject({ shape: "final-state", label: "Finish" });

    const previous = await page.evaluate(() => window.__thinodaTestClipboard.plainWrites);
    await command(page, "Copy as draw.io");
    await expect.poll(() => page.evaluate(() => window.__thinodaTestClipboard.plainWrites))
        .toBe(previous + 1);
    const copied = await page.evaluate(async () =>
        (await window.__thinodaTestClipboard.items[0].getType("text/plain")).text());
    const model = decodeURIComponent(copied);
    expect(model).toContain("shape=endState;perimeter=ellipsePerimeter;");
    expect(model).toContain("Finish");
    await expectWarningToast(page, testInfo.outputPath("drawio-final-state-copy-warning.png"));

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    const labelUndone = archive(await saveAs(page, "drawio-final-state-label-undo.thinoda"));
    const restored = labelUndone.content.sheets[labelUndone.content.active_sheet].canvas?.objects?.[0];
    expect(restored?.kind.shape?.shape).toBe("final-state");
    expect(restored?.kind.shape?.label ?? "").toBe("");
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    const undone = archive(await saveAs(page, "drawio-final-state-undo.thinoda"));
    expect(undone.content.sheets).toEqual(before.content.sheets);
});

test("draw.io paste and copy preserve an editable double ellipse", async ({ page }, testInfo) => {
    const text = [
        '<mxGraphModel adaptiveColors="none"><root><mxCell id="0"/><mxCell id="1" parent="0"/>',
        '<mxCell id="shape" vertex="1" parent="1" value="End" ',
        'style="shape=doubleEllipse;fillColor=none;strokeColor=#ed1234;',
        'fontColor=#000000;strokeWidth=4;margin=8.5;">',
        '<mxGeometry width="100" height="60" as="geometry"/></mxCell>',
        '</root></mxGraphModel>',
    ].join("");
    const before = archive(await saveAs(page, "drawio-double-ellipse-before.thinoda"));
    await page.evaluate((value) => {
        window.__thinodaTestClipboard.items = [new ClipboardItem({
            "text/plain": new Blob([value], { type: "text/plain" }),
        })];
    }, text);
    await paste(page);
    await page.screenshot({ path: testInfo.outputPath("drawio-double-ellipse-paste.png") });
    const viewport = page.viewportSize();
    if (!viewport) throw new Error("Cannot inspect the canvas without a viewport");
    // Exclude the inspector's matching stroke-color swatch.
    const canvas = await page.screenshot({
        clip: {
            x: 0,
            y: 100,
            width: Math.floor(viewport.width * 0.75),
            height: viewport.height - 150,
        },
    });
    const bounds = (await colorBounds(page, canvas, [[237, 18, 52]], 15))[0];
    expect(bounds.count).toBeGreaterThan(100);
    expect(bounds.maxX - bounds.minX).toBeGreaterThan(80);
    expect(bounds.maxX - bounds.minX).toBeLessThan(140);
    expect(bounds.maxY - bounds.minY).toBeGreaterThan(45);
    expect(bounds.maxY - bounds.minY).toBeLessThan(100);
    const topStrokes = await page.evaluate(async ({ base64, bounds }) => {
        const image = new Image();
        image.src = `data:image/png;base64,${base64}`;
        await image.decode();
        const surface = document.createElement("canvas");
        surface.width = image.width;
        surface.height = image.height;
        const context = surface.getContext("2d");
        if (!context) throw new Error("Cannot inspect the rendered double ellipse");
        context.drawImage(image, 0, 0);
        const x = Math.round((bounds.minX + bounds.maxX) / 2);
        const middle = Math.round((bounds.minY + bounds.maxY) / 2);
        const strokes: number[] = [];
        let inside = false;
        for (let y = bounds.minY; y < middle - 4; y++) {
            const [r, g, b] = context.getImageData(x, y, 1, 1).data;
            const red = Math.abs(r - 237) < 30 && g < 45 && Math.abs(b - 52) < 30;
            if (red && !inside) strokes.push(y);
            inside = red;
        }
        return strokes;
    }, { base64: canvas.toString("base64"), bounds });
    expect(topStrokes).toHaveLength(2);
    const after = archive(await saveAs(page, "drawio-double-ellipse-after.thinoda"));
    const objects = after.content.sheets[after.content.active_sheet].canvas?.objects ?? [];
    expect(objects).toHaveLength(1);
    expect(objects[0].kind.shape).toMatchObject({ shape: "double-ellipse", label: "End" });
    expect(objects[0].double_ellipse_margin).toBe(8.5);
    expect(after.manifest.version).toBe(21);

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("F2");
    await frames(page);
    const label = page.locator("input[type='text']");
    await expect(label).toBeFocused();
    await label.fill("Finish");
    await page.keyboard.press("Enter");
    await frames(page);
    const edited = archive(await saveAs(page, "drawio-double-ellipse-edited.thinoda"));
    const editedObject = edited.content.sheets[edited.content.active_sheet].canvas?.objects?.[0];
    expect(editedObject?.kind.shape).toMatchObject({ shape: "double-ellipse", label: "Finish" });
    expect(editedObject?.double_ellipse_margin).toBe(8.5);

    const previous = await page.evaluate(() => window.__thinodaTestClipboard.plainWrites);
    await command(page, "Copy as draw.io");
    await expect.poll(() => page.evaluate(() => window.__thinodaTestClipboard.plainWrites))
        .toBe(previous + 1);
    const copied = await page.evaluate(async () =>
        (await window.__thinodaTestClipboard.items[0].getType("text/plain")).text());
    const model = decodeURIComponent(copied);
    expect(model).toContain("shape=doubleEllipse;");
    expect(model).toContain("margin=8.5;");
    expect(model).toContain("Finish");
    await page.screenshot({ path: testInfo.outputPath("drawio-double-ellipse-copy-warning.png") });

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    const labelUndone = archive(await saveAs(page, "drawio-double-ellipse-label-undo.thinoda"));
    const restored = labelUndone.content.sheets[labelUndone.content.active_sheet].canvas?.objects?.[0];
    expect(restored?.kind.shape).toMatchObject({ shape: "double-ellipse", label: "End" });
    expect(restored?.double_ellipse_margin).toBe(8.5);
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    const undone = archive(await saveAs(page, "drawio-double-ellipse-undo.thinoda"));
    expect(undone.content.sheets).toEqual(before.content.sheets);
});

test("folded draw.io notes paste as editable cards with copy and paste warnings", async ({ page }, testInfo) => {
    const text = [
        '<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>',
        '<mxCell id="note" vertex="1" parent="1" value="Folded note" ',
        'style="shape=note2;boundedLbl=1;size=15;align=right;fillColor=#F4C27A;">',
        '<mxGeometry width="180" height="120" as="geometry"/></mxCell>',
        '</root></mxGraphModel>',
    ].join("");
    const before = archive(await saveAs(page, "drawio-note-before.thinoda"));
    await page.evaluate((value) => {
        window.__thinodaTestClipboard.items = [new ClipboardItem({
            "text/plain": new Blob([value], { type: "text/plain" }),
        })];
    }, text);
    await paste(page);
    await page.screenshot({ path: testInfo.outputPath("drawio-note-paste-warning.png") });
    const after = archive(await saveAs(page, "drawio-note-after.thinoda"));
    const sheet = after.content.sheets[after.content.active_sheet];
    const note = Object.values(sheet.topics).find((topic) => topic.title === "Folded note");
    expect(note?.card).toMatchObject({ kind: "sticky", color: "#F4C27A" });
    expect(note?.style?.align).toBe("right");

    const previous = await page.evaluate(() => window.__thinodaTestClipboard.plainWrites);
    await command(page, "Copy as draw.io");
    await expect.poll(() => page.evaluate(() => window.__thinodaTestClipboard.plainWrites))
        .toBe(previous + 1);
    const copied = await page.evaluate(async () =>
        (await window.__thinodaTestClipboard.items[0].getType("text/plain")).text());
    expect(decodeURIComponent(copied)).toContain("shape=note;thinodaCard=sticky;");
    await page.screenshot({ path: testInfo.outputPath("drawio-note-copy-warning.png") });

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    const undone = archive(await saveAs(page, "drawio-note-undo.thinoda"));
    expect(undone.content.sheets).toEqual(before.content.sheets);
});

test("draw.io multipage paste reports omitted pages without unrelated appearance warnings", async ({ page }, testInfo) => {
    const text = [
        '<mxfile><diagram name="First"><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>',
        '<mxCell id="shape" vertex="1" parent="1" value="First page" style="shape=rectangle;">',
        '<mxGeometry width="120" height="60" as="geometry"/></mxCell>',
        '</root></mxGraphModel></diagram>',
        '<diagram name="Later"><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>',
        '<mxCell id="note" vertex="1" parent="1" value="Later note" style="shape=note2;">',
        '<mxGeometry width="120" height="60" as="geometry"/></mxCell>',
        '<mxCell id="unknown" vertex="1" parent="1" value="Later shape" style="shape=unknown;">',
        '<mxGeometry x="150" width="120" height="60" as="geometry"/></mxCell>',
        '</root></mxGraphModel></diagram></mxfile>',
    ].join("");
    const before = archive(await saveAs(page, "drawio-multipage-before.thinoda"));
    await page.evaluate((value) => {
        window.__thinodaTestClipboard.items = [new ClipboardItem({
            "text/plain": new Blob([value], { type: "text/plain" }),
        })];
    }, text);
    await paste(page);
    await page.screenshot({ path: testInfo.outputPath("drawio-multipage-warning.png") });
    const after = archive(await saveAs(page, "drawio-multipage-after.thinoda"));
    const sheet = after.content.sheets[after.content.active_sheet];
    expect(sheet.canvas?.objects).toHaveLength(1);
    expect(sheet.canvas?.objects[0].kind.shape?.label).toBe("First page");
    expect(Object.values(sheet.topics).some((topic) => topic.title === "Later note")).toBe(false);

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    const undone = archive(await saveAs(page, "drawio-multipage-undo.thinoda"));
    expect(undone.content.sheets).toEqual(before.content.sheets);
});

test("draw.io process paste warns and copies its square approximation", async ({ page }, testInfo) => {
    const text = [
        '<mxGraphModel adaptiveColors="none"><root><mxCell id="0"/><mxCell id="1" parent="0"/>',
        '<mxCell id="shape" vertex="1" parent="1" value="Task" style="shape=mxgraph.flowchart.process;">',
        '<mxGeometry width="300" height="200" as="geometry"/></mxCell>',
        '</root></mxGraphModel>',
    ].join("");
    const before = archive(await saveAs(page, "drawio-process-before.thinoda"));
    await page.evaluate((value) => {
        window.__thinodaTestClipboard.items = [new ClipboardItem({
            "text/plain": new Blob([value], { type: "text/plain" }),
        })];
    }, text);
    await paste(page);
    await page.screenshot({ path: testInfo.outputPath("drawio-process-warning.png") });
    const after = archive(await saveAs(page, "drawio-process-after.thinoda"));
    const objects = after.content.sheets[after.content.active_sheet].canvas?.objects ?? [];
    expect(objects).toHaveLength(1);
    expect(objects[0].kind.shape).toMatchObject({ shape: "process", label: "Task" });

    const previous = await page.evaluate(() => window.__thinodaTestClipboard.plainWrites);
    await command(page, "Copy as draw.io");
    await expect.poll(() => page.evaluate(() => window.__thinodaTestClipboard.plainWrites))
        .toBe(previous + 1);
    const copied = await page.evaluate(async () =>
        (await window.__thinodaTestClipboard.items[0].getType("text/plain")).text());
    expect(decodeURIComponent(copied)).toContain("shape=rectangle;rounded=0;thinodaShape=process;");

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    const undone = archive(await saveAs(page, "drawio-process-undo.thinoda"));
    expect(undone.content.sheets).toEqual(before.content.sheets);
});

test("copy as draw.io warns about a collapsed process outline", async ({ page }, testInfo) => {
    const text = [
        '<mxGraphModel adaptiveColors="none"><root><mxCell id="0"/><mxCell id="1" parent="0"/>',
        '<mxCell id="shape" vertex="1" parent="1" ',
        'style="shape=rectangle;rounded=0;thinodaShape=process;strokeColor=#000000;fillColor=none;">',
        '<mxGeometry width="0" height="100" as="geometry"/></mxCell>',
        '</root></mxGraphModel>',
    ].join("");
    await page.evaluate((value) => {
        window.__thinodaTestClipboard.items = [new ClipboardItem({
            "text/plain": new Blob([value], { type: "text/plain" }),
        })];
    }, text);
    await paste(page);
    const previous = await page.evaluate(() => window.__thinodaTestClipboard.plainWrites);
    await command(page, "Copy as draw.io");
    await expect.poll(() => page.evaluate(() => window.__thinodaTestClipboard.plainWrites))
        .toBe(previous + 1);
    const copied = await page.evaluate(async () =>
        (await window.__thinodaTestClipboard.items[0].getType("text/plain")).text());
    expect(decodeURIComponent(copied)).toContain("shape=rectangle;rounded=0;thinodaShape=process;");
    expect(decodeURIComponent(copied)).toContain('width="0"');
    await page.screenshot({ path: testInfo.outputPath("drawio-collapsed-process-copy-warning.png") });
});

test("draw.io annotation pastes as an editable open bracket", async ({ page }, testInfo) => {
    const text = [
        '<mxGraphModel adaptiveColors="none"><root><mxCell id="0"/><mxCell id="1" parent="0"/>',
        '<mxCell id="shape" vertex="1" parent="1" ',
        'style="shape=mxgraph.flowchart.annotation_1;fillColor=#ff0000;strokeColor=#336699;strokeWidth=4;">',
        '<mxGeometry width="100" height="100" as="geometry"/></mxCell>',
        '</root></mxGraphModel>',
    ].join("");
    const before = archive(await saveAs(page, "drawio-annotation-before.thinoda"));
    await page.evaluate((value) => {
        window.__thinodaTestClipboard.items = [new ClipboardItem({
            "text/plain": new Blob([value], { type: "text/plain" }),
        })];
    }, text);
    await paste(page);
    await page.screenshot({ path: testInfo.outputPath("drawio-annotation-warning.png") });
    const after = archive(await saveAs(page, "drawio-annotation-after.thinoda"));
    const objects = after.content.sheets[after.content.active_sheet].canvas?.objects ?? [];
    expect(objects).toHaveLength(1);
    expect(objects[0].kind.shape?.shape).toBe("rectangle");
    expect(objects[0].rectangle_sides).toEqual({ top: true, right: false, bottom: true, left: true });
    expect(objects[0].style).toMatchObject({ fill: "#00000000", stroke: "#336699" });

    // The egui inspector is painted in the canvas rather than exposed as DOM controls.
    await page.mouse.click(1062, 809);
    await frames(page);
    const edited = archive(await saveAs(page, "drawio-annotation-edited.thinoda"));
    expect(edited.content.sheets[edited.content.active_sheet].canvas?.objects[0].rectangle_sides?.right)
        .toBe(true);

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    const restored = archive(await saveAs(page, "drawio-annotation-restored.thinoda"));
    expect(restored.content.sheets).toEqual(after.content.sheets);

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    const undone = archive(await saveAs(page, "drawio-annotation-undo.thinoda"));
    expect(undone.content.sheets).toEqual(before.content.sheets);
});

test("draw.io paste preserves CSS colors and can be undone", async ({ page }) => {
    const text = [
        '<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>',
        '<mxCell id="shape" vertex="1" parent="1" value="Color" ',
        'style="shape=rectangle;fillColor=rgba(10,20,30,0.5);fillOpacity=50;strokeColor=blue;fontColor=green;">',
        '<mxGeometry width="100" height="60" as="geometry"/></mxCell>',
        '</root></mxGraphModel>',
    ].join("");
    const before = archive(await saveAs(page, "drawio-css-before.thinoda"));
    await page.evaluate((value) => {
        window.__thinodaTestClipboard.items = [new ClipboardItem({
            "text/plain": new Blob([value], { type: "text/plain" }),
        })];
    }, text);
    await paste(page);
    const after = archive(await saveAs(page, "drawio-css-after.thinoda"));
    const objects = after.content.sheets[after.content.active_sheet].canvas?.objects ?? [];
    expect(objects).toHaveLength(1);
    expect(objects[0].kind.shape?.label).toBe("Color");
    expect(objects[0].style).toMatchObject({
        fill: "#0A141E40",
        stroke: "#0000FF",
        text_color: "#008000",
    });

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    const undone = archive(await saveAs(page, "drawio-css-undo.thinoda"));
    expect(undone.content.sheets).toEqual(before.content.sheets);
});

test("draw.io paste omits shapes with malformed geometry", async ({ page }, testInfo) => {
    const text = [
        '<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>',
        '<mxCell id="bad" value="Bad" vertex="1" parent="1">',
        '<mxGeometry x="invalid" width="40" height="50" as="geometry"/></mxCell>',
        '<mxCell id="safe" value="Safe" vertex="1" parent="1">',
        '<mxGeometry x="100" width="40" height="50" as="geometry"/></mxCell>',
        '</root></mxGraphModel>',
    ].join("");
    const before = archive(await saveAs(page, "drawio-invalid-geometry-before.thinoda"));
    await page.evaluate((value) => {
        window.__thinodaTestClipboard.items = [new ClipboardItem({
            "text/plain": new Blob([value], { type: "text/plain" }),
        })];
    }, text);
    await paste(page);
    await page.screenshot({ path: testInfo.outputPath("drawio-invalid-geometry-warning.png") });
    const after = archive(await saveAs(page, "drawio-invalid-geometry-after.thinoda"));
    const objects = after.content.sheets[after.content.active_sheet].canvas?.objects ?? [];
    expect(objects).toHaveLength(1);
    expect(objects[0].kind.shape?.label).toBe("Safe");

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    const undone = archive(await saveAs(page, "drawio-invalid-geometry-undo.thinoda"));
    expect(undone.content.sheets).toEqual(before.content.sheets);
});

test("plain clipboard rejects excessive depth and still supports paste and undo", async ({ page }) => {
    const before = archive(await saveAs(page, "plain-depth-before.thinoda"));
    const setText = (text: string) => page.evaluate((value) => {
        window.__thinodaTestClipboard.items = [new ClipboardItem({
            "text/plain": new Blob([value], { type: "text/plain" }),
        })];
    }, text);
    let tooDeep = "Root\n";
    for (let depth = 1; depth <= 257; depth++) {
        tooDeep += "\t".repeat(depth) + "Child\n";
    }
    await setText(tooDeep);
    await paste(page);
    const rejected = archive(await saveAs(page, "plain-depth-rejected.thinoda"));
    expect(rejected.content.sheets).toEqual(before.content.sheets);

    await setText("  - Parent\n    - Child\n- Separate");
    await paste(page);
    const after = archive(await saveAs(page, "plain-depth-after.thinoda"));
    const sheet = after.content.sheets[after.content.active_sheet];
    const parent = Object.values(sheet.topics).find((topic) => topic.title === "Parent");
    const separate = Object.values(sheet.topics).find((topic) => topic.title === "Separate");
    if (!parent || !separate || !parent.children?.length) throw new Error("Missing pasted outline");
    expect(sheet.floating).toEqual(expect.arrayContaining([parent.id, separate.id]));
    expect(sheet.topics[String(parent.children[0])].title).toBe("Child");

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    const undone = archive(await saveAs(page, "plain-depth-undo.thinoda"));
    expect(undone.content.sheets).toEqual(before.content.sheets);
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+Shift+z");
    await frames(page);
    const redone = archive(await saveAs(page, "plain-depth-redo.thinoda"));
    expect(redone.content.sheets).toEqual(after.content.sheets);
});

test("topic clipboard preserves a styled branch in a separate app instance", async ({ page }) => {
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+n");
    await frames(page);
    await command(page, "Quick Style: Warning");
    const source = archive(await saveAs(page, "clipboard-topic-source.thinoda"));
    const sourceSheet = source.content.sheets[source.content.active_sheet];
    const sourceTopic = sourceSheet.topics[String(sourceSheet.root)];
    expect(sourceTopic.style?.bold).toBe(true);
    expect(sourceTopic.children?.length).toBeGreaterThan(0);

    await copyOrCut(page, "copy");
    const data = await page.evaluate(async () => {
        const item = window.__thinodaTestClipboard.items[0];
        return {
            text: await (await item.getType("text/plain")).text(),
            html: await (await item.getType("text/html")).text(),
        };
    });
    expect(data.html).toContain("data-thinoda-topic-v2");

    const target = await page.context().newPage();
    await target.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(target.locator("#loading")).toBeHidden({ timeout: 120_000 });
    await target.locator("#thinoda_canvas").focus();
    await target.keyboard.press("Control+n");
    await frames(target);
    const before = archive(await saveAs(target, "clipboard-topic-target-before.thinoda"));
    const setClipboard = (text: string, html = data.html) => target.evaluate(({ text, html }) => {
        window.__thinodaTestClipboard.items = [new ClipboardItem({
            "text/plain": new Blob([text], { type: "text/plain" }),
            "text/html": new Blob([html], { type: "text/html" }),
        })];
    }, { text, html });
    await setClipboard("Different text");
    await paste(target);
    const rejected = archive(await saveAs(target, "clipboard-topic-target-rejected.thinoda"));
    expect(rejected.content.sheets).toEqual(before.content.sheets);

    await setClipboard(data.text, data.html.slice(0, data.html.indexOf(">")));
    await paste(target);
    const malformed = archive(await saveAs(target, "clipboard-topic-target-malformed.thinoda"));
    expect(malformed.content.sheets).toEqual(before.content.sheets);

    await setClipboard(data.text);
    await paste(target);

    const after = archive(await saveAs(target, "clipboard-topic-target-after.thinoda"));
    const beforeSheet = before.content.sheets[before.content.active_sheet];
    const afterSheet = after.content.sheets[after.content.active_sheet];
    const pasted = Object.values(afterSheet.topics).find(topic =>
        !beforeSheet.topics[String(topic.id)] && topic.title === sourceTopic.title);
    expect(pasted?.style).toEqual(sourceTopic.style);
    expect(pasted?.children?.length).toBe(sourceTopic.children?.length);

    await target.locator("#thinoda_canvas").focus();
    await target.keyboard.press("Control+z");
    await frames(target);
    const undone = archive(await saveAs(target, "clipboard-topic-target-undo.thinoda"));
    expect(undone.content.sheets).toEqual(before.content.sheets);
    await target.close();
});

test("a rejected browser clipboard write does not cut board content", async ({ page }) => {
    await createShape(page, "Clipboard cut failure");
    const before = archive(await saveAs(page, "clipboard-cut-before.thinoda"));
    await page.evaluate(() => { window.__thinodaTestClipboard.rejectWrites = true; });
    await copyOrCut(page, "cut");
    const state = await page.evaluate(() => ({
        attempts: window.__thinodaTestClipboard.writeAttempts,
        writes: window.__thinodaTestClipboard.writes,
        items: window.__thinodaTestClipboard.items.length,
    }));
    expect(state).toEqual({ attempts: 1, writes: 0, items: 0 });
    const after = archive(await saveAs(page, "clipboard-cut-after.thinoda"));
    expect(after.content.sheets).toEqual(before.content.sheets);
    expect(after.manifest.resources).toEqual(before.manifest.resources);
});

test("formatted card cuts require readable, unchanged rich clipboard data", async ({ page }) => {
    const { formatted } = await formattedCardBoard(page);
    for (const mode of [
        "denied", "missing-html", "sanitized", "changed-rich", "changed-text", "malformed",
        "oversized-text", "oversized-html", "missing-read",
    ] as const) {
        await page.evaluate((mode) => {
            window.__thinodaTestClipboard.readMode = mode;
            if (mode === "missing-read") {
                Object.defineProperty(navigator.clipboard, "read", { value: undefined });
            }
        }, mode);
        const reads = await page.evaluate(() => window.__thinodaTestClipboard.reads);
        await copyOrCut(page, "cut");
        await expect.poll(() => page.evaluate(() => window.__thinodaTestClipboard.reads))
            .toBe(reads + (mode === "missing-read" ? 0 : 1));
        const after = archive(await saveAs(page, `clipboard-cut-${mode}.thinoda`));
        expect(after.content.sheets).toEqual(formatted.content.sheets);
    }
    expect(await page.evaluate(() => window.__thinodaTestClipboard.textReads)).toBe(0);
    expect(await page.evaluate(() => window.__thinodaTestClipboard.oversizedBlobReads)).toBe(0);
});

test("a timed-out browser cut ignores late clipboard writes and reads", async ({ page }) => {
    const { formatted } = await formattedCardBoard(page);
    await page.evaluate(() => {
        const original = window.setTimeout.bind(window);
        window.setTimeout = ((handler: TimerHandler, timeout?: number, ...args: unknown[]) =>
            original(handler, timeout === 30_000 ? 50 : timeout, ...args)) as typeof window.setTimeout;
    });
    for (const phase of ["write", "read"] as const) {
        await page.evaluate((phase) => {
            const state = window.__thinodaTestClipboard;
            if (phase === "write") state.holdWrite = true;
            else state.holdRead = true;
        }, phase);
        await copyOrCut(page, "cut");
        await expect.poll(() => page.evaluate((phase) => phase === "write"
            ? window.__thinodaTestClipboard.releaseWrite !== null
            : window.__thinodaTestClipboard.releaseRead !== null, phase)).toBe(true);
        await page.evaluate((phase) => {
            const state = window.__thinodaTestClipboard;
            if (phase === "write") {
                state.holdWrite = false;
                state.releaseWrite?.();
            } else {
                state.holdRead = false;
                state.releaseRead?.();
            }
        }, phase);
        await frames(page);
        await page.waitForTimeout(300);
        const after = archive(await saveAs(page, `clipboard-cut-timeout-${phase}.thinoda`));
        expect(after.content.sheets).toEqual(formatted.content.sheets);
        await page.locator("#thinoda_canvas").focus();
        await page.keyboard.press("Control+a");
        await frames(page);
    }
});

test("a formatted card branch creates a rootless sheet and supports undo and redo", async ({ page }) => {
    const { formatted, card } = await formattedCardBoard(page);
    await page.mouse.click(330, 545);
    await frames(page);
    await command(page, "New Sheet from Topic");
    const created = archive(await saveAs(page, "card-branch-sheet.thinoda"));
    expect(created.content.sheets).toHaveLength(3);
    expect(created.content.sheets.slice(0, 2)).toEqual(formatted.content.sheets);
    const sheet = created.content.sheets[2];
    expect(sheet.root_mode).toBe("hidden");
    expect(sheet.floating).toEqual([card.id]);
    expect(sheet.topics[String(card.id)].inline_text).toEqual(card.inline_text);

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    const undone = archive(await saveAs(page, "card-branch-sheet-undo.thinoda"));
    expect(undone.content.sheets).toEqual(formatted.content.sheets);

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+Shift+z");
    await frames(page);
    const redone = archive(await saveAs(page, "card-branch-sheet-redo.thinoda"));
    expect(redone.content.sheets).toEqual(created.content.sheets);
});

test("verified formatted card cut is undoable and pastes its inline formatting", async ({ page }) => {
    const { formatted, card } = await formattedCardBoard(page);
    await page.evaluate(() => { window.__thinodaTestClipboard.readMode = "wrapped"; });
    await copyOrCut(page, "cut");
    const cut = archive(await saveAs(page, "clipboard-formatted-cut.thinoda"));
    expect(cut.content.sheets[1].topics[String(card.id)]).toBeUndefined();

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    const undone = archive(await saveAs(page, "clipboard-formatted-cut-undo.thinoda"));
    expect(undone.content.sheets).toEqual(formatted.content.sheets);

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+Shift+z");
    await frames(page);
    const redone = archive(await saveAs(page, "clipboard-formatted-cut-redo.thinoda"));
    expect(redone.content.sheets).toEqual(cut.content.sheets);

    await paste(page);
    const pasted = archive(await saveAs(page, "clipboard-formatted-paste.thinoda"));
    const pastedCard = Object.values(pasted.content.sheets[1].topics)
        .find((topic) => topic.title === card.title);
    expect(pastedCard?.inline_text).toEqual(card.inline_text);
});

test("delayed cut verification cannot delete after selection, clipboard, or copy changes", async ({ page }) => {
    const { formatted } = await formattedCardBoard(page);
    for (const change of ["selection", "clipboard", "new-copy"] as const) {
        await page.evaluate(() => { window.__thinodaTestClipboard.holdRead = true; });
        await copyOrCut(page, "cut");
        await expect.poll(() => page.evaluate(() => window.__thinodaTestClipboard.releaseRead !== null))
            .toBe(true);

        if (change === "selection") {
            await page.keyboard.press("Escape");
            await frames(page);
        } else if (change === "clipboard") {
            await page.evaluate(async () => {
                const state = window.__thinodaTestClipboard;
                const text = await state.items[0].getType("text/plain");
                state.items = [new ClipboardItem({ "text/plain": text })];
            });
        } else {
            await copyOrCut(page, "copy");
        }
        await page.evaluate(() => {
            const state = window.__thinodaTestClipboard;
            state.holdRead = false;
            state.releaseRead?.();
        });
        await frames(page);
        await page.waitForTimeout(300);
        const after = archive(await saveAs(page, `clipboard-cut-delayed-${change}.thinoda`));
        expect(after.content.sheets).toEqual(formatted.content.sheets);
        await page.locator("#thinoda_canvas").focus();
        await page.keyboard.press("Control+a");
        await frames(page);
    }
});
