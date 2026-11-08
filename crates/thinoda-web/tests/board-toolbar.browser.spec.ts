import { expect, test, type Page } from "@playwright/test";
import { join } from "node:path";
import { command, frames, saveAs, zipEntry } from "./board-browser-helpers";

test.use({
    viewport: { width: 800, height: 600 },
    deviceScaleFactor: 1,
    locale: "en-US",
    serviceWorkers: "block",
    screenshot: "only-on-failure",
});
test.setTimeout(180_000);

async function stickyPixels(page: Page): Promise<number[]> {
    const screenshot = await page.screenshot();
    return page.evaluate(async (base64) => {
        const image = new Image();
        image.src = `data:image/png;base64,${base64}`;
        await image.decode();
        const canvas = document.createElement("canvas");
        canvas.width = image.width;
        canvas.height = image.height;
        const context = canvas.getContext("2d");
        if (!context) throw new Error("Cannot inspect the board screenshot");
        context.drawImage(image, 0, 0);
        const { data } = context.getImageData(0, 0, canvas.width, canvas.height);
        const colors = [
            [255, 229, 143], [183, 235, 143], [165, 216, 255], [255, 193, 217],
        ];
        const counts = colors.map(() => 0);
        for (let y = 0; y < canvas.height; y++) {
            for (let x = 0; x < canvas.width * 0.75; x++) {
                const offset = (y * canvas.width + x) * 4;
                colors.forEach(([r, g, b], index) => {
                    if (Math.abs(data[offset] - r) <= 4
                        && Math.abs(data[offset + 1] - g) <= 4
                        && Math.abs(data[offset + 2] - b) <= 4) counts[index]++;
                });
            }
        }
        return counts;
    }, screenshot.toString("base64"));
}

test("selected-topic actions leave the Affinity starter notes visible", async ({ page }) => {
    await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    const canvas = page.locator("#thinoda_canvas");
    // Exported from the built-in affinity-diagram template with thinoda-io.
    await canvas.drop({ files: join(__dirname, "fixtures", "affinity-diagram.thinoda") });
    await expect.poll(async () => (await stickyPixels(page))[2], { timeout: 30_000 })
        .toBeGreaterThan(200);

    const box = await canvas.boundingBox();
    if (!box) throw new Error("The board canvas is unavailable");
    await page.mouse.move(box.x + box.width * 0.6, box.y + box.height * 0.8);
    await page.mouse.wheel(0, 80);
    await page.waitForTimeout(600);
    await canvas.focus();
    await page.keyboard.down("Control");
    await page.mouse.wheel(0, 100);
    await page.keyboard.up("Control");
    await page.waitForTimeout(600);
    await page.mouse.wheel(0, 170);
    await page.waitForTimeout(600);

    const [yellow, green, blue, pink] = await stickyPixels(page);
    expect(blue).toBeGreaterThan(500);
    expect(pink).toBeGreaterThan(500);
    expect(yellow, "the toolbar must not obscure the upper yellow note")
        .toBeGreaterThan(blue * 0.85);
    expect(green, "the toolbar must not obscure the upper green note")
        .toBeGreaterThan(pink * 0.85);

    await command(page, "To-Do");
    await frames(page);
    const workbook: { sheets: { topics: Record<string, { title: string; todo?: boolean }> }[] } =
        JSON.parse(zipEntry(await saveAs(page, "affinity-toolbar.thinoda"), "content.json")
            .toString("utf8"));
    const heading = Object.values(workbook.sheets[0].topics).find(
        (topic) => topic.title === "Group related ideas, then name each theme",
    );
    expect(heading?.todo).toBe(false);
});

test("the selected-shape toolbar changes and restores label text color", async ({ page }) => {
    await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    const canvas = page.locator("#thinoda_canvas");
    await canvas.focus();
    await page.keyboard.press("Control+n");
    await frames(page);
    await command(page, "Hide central topic");
    await page.keyboard.press("Escape");
    await frames(page);
    await command(page, "Format Panel");
    await command(page, "Shape tool");
    await canvas.click({ position: { x: 450, y: 350 } });
    await frames(page);
    await canvas.focus();
    await page.keyboard.press("F2");
    await frames(page);
    const input = page.locator("input[type='text']");
    await expect(input).toBeFocused();
    await input.pressSequentially("Colored browser shape");
    await page.keyboard.press("Enter");
    await frames(page);

    const shape = async (name: string) => {
        const workbook: {
            active_sheet: number;
            sheets: { canvas: { objects: {
                kind: { shape?: { label?: string } };
                style: { text_color?: string };
            }[] } }[];
        } = JSON.parse(zipEntry(await saveAs(page, name), "content.json").toString("utf8"));
        const found = workbook.sheets[workbook.active_sheet].canvas.objects.find(
            (object) => object.kind.shape?.label === "Colored browser shape",
        );
        if (!found) throw new Error("The labeled shape was not saved");
        return found;
    };

    expect((await shape("shape-color-auto.thinoda")).style.text_color).toBeUndefined();
    // The egui toolbar is drawn inside the canvas rather than exposed as DOM buttons.
    await canvas.click({ position: { x: 277, y: 76 } });
    await frames(page);
    await canvas.click({ position: { x: 277, y: 145 } });
    await frames(page);
    await page.keyboard.press("Escape");
    await frames(page);
    expect((await shape("shape-color-blue.thinoda")).style.text_color).toBe("#2E7CF6");

    await page.keyboard.press("Control+z");
    await frames(page);
    expect((await shape("shape-color-undo.thinoda")).style.text_color).toBeUndefined();
    await page.keyboard.press("Control+Shift+z");
    await frames(page);
    expect((await shape("shape-color-redo.thinoda")).style.text_color).toBe("#2E7CF6");

    await canvas.click({ position: { x: 277, y: 76 } });
    await frames(page);
    await canvas.click({ position: { x: 290, y: 112 } });
    await frames(page);
    await page.keyboard.press("Escape");
    await frames(page);
    expect((await shape("shape-color-reset.thinoda")).style.text_color).toBeUndefined();
});

test("double-click label editing handles multiline, cancel and outside click", async ({ page }, testInfo) => {
    await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    const canvas = page.locator("#thinoda_canvas");
    await canvas.focus();
    await page.keyboard.press("Control+n");
    await frames(page);
    await command(page, "Hide central topic");
    await page.keyboard.press("Escape");
    await frames(page);
    await command(page, "Format Panel");
    await command(page, "Shape tool");
    await canvas.click({ position: { x: 450, y: 350 } });
    await frames(page);

    const edit = async (delay = 0) => {
        await canvas.dblclick({ position: { x: 450, y: 350 }, delay });
        await frames(page);
        const input = page.locator("input[type='text']");
        await expect(input).toBeFocused();
        return input;
    };
    const savedShapes = async (name: string) => {
        const workbook: {
            active_sheet: number;
            sheets: { canvas: { objects: { kind: { shape?: { label?: string } } }[] } }[];
        } = JSON.parse(zipEntry(await saveAs(page, name), "content.json").toString("utf8"));
        return workbook.sheets[workbook.active_sheet].canvas.objects
            .filter((object) => object.kind.shape);
    };

    const first = await edit(100);
    await page.screenshot({ path: testInfo.outputPath("shape-label-mode.png") });
    await first.pressSequentially("First");
    await page.keyboard.press("Shift+Enter");
    await first.pressSequentially("Second");
    await page.keyboard.press("Enter");
    await frames(page);
    expect((await savedShapes("multiline-label.thinoda"))[0].kind.shape?.label)
        .toBe("First\nSecond");

    const canceled = await edit();
    await canceled.pressSequentially("Discarded");
    await page.keyboard.press("Escape");
    await frames(page);
    expect((await savedShapes("canceled-label.thinoda"))[0].kind.shape?.label)
        .toBe("First\nSecond");

    const outside = await edit(100);
    await outside.pressSequentially("Outside");
    await canvas.click({ position: { x: 700, y: 500 } });
    await frames(page);
    const shapes = await savedShapes("outside-label.thinoda");
    expect(shapes).toHaveLength(2);
    expect(shapes[0].kind.shape?.label).toBe("Outside");
    expect(shapes[1].kind.shape?.label ?? "").toBe("");
});
