import { expect, test } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { colorBounds, command, frames, saveAs, zipEntry } from "./board-browser-helpers";

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

type Sheet = {
    title: string;
    theme: { background?: string };
    canvas: { objects: { kind: { shape?: { label?: string } } }[] };
};

function activeSheet(archive: Buffer): Sheet {
    const workbook: { active_sheet: number; sheets: Sheet[] } =
        JSON.parse(zipEntry(archive, "content.json").toString("utf8"));
    return workbook.sheets[workbook.active_sheet];
}

function diagram(page: string, background: string, adaptive = ""): Buffer {
    return Buffer.from(`<mxfile><diagram name="Background page"><mxGraphModel grid="0" page="${page}"
        background="${background}" ${adaptive}><root>
        <mxCell id="0"/><mxCell id="1" parent="0"/>
        <mxCell id="reference" value="Background reference" vertex="1" parent="1"
          style="shape=rectangle;fillColor=#285997;strokeColor=#285997;">
          <mxGeometry x="0" y="0" width="100" height="60" as="geometry"/>
        </mxCell></root></mxGraphModel></diagram></mxfile>`);
}

async function dropDiagram(page: import("@playwright/test").Page, buffer: Buffer) {
    await page.locator("#thinoda_canvas").drop({
        files: { name: "background.drawio", mimeType: "application/xml", buffer },
    });
    await frames(page);
}

test("imports a fixed background, paints it, and exports it for draw.io", async ({ page }) => {
    await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    await dropDiagram(page, diagram("0", "light-dark(#F4C27A,#F4C27A)"));
    await expect.poll(async () => (
        await colorBounds(page, await page.screenshot(), [[244, 194, 122]], 5)
    )[0].count, { timeout: 30_000 }).toBeGreaterThan(20_000);

    await command(page, "Dark");
    await expect.poll(async () => (
        await colorBounds(page, await page.screenshot(), [[36, 36, 40]], 4)
    )[0].count, { timeout: 30_000 }).toBeGreaterThan(1_000);
    await expect.poll(async () => (
        await colorBounds(page, await page.screenshot(), [[244, 194, 122]], 5)
    )[0].count, { timeout: 30_000 }).toBeGreaterThan(20_000);

    const sheet = activeSheet(await saveAs(page, "background.thinoda"));
    expect(sheet.title).toBe("Background page");
    expect(sheet.theme.background).toBe("#F4C27A");
    expect(sheet.canvas.objects.some((object) =>
        object.kind.shape?.label === "Background reference")).toBe(true);

    await page.evaluate(() => { window.prompt = () => "background-export.drawio"; });
    const pending = page.waitForEvent("download", { timeout: 30_000 });
    await command(page, "Export draw.io");
    const download = await pending;
    const xml = (await readFile(await download.path())).toString("utf8");
    expect(xml).toContain('background="light-dark(#F4C27A,#F4C27A)"');
    expect(xml).not.toContain("adaptiveColors=");
});

test("page-boundary approximation can be canceled or imported", async ({ page }) => {
    await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    await dropDiagram(page, diagram("0", "light-dark(#F4C27A,#F4C27A)"));
    await expect.poll(async () => (
        await colorBounds(page, await page.screenshot(), [[244, 194, 122]], 5)
    )[0].count, { timeout: 30_000 }).toBeGreaterThan(20_000);
    const original = activeSheet(await saveAs(page, "background-before.thinoda"));
    const xml = diagram("1", "#35CBA0", 'adaptiveColors="simple"');

    await dropDiagram(page, xml);
    await page.mouse.move(793, 832);
    await expect.poll(() => page.locator("#thinoda_canvas").evaluate(
        (canvas) => (canvas as HTMLCanvasElement).style.cursor,
    ), { timeout: 30_000 }).toBe("pointer");
    await page.keyboard.press("Escape");
    await frames(page);
    expect(activeSheet(await saveAs(page, "background-canceled.thinoda"))).toEqual(original);

    await dropDiagram(page, xml);
    // The conversion dialog is painted by egui into the canvas.
    await page.mouse.move(670, 832);
    await expect.poll(() => page.locator("#thinoda_canvas").evaluate(
        (canvas) => (canvas as HTMLCanvasElement).style.cursor,
    ), { timeout: 30_000 }).toBe("pointer");
    await page.mouse.click(670, 832);
    await frames(page);
    await expect.poll(async () => (
        await colorBounds(page, await page.screenshot(), [[53, 203, 160]], 5)
    )[0].count, { timeout: 30_000 }).toBeGreaterThan(20_000);
    expect(activeSheet(await saveAs(page, "background-accepted.thinoda")).theme.background)
        .toBe("#35CBA0");
});
