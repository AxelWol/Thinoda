/**
 * End-to-end board editing against a freshly built production web shell.
 * The native archive is inspected rather than inferring edits from pixels.
 */
import { expect, test } from "@playwright/test";
import { command, drag, frames, saveAs, touchEvents, zipEntry } from "./board-browser-helpers";

test.use({
    viewport: { width: 1252, height: 1278 },
    deviceScaleFactor: 1,
    serviceWorkers: "block",
    screenshot: "only-on-failure",
});
test.setTimeout(180_000);

type BoardSheet = {
    root_mode: string;
    floating: number[];
    topics: Record<string, { id: number; title: string; card?: { kind: string; color?: string } }>;
    zones: { keep_empty?: boolean; frame: { size: { x: number; y: number } } }[];
    canvas: {
        objects: {
            id: number;
            kind: { shape?: { shape: string; label?: string }; connector?: { from: unknown; to: unknown; label?: string }; ink?: { points: unknown[] } };
            rect: { min: { x: number; y: number }; size: { x: number; y: number } };
            style: { fill?: string; text_color?: string };
        }[];
    };
};

function sheetFrom(zip: Buffer): BoardSheet {
    const content: { sheets: BoardSheet[] } = JSON.parse(zipEntry(zip, "content.json").toString("utf8"));
    return content.sheets[0];
}

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

test.describe("touch gestures", () => {
    test.use({ hasTouch: true });

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
