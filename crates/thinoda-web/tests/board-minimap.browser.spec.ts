import { expect, test } from "@playwright/test";
import { command, frames, saveAs, zipEntry } from "./board-browser-helpers";

test.use({
    viewport: { width: 1252, height: 1278 },
    deviceScaleFactor: 1,
    locale: "en-US",
    serviceWorkers: "block",
});
test.setTimeout(180_000);

function sheetFrom(zip: Buffer) {
    const workbook = JSON.parse(zipEntry(zip, "content.json").toString("utf8"));
    return workbook.sheets[0] as {
        root_mode: string;
        canvas: { objects: { rect: { min: { x: number; y: number } } }[] };
    };
}

test("the minimap captures a shape-tool click without creating an object", async ({ page }) => {
    await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+n");
    await frames(page);
    await command(page, "Hide central topic");
    await page.keyboard.press("Escape");
    await frames(page);
    await command(page, "Format Panel");
    await command(page, "Shape tool");
    await page.mouse.click(650, 400);
    await frames(page);
    await command(page, "Board minimap");
    await command(page, "Shape tool");

    const before = sheetFrom(await saveAs(page, "minimap-before.thinoda"));
    expect(before.root_mode).toBe("hidden");
    expect(before.canvas.objects).toHaveLength(1);

    const viewport = page.viewportSize()!;
    await page.mouse.click(viewport.width - 80, viewport.height - 90);
    await frames(page);
    const after = sheetFrom(await saveAs(page, "minimap-after.thinoda"));
    expect(after.canvas.objects).toEqual(before.canvas.objects);

    await page.mouse.click(650, 400);
    await frames(page);
    const outside = sheetFrom(await saveAs(page, "minimap-outside.thinoda"));
    expect(outside.canvas.objects).toHaveLength(2);
    expect(outside.canvas.objects[1].rect.min).not.toEqual(before.canvas.objects[0].rect.min);

    await command(page, "Board minimap");
    await command(page, "Shape tool");
    await page.mouse.click(viewport.width - 80, viewport.height - 90);
    await frames(page);
    const hidden = sheetFrom(await saveAs(page, "minimap-hidden.thinoda"));
    expect(hidden.canvas.objects).toHaveLength(3);
});
