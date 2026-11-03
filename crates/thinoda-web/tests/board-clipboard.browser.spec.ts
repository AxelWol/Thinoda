/**
 * Browser transport coverage using an isolated, in-page clipboard.
 * The synthetic events never read or replace the system clipboard.
 */
import { expect, test, type Page } from "@playwright/test";
import { command, drag, frames, imagePicker, jpeg, saveAs, zipEntry } from "./board-browser-helpers";

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
    reads: number;
    rejectWrites: boolean;
};

declare global {
    interface Window {
        __thinodaTestClipboard: TestClipboard;
    }
}

type Topic = {
    id: number;
    title: string;
    image?: string;
    card?: { kind: string; color?: string };
};

type BoardObject = {
    id: number;
    kind: {
        shape?: { shape: string; label?: string };
        connector?: { from: unknown; to: unknown };
    };
    style: { fill?: string };
};

type BoardSheet = {
    root_mode: string;
    floating: number[];
    topics: Record<string, Topic>;
    canvas?: { objects: BoardObject[] };
};

type Resource = { id: string; name: string; size: number };

function archive(zip: Buffer) {
    return {
        zip,
        content: JSON.parse(zipEntry(zip, "content.json").toString("utf8")) as { sheets: BoardSheet[] },
        manifest: JSON.parse(zipEntry(zip, "manifest.json").toString("utf8")) as { resources: Resource[] },
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

test.beforeEach(async ({ page }) => {
    await page.addInitScript(() => {
        const state: TestClipboard = {
            items: [], writes: 0, writeAttempts: 0, plainWrites: 0, reads: 0, rejectWrites: false,
        };
        Object.defineProperty(navigator, "clipboard", {
            configurable: true,
            value: {
                async write(items: ClipboardItem[]) {
                    state.writeAttempts++;
                    if (state.rejectWrites) throw new DOMException("Clipboard denied", "NotAllowedError");
                    state.items = items;
                    state.writes++;
                },
                async writeText(text: string) {
                    state.items = [new ClipboardItem({ "text/plain": new Blob([text], { type: "text/plain" }) })];
                    state.plainWrites++;
                },
                async read() {
                    state.reads++;
                    return state.items;
                },
                async readText() {
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
