import { expect, test } from "@playwright/test";
import { colorBounds, frames, saveAs, zipEntry } from "./board-browser-helpers";

test.use({
    viewport: { width: 1000, height: 800 },
    deviceScaleFactor: 1,
    locale: "en-US",
    colorScheme: "light",
    serviceWorkers: "block",
});
test.setTimeout(180_000);

test("imports and saves a partial rectangle with only its visible sides", async ({ page }) => {
    await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    const canvas = page.locator("#thinoda_canvas");
    const xml = `<mxfile><diagram name="Partial"><mxGraphModel adaptiveColors="simple"><root>
        <mxCell id="0"/><mxCell id="1" parent="0"/>
        <mxCell id="partial" vertex="1" parent="1"
          style="shape=partialRectangle;top=1;right=0;bottom=1;left=0;fillColor=none;strokeColor=#ED1234;strokeWidth=10;">
          <mxGeometry x="250" y="200" width="160" height="100" as="geometry"/>
        </mxCell>
        </root></mxGraphModel></diagram></mxfile>`;
    await canvas.drop({
        files: { name: "partial.drawio", mimeType: "application/xml", buffer: Buffer.from(xml) },
    });
    await frames(page);
    await expect.poll(async () => (
        await colorBounds(page, await page.screenshot(), [[237, 18, 52]], 5)
    )[0].count, { timeout: 30_000 }).toBeGreaterThan(200);
    const screenshot = await page.screenshot();
    const bounds = (await colorBounds(page, screenshot, [[237, 18, 52]], 5))[0];
    expect(bounds.maxX - bounds.minX).toBeGreaterThan(50);
    expect(bounds.maxY - bounds.minY).toBeGreaterThan(30);
    const samples = await page.evaluate(async ({ base64, bounds }) => {
        const image = new Image();
        image.src = `data:image/png;base64,${base64}`;
        await image.decode();
        const surface = document.createElement("canvas");
        surface.width = image.width;
        surface.height = image.height;
        const context = surface.getContext("2d");
        if (!context) throw new Error("Cannot inspect the rendered rectangle");
        context.drawImage(image, 0, 0);
        const x = Math.round((bounds.minX + bounds.maxX) / 2);
        const y = Math.round((bounds.minY + bounds.maxY) / 2);
        return [
            [x, bounds.minY + 2],
            [x, bounds.maxY - 2],
            [bounds.minX + 2, y],
            [bounds.maxX - 2, y],
        ].map(([px, py]) => [...context.getImageData(px, py, 1, 1).data]);
    }, { base64: screenshot.toString("base64"), bounds });
    const red = (pixel: number[]) =>
        Math.abs(pixel[0] - 237) < 10 && pixel[1] < 30 && Math.abs(pixel[2] - 52) < 10;
    expect(samples.map(red)).toEqual([true, true, false, false]);

    const archive = await saveAs(page, "partial-rectangle.thinoda");
    const manifest = JSON.parse(zipEntry(archive, "manifest.json").toString("utf8"));
    const workbook = JSON.parse(zipEntry(archive, "content.json").toString("utf8"));
    const objects = workbook.sheets[workbook.active_sheet].canvas.objects;
    expect(manifest.version).toBe(20);
    expect(objects).toHaveLength(1);
    expect(objects[0].rectangle_sides).toEqual({
        top: true, right: false, bottom: true, left: false,
    });
});

test("move-only outline subpaths do not change browser rasterization", async ({ page }) => {
    const differences = await page.evaluate(async () => {
        async function pixels(path: string, opacity: number) {
            const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128">
                <path d="${path}" fill="none" stroke="#1264D2" stroke-opacity="${opacity}"
                  stroke-width="8" stroke-linecap="square" stroke-linejoin="miter"/>
                </svg>`;
            const image = new Image();
            image.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
            await image.decode();
            const surface = document.createElement("canvas");
            surface.width = 128;
            surface.height = 128;
            const context = surface.getContext("2d");
            if (!context) throw new Error("Cannot inspect the SVG outline");
            context.drawImage(image, 0, 0);
            return context.getImageData(0, 0, 128, 128).data;
        }

        const failures = [];
        for (const [width, height, mask] of [
            [40, 30, 0b0010], [8, 30, 0b0011], [40, 8, 0b0001],
            [40, 0, 0b0001], [40, 0, 0b0011],
        ]) {
            const radians = 37 * Math.PI / 180;
            const cx = 40 + width / 2;
            const cy = 40 + height / 2;
            const corners = [[40, 40], [40 + width, 40], [40 + width, 40 + height], [40, 40 + height]]
                .map(([x, y]) => [
                    cx + (x - cx) * Math.cos(radians) - (y - cy) * Math.sin(radians),
                    cy + (x - cx) * Math.sin(radians) + (y - cy) * Math.cos(radians),
                ].map((value) => value.toFixed(2)).join(","));
            let source = `M ${corners[0]}`;
            let simplified = "";
            let drawing = false;
            for (let side = 0; side < 4; side++) {
                const visible = Boolean(mask & (1 << side));
                if (side < 3 || visible) {
                    source += ` ${visible ? "L" : "M"} ${corners[(side + 1) % 4]}`;
                }
                if (visible) {
                    if (!drawing) simplified += ` M ${corners[side]}`;
                    simplified += ` L ${corners[(side + 1) % 4]}`;
                }
                drawing = visible;
            }
            for (const opacity of [1, 0.5]) {
                const actual = await pixels(simplified, opacity);
                const expected = await pixels(source, opacity);
                if (!expected.some((value, index) => index % 4 === 3 && value > 0)) {
                    throw new Error("The SVG reference outline did not paint");
                }
                const changed = actual.some((value, index) => value !== expected[index]);
                if (changed) failures.push({ width, height, mask, opacity });
            }
        }
        return failures;
    });
    expect(differences).toEqual([]);
});
