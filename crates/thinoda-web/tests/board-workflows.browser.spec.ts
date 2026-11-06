/**
 * End-to-end board editing against a freshly built production web shell.
 * The native archive is inspected rather than inferring edits from pixels.
 */
import { expect, test } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { colorBounds, command, drag, frames, saveAs, touchEvents, withPdfViewer, zipEntry } from "./board-browser-helpers";

test.use({
    viewport: { width: 1252, height: 1278 },
    deviceScaleFactor: 1,
    serviceWorkers: "block",
    screenshot: "only-on-failure",
});
test.setTimeout(180_000);

type BoardSheet = {
    title: string;
    root_mode: string;
    floating: number[];
    topics: Record<string, {
        id: number;
        title: string;
        position?: { x: number; y: number };
        image?: string;
        card?: { kind: string; color?: string };
    }>;
    zones: {
        name: string;
        roots?: number[];
        keep_empty?: boolean;
        frame: { min: { x: number; y: number }; size: { x: number; y: number } };
    }[];
    canvas: {
        objects: {
            id: number;
            kind: {
                shape?: { shape: string; label?: string };
                connector?: { from: unknown; to: unknown; label?: string; route?: string; arrow_end?: boolean };
                ink?: { tool: string; color: string; width: number; points: { x: number; y: number }[]; pressure?: number[] };
            };
            rect: { min: { x: number; y: number }; size: { x: number; y: number } };
            band?: string;
            alt_text?: string;
            style: { fill?: string; text_color?: string; stroke_width?: number; opacity?: number };
        }[];
    };
};

function sheetFrom(zip: Buffer): BoardSheet {
    const content: { sheets: BoardSheet[] } = JSON.parse(zipEntry(zip, "content.json").toString("utf8"));
    return content.sheets[0];
}

test.describe("storyboard gallery", () => {
    test.use({ viewport: { width: 1280, height: 800 } });

    test("exports a filled scene to PDF and restores it through undo and redo", async ({ page }) => {
        await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
        await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
        const canvas = page.locator("#thinoda_canvas");
        await frames(page);
        for (const position of [
            { x: 220, y: 128 },
            { x: 220, y: 225 },
            { x: 115, y: 532 },
            { x: 67, y: 722 },
        ]) {
            await canvas.click({ position });
            await frames(page);
        }

        const baseline = await saveAs(page, "storyboard-baseline.thinoda");
        const initial = sheetFrom(baseline);
        const manifest = (zip: Buffer): { resources: { id: string; name: string; media_type: string; size: number }[] } =>
            JSON.parse(zipEntry(zip, "manifest.json").toString("utf8"));
        expect(initial.title).toBe("Storyboard");
        expect(initial.root_mode).toBe("hidden");
        expect(initial.zones.map(zone => zone.name)).toEqual(["Scene 1", "Scene 2", "Scene 3"]);
        expect(initial.zones.every(zone => zone.keep_empty && !zone.roots?.length)).toBe(true);
        const images = Object.values(initial.topics).filter(topic => topic.card?.kind === "image");
        expect(images.map(image => image.title)).toEqual(["Scene 1 image", "Scene 2 image", "Scene 3 image"]);
        expect(images.every(image => !image.image)).toBe(true);
        expect(manifest(baseline).resources).toHaveLength(0);

        const fixture = await page.evaluate(() => {
            const image = document.createElement("canvas");
            image.width = image.height = 64;
            const context = image.getContext("2d");
            if (!context) throw new Error("Cannot create the image fixture");
            context.fillStyle = "#12c478";
            context.fillRect(0, 0, 64, 64);
            return image.toDataURL("image/jpeg", 0.95).split(",")[1];
        });
        const bytes = Buffer.from(fixture, "base64");
        await canvas.click({ position: { x: 290, y: 400 } });
        await frames(page);
        const pending = page.waitForEvent("filechooser");
        await command(page, "Local Image");
        await (await pending).setFiles({ name: "storyboard-scene.jpg", mimeType: "image/jpeg", buffer: bytes });
        await page.waitForTimeout(500);

        const uploaded = await saveAs(page, "storyboard-upload.thinoda");
        const updated = sheetFrom(uploaded);
        const resources = manifest(uploaded).resources;
        expect(resources).toHaveLength(1);
        expect(resources[0]).toMatchObject({
            name: "storyboard-scene.jpg",
            media_type: "image/jpeg",
            size: bytes.length,
        });
        expect(updated.topics[String(images[0].id)].image).toBe(`thinoda-resource:${resources[0].id}`);
        expect(updated.topics[String(images[1].id)].image).toBeUndefined();
        expect(updated.topics[String(images[2].id)].image).toBeUndefined();
        expect(zipEntry(uploaded, `resources/${resources[0].id}`)).toEqual(bytes);

        await page.evaluate(() => { window.prompt = () => "storyboard.pdf"; });
        await command(page, "Export PDF");
        await page.mouse.move(879, 476);
        await expect.poll(() => canvas.evaluate(element => (element as HTMLCanvasElement).style.cursor))
            .toBe("pointer");
        const pendingPdf = page.waitForEvent("download", { timeout: 30_000 });
        await page.mouse.click(879, 476);
        const download = await pendingPdf;
        expect(download.suggestedFilename()).toBe("storyboard.pdf");
        const pdf = await readFile(await download.path());
        expect(pdf.subarray(0, 5).toString("ascii")).toBe("%PDF-");
        await withPdfViewer(pdf, async (viewer) => {
            const colors = [
                [18, 196, 120],
                [238, 247, 254], [241, 249, 241], [255, 247, 235],
                [255, 229, 143], [165, 216, 255], [183, 235, 143],
            ];
            const inspect = async () => colorBounds(page, await viewer.screenshot(), colors, 4);
            await expect.poll(async () => (await inspect()).every(region => region.count > 500), {
                timeout: 30_000,
                intervals: [500, 1_000, 2_000],
            }).toBe(true);
            const [picture, ...regions] = await inspect();
            const [first, second, third, yellow, blue, green] = regions;
            expect(first.maxX).toBeLessThan(second.minX);
            expect(second.maxX).toBeLessThan(third.minX);
            for (const [frame, note] of [[first, yellow], [second, blue], [third, green]]) {
                expect(note.minX).toBeGreaterThan(frame.minX);
                expect(note.maxX).toBeLessThan(frame.maxX);
                expect(note.minY).toBeGreaterThan(frame.minY);
                expect(note.maxY).toBeLessThan(frame.maxY);
            }
            expect(picture.minX).toBeGreaterThan(first.minX);
            expect(picture.maxX).toBeLessThan(first.maxX);
            expect(picture.minY).toBeGreaterThan(first.minY);
            expect(picture.maxY).toBeLessThan(yellow.minY);
        });
        const afterExport = await saveAs(page, "storyboard-after-export.thinoda");
        expect(sheetFrom(afterExport)).toEqual(updated);
        expect(manifest(afterExport).resources).toEqual(resources);
        expect(zipEntry(afterExport, `resources/${resources[0].id}`)).toEqual(bytes);

        await page.keyboard.press("Control+z");
        await frames(page);
        const undone = await saveAs(page, "storyboard-undone.thinoda");
        expect(sheetFrom(undone)).toEqual(initial);
        expect(manifest(undone).resources).toHaveLength(0);

        await page.keyboard.press("Control+Shift+z");
        await frames(page);
        const redone = await saveAs(page, "storyboard-redone.thinoda");
        expect(sheetFrom(redone)).toEqual(updated);
        expect(manifest(redone).resources).toEqual(resources);
        expect(zipEntry(redone, `resources/${resources[0].id}`)).toEqual(bytes);
    });
});

test.describe("moodboard gallery", () => {
    test.use({ viewport: { width: 1280, height: 800 } });

    test("persists an uploaded image through undo, redo and reopen", async ({ page }) => {
        await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
        await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
        const canvas = page.locator("#thinoda_canvas");
        await frames(page);
        for (const position of [
            { x: 220, y: 128 },
            { x: 220, y: 225 },
            { x: 465, y: 532 },
            { x: 67, y: 722 },
        ]) {
            await canvas.click({ position });
            await frames(page);
        }

        const manifest = (zip: Buffer): { resources: { id: string; name: string; media_type: string; size: number }[] } =>
            JSON.parse(zipEntry(zip, "manifest.json").toString("utf8"));
        const baseline = await saveAs(page, "moodboard-baseline.thinoda");
        const initial = sheetFrom(baseline);
        const images = Object.values(initial.topics).filter(topic => topic.card?.kind === "image");
        expect(initial.title).toBe("Moodboard");
        expect(initial.root_mode).toBe("hidden");
        expect(initial.zones).toEqual([]);
        expect(images.map(image => image.title)).toEqual(
            Array.from({ length: 6 }, (_, index) => `Image ${index + 1}`),
        );
        expect(images.every(image => !image.image)).toBe(true);
        expect(manifest(baseline).resources).toHaveLength(0);

        const fixture = await page.evaluate(() => {
            const image = document.createElement("canvas");
            image.width = image.height = 64;
            const context = image.getContext("2d");
            if (!context) throw new Error("Cannot create the image fixture");
            context.fillStyle = "#12c478";
            context.fillRect(0, 0, 64, 64);
            return image.toDataURL("image/png").split(",")[1];
        });
        const bytes = Buffer.from(fixture, "base64");
        await canvas.click({ position: { x: 235, y: 320 } });
        await frames(page);
        const pending = page.waitForEvent("filechooser");
        await command(page, "Local Image");
        await (await pending).setFiles({ name: "inspiration.png", mimeType: "image/png", buffer: bytes });
        await page.waitForTimeout(500);

        const uploaded = await saveAs(page, "moodboard-upload.thinoda");
        const updated = sheetFrom(uploaded);
        const resources = manifest(uploaded).resources;
        expect(resources).toHaveLength(1);
        expect(resources[0]).toMatchObject({
            name: "inspiration.png",
            media_type: "image/png",
            size: bytes.length,
        });
        expect(updated.topics[String(images[0].id)].image).toBe(`thinoda-resource:${resources[0].id}`);
        expect(images.slice(1).every(image => !updated.topics[String(image.id)].image)).toBe(true);
        expect(zipEntry(uploaded, `resources/${resources[0].id}`)).toEqual(bytes);

        await page.keyboard.press("Control+z");
        await frames(page);
        const undone = await saveAs(page, "moodboard-undone.thinoda");
        expect(sheetFrom(undone)).toEqual(initial);
        expect(manifest(undone).resources).toHaveLength(0);

        await page.keyboard.press("Control+Shift+z");
        await frames(page);
        const redone = await saveAs(page, "moodboard-redone.thinoda");
        expect(sheetFrom(redone)).toEqual(updated);
        expect(manifest(redone).resources).toEqual(resources);
        expect(zipEntry(redone, `resources/${resources[0].id}`)).toEqual(bytes);

        await page.reload({ waitUntil: "domcontentloaded" });
        await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
        await page.evaluate(() => {
            Object.defineProperty(window, "showOpenFilePicker", { configurable: true, value: undefined });
            const read = File.prototype.arrayBuffer;
            let delayUpload = true;
            File.prototype.arrayBuffer = async function (this: File) {
                if (delayUpload && this.name === "moodboard-redone.thinoda") {
                    delayUpload = false;
                    Reflect.set(window, "__moodboardUploadDelayed", true);
                    await new Promise(resolve => setTimeout(resolve, 3_000));
                }
                return read.call(this);
            };
        });
        const reopen = page.waitForEvent("filechooser");
        await command(page, "Open");
        await (await reopen).setFiles({
            name: "moodboard-redone.thinoda",
            mimeType: "application/octet-stream",
            buffer: redone,
        });
        // The picker returns before storage, parsing, and rendering finish.
        // Wait for the distinctive uploaded image rather than racing Save As.
        await expect.poll(async () => {
            const screenshot = await page.screenshot({ clip: { x: 210, y: 282, width: 54, height: 54 } });
            const [image] = await colorBounds(page, screenshot, [[18, 196, 120]], 6);
            return image.count;
        }, { timeout: 30_000 }).toBeGreaterThan(1_000);
        expect(await page.evaluate(() => Reflect.get(window, "__moodboardUploadDelayed"))).toBe(true);
        const reopened = await saveAs(page, "moodboard-reopened.thinoda");
        expect(sheetFrom(reopened)).toEqual(updated);
        expect(manifest(reopened).resources).toEqual(resources);
        expect(zipEntry(reopened, `resources/${resources[0].id}`)).toEqual(bytes);
    });
});

test.describe("user journey map gallery", () => {
    test.use({ viewport: { width: 1280, height: 800 } });

    test("creates a rootless journey with independent prompts and stage frames", async ({ page }) => {
        await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
        await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
        const canvas = page.locator("#thinoda_canvas");
        await frames(page);
        for (const position of [
            { x: 220, y: 128 },
            { x: 220, y: 225 },
            { x: 290, y: 532 },
            { x: 67, y: 722 },
        ]) {
            await canvas.click({ position });
            await frames(page);
        }

        const saved = sheetFrom(await saveAs(page, "user-journey.thinoda"));
        expect(saved.title).toBe("User Journey Map");
        expect(saved.root_mode).toBe("hidden");
        expect(saved.zones.map(zone => zone.name)).toEqual(["Stage 1", "Stage 2", "Stage 3"]);
        expect(saved.zones.every(zone => zone.keep_empty && !zone.roots?.length)).toBe(true);
        const cards = Object.values(saved.topics).filter(topic => topic.card);
        expect(cards.map(card => [card.title, card.card?.kind])).toEqual([
            ["Journey: person and scenario", "text"],
            ["User actions", "text"],
            ["Feelings", "text"],
            ["Opportunities", "text"],
            ...Array.from({ length: 3 }, () => ["What do they do?", "sticky"]),
            ...Array.from({ length: 3 }, () => ["How might they feel?", "sticky"]),
            ...Array.from({ length: 3 }, () => ["What could be improved?", "sticky"]),
        ]);
        expect(saved.floating).toEqual(cards.map(card => card.id));
        expect(cards.slice(4).map(card => card.position)).toEqual(
            [130, 260, 390].flatMap(y => [160, 500, 840].map(x => ({ x, y }))),
        );
    });
});

test.describe("meeting board gallery", () => {
    test.use({ viewport: { width: 1280, height: 800 } });

    test("creates the selected board with independent cards and persistent frames", async ({ page }) => {
        await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
        await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
        const canvas = page.locator("#thinoda_canvas");
        await frames(page);
        // The egui gallery currently exposes only one browser canvas.
        const categoryFilter = { x: 220, y: 128 };
        const boardsCategory = { x: 220, y: 225 };
        const meetingTemplate = { x: 640, y: 394 };
        const createButton = { x: 67, y: 722 };
        for (const position of [categoryFilter, boardsCategory, meetingTemplate, createButton]) {
            await canvas.click({ position });
            await frames(page);
        }

        const saved = sheetFrom(await saveAs(page, "meeting-board.thinoda"));
        expect(saved.title).toBe("Meeting Board");
        expect(saved.root_mode).toBe("hidden");
        expect(saved.zones.map(zone => zone.name)).toEqual(["Agenda", "Notes", "Decisions", "Actions"]);
        expect(saved.zones.every(zone => zone.keep_empty && !zone.roots?.length)).toBe(true);
        const cards = Object.values(saved.topics).filter(topic => topic.card);
        expect(cards.map(card => [card.title, card.card?.kind])).toEqual([
            ["Meeting: purpose and date", "text"],
            ["Topic and timebox", "sticky"],
            ["Key points and questions", "sticky"],
            ["Decision and reason", "sticky"],
            ["Action, owner and due date", "sticky"],
        ]);
        expect(saved.floating).toEqual(cards.map(card => card.id));
    });
});

test.describe("swimlane gallery", () => {
    test.use({ viewport: { width: 1280, height: 800 } });

    test("creates editable handoffs in persistent visual role lanes", async ({ page }) => {
        await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
        await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
        const canvas = page.locator("#thinoda_canvas");
        await frames(page);
        for (const position of [
            { x: 220, y: 128 },
            { x: 220, y: 225 },
            { x: 815, y: 394 },
            { x: 67, y: 722 },
        ]) {
            await canvas.click({ position });
            await frames(page);
        }

        await drag(page, [210, 316], [210, 459], 12);
        const saved = sheetFrom(await saveAs(page, "swimlane.thinoda"));
        expect(saved.title).toBe("Swimlane");
        expect(saved.root_mode).toBe("hidden");
        expect(saved.zones.map(zone => zone.name)).toEqual(["Role 1", "Role 2", "Role 3"]);
        expect(saved.zones.every(zone => zone.keep_empty && !zone.roots?.length)).toBe(true);
        const shapes = saved.canvas.objects.filter(object => object.kind.shape);
        expect(shapes.map(object => object.kind.shape?.label)).toEqual(["Start", "Step", "Step", "Action", "End"]);
        const moved = shapes[0].rect;
        const lane = saved.zones[1].frame;
        expect(moved.min.x).toBeGreaterThanOrEqual(lane.min.x);
        expect(moved.min.y).toBeGreaterThanOrEqual(lane.min.y);
        expect(moved.min.x + moved.size.x).toBeLessThanOrEqual(lane.min.x + lane.size.x);
        expect(moved.min.y + moved.size.y).toBeLessThanOrEqual(lane.min.y + lane.size.y);
        const connectors = saved.canvas.objects.flatMap(object => object.kind.connector ? [object.kind.connector] : []);
        expect(connectors).toHaveLength(4);
        for (let index = 0; index < connectors.length; index++) {
            expect(connectors[index]).toMatchObject({
                from: { object: shapes[index].id },
                to: { object: shapes[index + 1].id },
                route: "elbow",
                arrow_end: true,
            });
        }
    });
});

test.describe("venn diagram gallery", () => {
    test.use({ viewport: { width: 1280, height: 800 } });

    test("creates editable overlapping circles and independent labels", async ({ page }) => {
        await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
        await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
        const canvas = page.locator("#thinoda_canvas");
        await frames(page);
        for (const position of [
            { x: 220, y: 128 },
            { x: 220, y: 225 },
            { x: 990, y: 394 },
            { x: 67, y: 722 },
        ]) {
            await canvas.click({ position });
            await frames(page);
        }

        await drag(page, [210, 461], [250, 461]);
        await command(page, "Quick Style: Warning");
        const saved = sheetFrom(await saveAs(page, "venn-diagram.thinoda"));
        expect(saved.title).toBe("Venn Diagram");
        expect(saved.root_mode).toBe("hidden");
        expect(saved.zones).toHaveLength(0);
        expect(saved.canvas.objects).toHaveLength(2);
        const [left, right] = saved.canvas.objects;
        expect(left.kind.shape?.shape).toBe("ellipse");
        expect(right.kind.shape?.shape).toBe("ellipse");
        expect([left.band, right.band]).toEqual(["below", "below"]);
        expect([left.alt_text, right.alt_text]).toEqual(["Set A", "Set B"]);
        expect([left.style.opacity, right.style.opacity]).toEqual([0.55, 0.55]);
        expect(left.style.fill).toBe("#FFEDD5");
        expect(left.rect.min.x).toBeCloseTo(40, 1);
        expect(left.rect.min.y).toBeCloseTo(0, 1);
        expect(right.rect.min).toEqual({ x: 300, y: 0 });
        const cards = Object.values(saved.topics).filter(topic => topic.card);
        expect(cards.map(card => [card.title, card.card?.kind, card.position])).toEqual([
            ["Compare two sets", "text", { x: 390, y: -70 }],
            ["Set A", "text", { x: 240, y: 140 }],
            ["Set B", "text", { x: 540, y: 140 }],
            ["Shared", "sticky", { x: 390, y: 265 }],
        ]);
    });
});

test.describe("empathy map gallery", () => {
    test.use({ viewport: { width: 1280, height: 800 } });

    test("creates editable prompts in independent visual sections", async ({ page }) => {
        await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
        await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
        const canvas = page.locator("#thinoda_canvas");
        await frames(page);
        for (const position of [
            { x: 220, y: 128 },
            { x: 220, y: 225 },
            { x: 1165, y: 394 },
            { x: 67, y: 722 },
        ]) {
            await canvas.click({ position });
            await frames(page);
        }

        await drag(page, [175, 310], [800, 400], 12);
        const saved = sheetFrom(await saveAs(page, "empathy-map.thinoda"));
        expect(saved.title).toBe("Empathy Map");
        expect(saved.root_mode).toBe("hidden");
        expect(saved.zones.map(zone => zone.name)).toEqual(["Says", "Thinks", "Does", "Feels"]);
        expect(saved.zones.every(zone => zone.keep_empty && !zone.roots?.length)).toBe(true);
        const cards = Object.values(saved.topics).filter(topic => topic.card);
        expect(cards.map(card => [card.title, card.card?.kind])).toEqual([
            ["Empathy map: whose perspective?", "text"],
            ["What do they say?", "sticky"],
            ["What might they think?", "sticky"],
            ["What do they do?", "sticky"],
            ["How might they feel?", "sticky"],
        ]);
        const moved = cards[1].position;
        if (!moved) throw new Error("Saved prompt has no position");
        const target = saved.zones[1].frame;
        expect(moved.x - 75).toBeGreaterThanOrEqual(target.min.x);
        expect(moved.x + 75).toBeLessThanOrEqual(target.min.x + target.size.x);
        expect(moved.y - 50).toBeGreaterThanOrEqual(target.min.y);
        expect(moved.y + 50).toBeLessThanOrEqual(target.min.y + target.size.y);
        expect(cards.slice(2).map(card => card.position)).toEqual([
            { x: 565, y: 120 },
            { x: 105, y: 440 },
            { x: 565, y: 440 },
        ]);
        expect(saved.floating).toEqual(cards.map(card => card.id));
    });
});

test("browser-synthesized pen input creates one saved ink stroke", async ({ page, browserName }) => {
    test.skip(browserName !== "chromium", "Pen input synthesis requires the Chromium DevTools protocol");
    await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    const canvas = page.locator("#thinoda_canvas");
    await canvas.focus();
    await page.keyboard.press("Control+n");
    await frames(page);
    await command(page, "Hide central topic");
    await page.keyboard.press("Escape");
    await frames(page);
    await command(page, "Pen");

    await canvas.evaluate((element) => {
        if (!(element instanceof HTMLCanvasElement)) throw new Error("Missing board canvas");
        const events: { type: string; pointerType: string; pressure: number; trusted: boolean }[] = [];
        for (const type of ["pointerdown", "pointermove", "pointerup"] as const) {
            element.addEventListener(type, (event) => {
                events.push({
                    type: event.type,
                    pointerType: event.pointerType,
                    pressure: event.pressure,
                    trusted: event.isTrusted,
                });
                element.dataset.testPenEvents = JSON.stringify(events);
            });
        }
    });
    const bounds = await canvas.boundingBox();
    if (!bounds) throw new Error("Board canvas has no bounds");
    const point = (x: number, y: number) => ({
        x: bounds.x + bounds.width * x,
        y: bounds.y + bounds.height * y,
    });
    const cdp = await page.context().newCDPSession(page);
    await cdp.send("Input.dispatchMouseEvent", {
        type: "mousePressed", ...point(0.30, 0.62),
        button: "left", buttons: 1, clickCount: 1, pointerType: "pen", force: 0.2,
    });
    await frames(page);
    await cdp.send("Input.dispatchMouseEvent", {
        type: "mouseMoved", ...point(0.42, 0.68),
        button: "left", buttons: 1, pointerType: "pen", force: 0.55,
    });
    await cdp.send("Input.dispatchMouseEvent", {
        type: "mouseMoved", ...point(0.47, 0.65),
        button: "left", buttons: 1, pointerType: "pen", force: 0.85,
    });
    await cdp.send("Input.dispatchMouseEvent", {
        type: "mouseMoved", ...point(0.50, 0.70),
        button: "left", buttons: 1, pointerType: "pen", force: 0.35,
    });
    await frames(page);
    await cdp.send("Input.dispatchMouseEvent", {
        type: "mouseReleased", ...point(0.54, 0.74),
        button: "left", buttons: 0, clickCount: 1, pointerType: "pen", force: 0,
    });
    await cdp.detach();
    await frames(page);

    const events = await canvas.evaluate((element) => {
        if (!(element instanceof HTMLCanvasElement)) throw new Error("Missing board canvas");
        return JSON.parse(element.dataset.testPenEvents ?? "[]") as {
            type: string; pointerType: string; pressure: number; trusted: boolean;
        }[];
    });
    expect(events.map((event) => event.type)).toEqual([
        "pointerdown", "pointermove", "pointermove", "pointermove", "pointerup",
    ]);
    expect(events.every((event) => event.trusted && event.pointerType === "pen")).toBe(true);
    expect(events[0].pressure).toBeCloseTo(0.2, 2);
    expect(events[1].pressure).toBeCloseTo(0.55, 2);

    const saved = sheetFrom(await saveAs(page, "synthetic-pen.thinoda"));
    const inkObjects = saved.canvas?.objects.filter((object) => object.kind.ink) ?? [];
    expect(inkObjects).toHaveLength(1);
    const ink = inkObjects[0].kind.ink;
    expect(ink).toBeDefined();
    expect(ink!.tool).toBe("pen");
    expect(ink!.points).toHaveLength(events.length);
    expect(ink!.width).toBeGreaterThan(0);
    expect(ink!.pressure).toHaveLength(ink!.points.length);
    for (const [index, event] of events.entries()) {
        expect(ink!.pressure![index]).toBeCloseTo(event.pressure, 2);
    }

    await page.keyboard.press("Control+z");
    await frames(page);
    expect(sheetFrom(await saveAs(page, "synthetic-pen-undo.thinoda")).canvas?.objects ?? []).toHaveLength(0);
    await page.keyboard.press("Control+Shift+z");
    await frames(page);
    expect(sheetFrom(await saveAs(page, "synthetic-pen-redo.thinoda"))).toEqual(saved);

    await command(page, "Pen");
    await drag(page, [300, 1030], [600, 1100], 4);
    const mixed = sheetFrom(await saveAs(page, "pen-then-mouse.thinoda"));
    const mixedInk = mixed.canvas.objects.filter((object) => object.kind.ink);
    expect(mixedInk).toHaveLength(2);
    expect(mixedInk[0].kind.ink?.pressure).toEqual(ink!.pressure);
    expect(mixedInk[1].kind.ink?.pressure).toBeUndefined();
});

test("pen cancellation and blur retain input without contaminating the next mouse stroke", async ({ page, browserName }) => {
    test.skip(browserName !== "chromium", "Pen input synthesis requires the Chromium DevTools protocol");
    await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    const canvas = page.locator("#thinoda_canvas");
    await canvas.focus();
    await page.keyboard.press("Control+n");
    await frames(page);
    await command(page, "Hide central topic");
    await page.keyboard.press("Escape");
    await frames(page);
    await command(page, "Pen");
    await canvas.evaluate(element => {
        if (!(element instanceof HTMLCanvasElement)) throw new Error("Missing board canvas");
        element.addEventListener("pointerdown", event => {
            if (event.pointerType === "pen") element.setAttribute("data-test-pen-id", String(event.pointerId));
        });
    });

    const cdp = await page.context().newCDPSession(page);
    await cdp.send("Input.dispatchMouseEvent", {
        type: "mousePressed", x: 350, y: 800, button: "left", buttons: 1,
        clickCount: 1, pointerType: "pen", force: 0.2,
    });
    await frames(page);
    await cdp.send("Input.dispatchMouseEvent", {
        type: "mouseMoved", x: 500, y: 850, button: "left", buttons: 1,
        pointerType: "pen", force: 0.6,
    });
    await frames(page);
    await canvas.evaluate(element => {
        if (!(element instanceof HTMLCanvasElement)) throw new Error("Missing board canvas");
        const pointerId = Number(element.getAttribute("data-test-pen-id"));
        if (!pointerId) throw new Error("The browser did not deliver the pen press");
        element.dispatchEvent(new PointerEvent("pointercancel", {
            bubbles: true, pointerId, pointerType: "pen", isPrimary: true,
        }));
    });
    await frames(page);

    const discardBlockedStroke = async () => {
        await page.evaluate(() => { window.prompt = () => "unexpected-partial-ink.thinoda"; });
        const unexpectedDownload = page.waitForEvent("download", { timeout: 2_000 })
            .then(() => true, () => false);
        await canvas.focus();
        await page.keyboard.press("Control+Shift+s");
        expect(await unexpectedDownload).toBe(false);
        await canvas.focus();
        await page.keyboard.press("Escape");
        await frames(page);
    };
    await discardBlockedStroke();
    await cdp.send("Input.dispatchMouseEvent", {
        type: "mouseReleased", x: 500, y: 850, button: "left", buttons: 0,
        clickCount: 1, pointerType: "pen", force: 0,
    });
    await frames(page);

    await command(page, "Pen");
    await cdp.send("Input.dispatchMouseEvent", {
        type: "mousePressed", x: 350, y: 800, button: "left", buttons: 1,
        clickCount: 1, pointerType: "pen", force: 0.3,
    });
    await frames(page);
    await cdp.send("Input.dispatchMouseEvent", {
        type: "mouseMoved", x: 500, y: 850, button: "left", buttons: 1,
        pointerType: "pen", force: 0.7,
    });
    await frames(page);
    await page.evaluate(() => {
        window.dispatchEvent(new Event("blur"));
        window.dispatchEvent(new Event("focus"));
    });
    await frames(page);
    await discardBlockedStroke();
    await cdp.send("Input.dispatchMouseEvent", {
        type: "mouseReleased", x: 500, y: 850, button: "left", buttons: 0,
        clickCount: 1, pointerType: "pen", force: 0,
    });
    await cdp.detach();
    await frames(page);

    await command(page, "Pen");
    await drag(page, [300, 1030], [600, 1100], 4);
    const saved = sheetFrom(await saveAs(page, "after-pen-cancel.thinoda"));
    const strokes = saved.canvas.objects.filter(object => object.kind.ink);
    expect(strokes).toHaveLength(1);
    expect(strokes[0].kind.ink?.pressure).toBeUndefined();
});

test("rootless board objects, styling, connections and ink survive a native save", async ({ page }) => {
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
    await page.locator("input[type='text']").pressSequentially("Browser idea");
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
    await page.locator("input[type='text']").pressSequentially("Browser shape");
    await page.keyboard.press("Enter");
    await frames(page);

    await command(page, "Connector tool");
    await drag(page, [260, 400], [610, 400]);
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("F2");
    await frames(page);
    await expect(page.locator("input[type='text']")).toBeFocused();
    await page.locator("input[type='text']").pressSequentially("Browser connector");
    await page.keyboard.press("Enter");
    await frames(page);

    await command(page, "Frame tool");
    await drag(page, [300, 750], [850, 1000]);

    await command(page, "Pen");
    await drag(page, [450, 850], [600, 900], 12);

    const saved = sheetFrom(await saveAs(page, "browser-board.thinoda"));
    expect(saved.root_mode).toBe("hidden");
    const cards = Object.values(saved.topics).filter(topic => topic.card?.kind === "sticky");
    expect(cards).toHaveLength(1);
    expect(cards[0].title).toBe("Browser idea");
    expect(saved.floating).toContain(cards[0].id);
    expect(cards[0].card?.color).toBe("#FFEDD5");
    const shape = saved.canvas.objects.find(object => object.kind.shape);
    expect(shape?.kind.shape?.shape).toBe("rectangle");
    expect(shape?.kind.shape?.label).toBe("Browser shape");
    expect(shape?.style.fill).toBe("#FFEDD5");
    expect(shape?.style.text_color).toBe("#7C2D12");
    const connector = saved.canvas.objects.find(object => object.kind.connector)?.kind.connector;
    expect(connector?.from).toEqual({ topic: cards[0].id });
    expect(connector?.to).toEqual({ object: shape?.id });
    expect(connector?.label).toBe("Browser connector");
    expect(saved.zones.some(zone => zone.keep_empty)).toBe(true);
    expect(saved.canvas.objects.find(object => object.kind.ink)?.kind.ink?.points.length)
        .toBeGreaterThan(1);
    expect(saved.canvas.objects.find(object => object.kind.ink)?.kind.ink?.pressure)
        .toBeUndefined();

    await page.keyboard.press("Control+z");
    await frames(page);
    const undone = sheetFrom(await saveAs(page, "browser-board-undo.thinoda"));
    expect(undone.canvas.objects.some(object => object.kind.ink)).toBe(false);
    expect(undone.canvas.objects).toEqual(saved.canvas.objects.filter(object => !object.kind.ink));
    expect(undone.topics).toEqual(saved.topics);
    expect(undone.zones).toEqual(saved.zones);
});

test("keyboard creation, labeling and movement work on a rootless board", async ({ page }) => {
    await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    const canvas = page.locator("#thinoda_canvas");
    const input = page.locator("input[type='text']");
    await canvas.focus();
    await page.keyboard.press("Control+n");
    await frames(page);
    await command(page, "Hide central topic");
    await page.keyboard.press("Escape");
    await frames(page);

    await command(page, "Sticky note");
    await canvas.focus();
    await page.keyboard.press("Enter");
    await frames(page);
    await expect(input).toBeFocused();
    await input.pressSequentially("Keyboard idea");
    await page.keyboard.press("Enter");
    await frames(page);
    await page.keyboard.press("Escape");
    await frames(page);

    await command(page, "Shape tool");
    await canvas.focus();
    await page.keyboard.press("Enter");
    await frames(page);
    await page.keyboard.press("Escape");
    await frames(page);
    await page.keyboard.press("F2");
    await frames(page);
    await expect(input).toBeFocused();
    await input.pressSequentially("Keyboard shape");
    await page.keyboard.press("Enter");
    await frames(page);

    const baseline = sheetFrom(await saveAs(page, "keyboard-board.thinoda"));
    expect(baseline.root_mode).toBe("hidden");
    const sticky = Object.values(baseline.topics).find(topic => topic.card?.kind === "sticky");
    const shape = baseline.canvas.objects.find(object => object.kind.shape);
    expect(sticky?.title).toBe("Keyboard idea");
    expect(shape?.kind.shape).toMatchObject({ shape: "rectangle", label: "Keyboard shape" });
    if (!shape) throw new Error("Keyboard shape was not saved");

    await canvas.focus();
    await page.keyboard.press("Shift+ArrowRight");
    await frames(page);
    const moved = sheetFrom(await saveAs(page, "keyboard-board-moved.thinoda"));
    const movedShape = moved.canvas.objects.find(object => object.id === shape.id);
    expect(movedShape?.rect.min.x).toBeCloseTo(shape.rect.min.x + 10, 4);
    expect(movedShape?.rect.min.y).toBeCloseTo(shape.rect.min.y, 4);

    await canvas.focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    expect(sheetFrom(await saveAs(page, "keyboard-board-undo.thinoda"))).toEqual(baseline);
    await canvas.focus();
    await page.keyboard.press("Control+Shift+z");
    await frames(page);
    expect(sheetFrom(await saveAs(page, "keyboard-board-redo.thinoda"))).toEqual(moved);
});

test("a fast mouse drag commits its full geometry", async ({ page }) => {
    await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+n");
    await frames(page);
    await command(page, "Hide central topic");
    await page.keyboard.press("Escape");
    await frames(page);
    await command(page, "Shape tool");

    await page.mouse.move(300, 500);
    await page.mouse.down();
    // Both events enter eframe before its pointerup handler runs the next UI frame.
    await page.evaluate(() => {
        document.dispatchEvent(new MouseEvent("mousemove", {
            bubbles: true, clientX: 650, clientY: 700, buttons: 1,
        }));
        document.dispatchEvent(new PointerEvent("pointerup", {
            bubbles: true, clientX: 650, clientY: 700, button: 0, pointerType: "mouse",
        }));
    });
    await page.mouse.up();
    await frames(page);

    const sheet = sheetFrom(await saveAs(page, "fast-mouse-board.thinoda"));
    const shapes = sheet.canvas?.objects.filter((object) => object.kind.shape) ?? [];
    expect(shapes).toHaveLength(1);
    expect(shapes[0].rect.size.x).toBeGreaterThan(300);
    expect(shapes[0].rect.size.y).toBeGreaterThan(150);
});

test("pixel eraser splits a stroke and preserves one-step undo and redo", async ({ page }) => {
    await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+n");
    await frames(page);
    await command(page, "Hide central topic");
    await page.keyboard.press("Escape");
    await frames(page);

    await command(page, "Pen");
    await drag(page, [300, 850], [800, 850], 20);
    const before = sheetFrom(await saveAs(page, "pixel-erase-before.thinoda"));
    const original = before.canvas.objects.filter((object) => object.kind.ink);
    expect(before.root_mode).toBe("hidden");
    expect(original).toHaveLength(1);
    const stroke = original[0].kind.ink;
    if (!stroke) throw new Error("Missing original stroke");
    expect(stroke.points.length).toBeGreaterThanOrEqual(2);

    await command(page, "Pixel eraser");
    await drag(page, [550, 830], [550, 870]);
    const after = sheetFrom(await saveAs(page, "pixel-erase-after.thinoda"));
    const fragments = after.canvas.objects.filter((object) => object.kind.ink);
    expect(fragments).toHaveLength(2);
    const first = fragments[0].kind.ink;
    const second = fragments[1].kind.ink;
    if (!first || !second) throw new Error("Missing pixel-erase fragments");
    expect(fragments[0].id).toBe(original[0].id);
    expect(fragments[1].id).not.toBe(original[0].id);
    expect(first.points.length).toBeGreaterThanOrEqual(2);
    expect(second.points.length).toBeGreaterThanOrEqual(2);
    expect(first.points[0]).toEqual(stroke.points[0]);
    expect(second.points.at(-1)).toEqual(stroke.points.at(-1));
    const gap = second.points[0].x - first.points.at(-1)!.x;
    expect(gap).toBeGreaterThan(20);
    expect(gap).toBeLessThan(24);
    for (const fragment of fragments) {
        expect(fragment.kind.ink).toMatchObject({
            tool: stroke.tool, color: stroke.color, width: stroke.width,
        });
        expect(fragment.style).toEqual(original[0].style);
    }

    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    expect(sheetFrom(await saveAs(page, "pixel-erase-undo.thinoda"))).toEqual(before);
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+Shift+z");
    await frames(page);
    expect(sheetFrom(await saveAs(page, "pixel-erase-redo.thinoda"))).toEqual(after);
});

test.describe("touch gestures", () => {
    test.use({ hasTouch: true });

    test("a touch ink stroke commits once and does not contaminate mouse ink", async ({ page }) => {
        await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
        await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
        await page.locator("#thinoda_canvas").focus();
        await page.keyboard.press("Control+n");
        await frames(page);
        await command(page, "Hide central topic");
        await page.keyboard.press("Escape");
        await frames(page);
        await command(page, "Pen");

        await touchEvents(page, [{ type: "touchstart", x: 300, y: 700 }]);
        await frames(page);
        await touchEvents(page, [{ type: "touchmove", x: 450, y: 750 }]);
        await frames(page);
        await touchEvents(page, [{ type: "touchend", x: 600, y: 800 }]);
        await frames(page);
        const touchSheet = sheetFrom(await saveAs(page, "touch-ink.thinoda"));
        const strokes = touchSheet.canvas?.objects.filter(object => object.kind.ink) ?? [];
        expect(strokes).toHaveLength(1);
        expect(strokes[0].kind.ink?.points.length).toBeGreaterThan(1);

        await command(page, "Pen");
        await drag(page, [300, 1030], [600, 1100], 4);
        const mixed = sheetFrom(await saveAs(page, "touch-then-mouse-ink.thinoda"));
        const mixedStrokes = mixed.canvas.objects.filter(object => object.kind.ink);
        expect(mixedStrokes).toHaveLength(2);
        expect(mixedStrokes[0]).toEqual(strokes[0]);
        expect(mixedStrokes[1].kind.ink?.pressure).toBeUndefined();
        await page.keyboard.press("Control+z");
        await frames(page);
        expect(sheetFrom(await saveAs(page, "touch-then-mouse-undo.thinoda")).canvas.objects)
            .toEqual(strokes);
    });

    test("only absent touch force becomes optional; invalid force blocks the partial stroke", async ({ page }) => {
        await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
        await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
        const canvas = page.locator("#thinoda_canvas");
        await canvas.focus();
        await page.keyboard.press("Control+n");
        await frames(page);
        await command(page, "Hide central topic");
        await page.keyboard.press("Escape");
        await frames(page);
        await command(page, "Pen");

        await touchEvents(page, [{ type: "touchstart", x: 300, y: 700, force: "missing" }]);
        await frames(page);
        await touchEvents(page, [{ type: "touchmove", x: 450, y: 750, force: 0.6 }]);
        await frames(page);
        await touchEvents(page, [{ type: "touchend", x: 600, y: 800, force: "missing" }]);
        await frames(page);
        const saved = sheetFrom(await saveAs(page, "mixed-touch-force.thinoda"));
        const strokes = saved.canvas?.objects.filter(object => object.kind.ink) ?? [];
        expect(strokes).toHaveLength(1);
        const pressures = strokes[0].kind.ink?.pressure;
        expect(pressures).toBeDefined();
        expect(pressures!.every(value => Math.abs(value - 0.6) < 0.001)).toBe(true);

        await command(page, "Pen");
        await touchEvents(page, [{ type: "touchstart", x: 300, y: 900, force: "missing" }]);
        await frames(page);
        await touchEvents(page, [{ type: "touchmove", x: 450, y: 950, force: "invalid" }]);
        await frames(page);
        await touchEvents(page, [{ type: "touchend", x: 600, y: 1000, force: "missing" }]);
        await frames(page);
        await page.evaluate(() => { window.prompt = () => "unexpected-invalid-touch.thinoda"; });
        const unexpectedDownload = page.waitForEvent("download", { timeout: 2_000 })
            .then(() => true, () => false);
        await canvas.focus();
        await page.keyboard.press("Control+Shift+s");
        expect(await unexpectedDownload).toBe(false);
        await canvas.focus();
        await page.keyboard.press("Escape");
        await frames(page);
        expect(sheetFrom(await saveAs(page, "discard-invalid-touch.thinoda"))).toEqual(saved);
    });

    test("ambiguous touch pointer input cannot commit a batched stroke", async ({ page }) => {
        await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
        await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
        const canvas = page.locator("#thinoda_canvas");
        await canvas.focus();
        await page.keyboard.press("Control+n");
        await frames(page);
        await command(page, "Hide central topic");
        await page.keyboard.press("Escape");
        await frames(page);
        await command(page, "Pen");

        await touchEvents(page, [
            { type: "touchstart", x: 300, y: 700, force: 0.3 },
            { type: "touchmove", x: 450, y: 750, force: 0.6 },
            { type: "touchend", x: 600, y: 800, force: 0.7 },
        ], { mismatchedPointer: true });
        await frames(page);
        await page.evaluate(() => { window.prompt = () => "unexpected-ambiguous-touch.thinoda"; });
        const unexpectedDownload = page.waitForEvent("download", { timeout: 2_000 })
            .then(() => true, () => false);
        await canvas.focus();
        await page.keyboard.press("Control+Shift+s");
        expect(await unexpectedDownload).toBe(false);
        // The egui feedback window is not exposed in the browser accessibility tree.
        await page.mouse.move(1055, 276);
        await expect.poll(() => canvas.evaluate(element => (element as HTMLCanvasElement).style.cursor))
            .toBe("pointer");
        await page.mouse.click(1055, 276);
        await frames(page);
        const rejected = sheetFrom(await saveAs(page, "ambiguous-touch-rejected.thinoda"));
        expect(rejected.canvas?.objects.filter(object => object.kind.ink) ?? []).toHaveLength(0);

        await command(page, "Pen");
        await touchEvents(page, [
            { type: "touchstart", x: 300, y: 700, force: 0.3 },
            { type: "touchend", x: 600, y: 800, force: 0.7 },
        ]);
        await frames(page);
        const saved = sheetFrom(await saveAs(page, "touch-after-ambiguous.thinoda"));
        expect(saved.canvas?.objects.filter(object => object.kind.ink) ?? []).toHaveLength(1);
    });

    test("a real emulated tap creates exactly one shape", async ({ page }) => {
        await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
        await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
        await page.locator("#thinoda_canvas").focus();
        await page.keyboard.press("Control+n");
        await frames(page);
        await command(page, "Hide central topic");
        await page.keyboard.press("Escape");
        await frames(page);
        await command(page, "Shape tool");
        await page.touchscreen.tap(600, 500);
        await frames(page);

        const sheet = sheetFrom(await saveAs(page, "touch-tap-board.thinoda"));
        expect(sheet.root_mode).toBe("hidden");
        expect(sheet.canvas?.objects.filter((object) => object.kind.shape) ?? []).toHaveLength(1);
    });

    test("a trusted emulated touchscreen tap creates one ink dot", async ({ page }) => {
        await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
        await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
        const canvas = page.locator("#thinoda_canvas");
        await canvas.focus();
        await page.keyboard.press("Control+n");
        await frames(page);
        await command(page, "Hide central topic");
        await page.keyboard.press("Escape");
        await frames(page);
        await command(page, "Pen");
        await canvas.evaluate(element => {
            element.addEventListener("touchstart", event => {
                element.setAttribute("data-test-touch-trusted", String(event.isTrusted));
            }, { once: true });
        });
        await page.touchscreen.tap(600, 700);
        await frames(page);
        expect(await canvas.getAttribute("data-test-touch-trusted")).toBe("true");
        const sheet = sheetFrom(await saveAs(page, "touch-ink-dot.thinoda"));
        const strokes = sheet.canvas?.objects.filter(object => object.kind.ink) ?? [];
        expect(strokes).toHaveLength(1);
        expect(strokes[0].kind.ink?.points.length).toBeGreaterThan(0);
        expect(strokes[0].kind.ink?.pressure?.every(Number.isFinite) ?? true).toBe(true);
    });

    test("touch releases and fully batched touch drags preserve their geometry", async ({ page }) => {
        await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
        await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
        await page.locator("#thinoda_canvas").focus();
        await page.keyboard.press("Control+n");
        await frames(page);
        await command(page, "Hide central topic");
        await page.keyboard.press("Escape");
        await frames(page);

        await command(page, "Frame tool");
        await touchEvents(page, [{ type: "touchstart", x: 300, y: 750 }]);
        await frames(page);
        await touchEvents(page, [{ type: "touchmove", x: 700, y: 950 }]);
        await frames(page);
        await touchEvents(page, [{ type: "touchend", x: 850, y: 1000 }]);
        await frames(page);

        await command(page, "Shape tool");
        await touchEvents(page, [
            { type: "touchstart", x: 300, y: 400 },
            { type: "touchmove", x: 650, y: 600 },
            { type: "touchend", x: 650, y: 600 },
        ]);
        await frames(page);

        const sheet = sheetFrom(await saveAs(page, "touch-drag-board.thinoda"));
        expect(sheet.root_mode).toBe("hidden");
        const frame = sheet.zones?.find((zone) => zone.keep_empty);
        const shape = sheet.canvas?.objects.find((object) => object.kind.shape);
        expect(frame?.frame.size.x).toBeGreaterThan(500);
        expect(frame?.frame.size.y).toBeGreaterThan(200);
        expect(shape?.rect.size.x).toBeGreaterThan(300);
        expect(shape?.rect.size.y).toBeGreaterThan(150);
    });

    test("a canceled touch does not block the next drag or create a ghost shape", async ({ page }) => {
        await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
        await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
        await page.locator("#thinoda_canvas").focus();
        await page.keyboard.press("Control+n");
        await frames(page);
        await command(page, "Hide central topic");
        await page.keyboard.press("Escape");
        await frames(page);
        await command(page, "Shape tool");

        await touchEvents(page, [{ type: "touchstart", x: 300, y: 400 }]);
        await frames(page);
        await touchEvents(page, [{ type: "touchmove", x: 600, y: 600 }]);
        await frames(page);
        await touchEvents(page, [{ type: "touchcancel", x: 600, y: 600 }]);
        await frames(page);
        await touchEvents(page, [{ type: "touchstart", x: 320, y: 450 }]);
        await frames(page);
        await touchEvents(page, [{ type: "touchmove", x: 680, y: 650 }]);
        await frames(page);
        await touchEvents(page, [{ type: "touchend", x: 680, y: 650 }]);
        await frames(page);

        const sheet = sheetFrom(await saveAs(page, "touch-cancel-board.thinoda"));
        expect(sheet.root_mode).toBe("hidden");
        const shapes = sheet.canvas?.objects.filter((object) => object.kind.shape) ?? [];
        expect(shapes).toHaveLength(1);
        expect(shapes[0].rect.size.x).toBeGreaterThan(300);
        expect(shapes[0].rect.size.y).toBeGreaterThan(150);
    });

    test("a canceled touch and its replacement can arrive in one browser frame", async ({ page }) => {
        await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
        await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
        await page.locator("#thinoda_canvas").focus();
        await page.keyboard.press("Control+n");
        await frames(page);
        await command(page, "Hide central topic");
        await page.keyboard.press("Escape");
        await frames(page);
        await command(page, "Shape tool");

        await touchEvents(page, [{ type: "touchstart", x: 300, y: 400 }]);
        await frames(page);
        await touchEvents(page, [{ type: "touchmove", x: 600, y: 600 }]);
        await frames(page);
        await touchEvents(page, [
            { type: "touchcancel", x: 600, y: 600 },
            { type: "touchstart", x: 320, y: 450 },
            { type: "touchmove", x: 680, y: 650 },
            { type: "touchend", x: 680, y: 650 },
        ]);
        await frames(page);

        const sheet = sheetFrom(await saveAs(page, "batched-touch-cancel-board.thinoda"));
        expect(sheet.root_mode).toBe("hidden");
        const shapes = sheet.canvas?.objects.filter((object) => object.kind.shape) ?? [];
        expect(shapes).toHaveLength(1);
        expect(shapes[0].rect.size.x).toBeGreaterThan(300);
        expect(shapes[0].rect.size.y).toBeGreaterThan(150);
    });
});

test("captured board viewport fits in the Pitch player", async ({ page }) => {
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
    await page.locator("input[type='text']").pressSequentially("Pitch note");
    await page.keyboard.press("Enter");
    await frames(page);
    await command(page, "Edit Presentation");
    await page.waitForTimeout(500);
    await page.mouse.click(482, 403);
    await page.waitForTimeout(500);
    await page.mouse.click(133, 963);
    await frames(page);
    await command(page, "Pitch");
    await page.waitForTimeout(500);

    const screenshot = await page.screenshot();
    const colors = await page.evaluate(async (base64) => {
        const image = new Image();
        image.src = `data:image/png;base64,${base64}`;
        await image.decode();
        const canvas = document.createElement("canvas");
        canvas.width = image.width;
        canvas.height = image.height;
        const context = canvas.getContext("2d");
        if (!context) throw new Error("Cannot inspect Pitch screenshot");
        context.drawImage(image, 0, 0);
        const { data } = context.getImageData(136, 475, 980, 475);
        let yellow = 0;
        let red = 0;
        for (let i = 0; i < data.length; i += 4) {
            const [r, g, b] = [data[i], data[i + 1], data[i + 2]];
            if (r > 240 && g > 215 && b < 160) yellow++;
            if (r > 190 && g < 170 && b < 170 && r > g * 1.3) red++;
        }
        return { yellow, red };
    }, screenshot.toString("base64"));
    expect(colors.yellow).toBeGreaterThan(100);
    expect(colors.red).toBeGreaterThan(10);
});
