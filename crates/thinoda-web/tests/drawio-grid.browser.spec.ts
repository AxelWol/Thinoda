import { expect, test } from "@playwright/test";
import { colorBounds, command, frames, saveAs, zipEntry } from "./board-browser-helpers";

test.use({
    viewport: { width: 1000, height: 800 },
    deviceScaleFactor: 1,
    locale: "en-US",
    colorScheme: "light",
    serviceWorkers: "block",
});
test.setTimeout(180_000);

type Sheet = {
    board: { grid: string; grid_size: number; snap: boolean };
    canvas: {
        objects: {
            kind: { shape?: { label?: string } };
            rect: { min: { x: number; y: number }; size: { x: number; y: number } };
        }[];
    };
};

function activeSheet(archive: Buffer): Sheet {
    const workbook: { active_sheet: number; sheets: Sheet[] } =
        JSON.parse(zipEntry(archive, "content.json").toString("utf8"));
    return workbook.sheets[workbook.active_sheet];
}

for (const enabled of [true, false]) {
    test(`draw.io grid ${enabled ? "enables" : "disables"} snapping for new shapes`, async ({ page }) => {
        await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
        await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
        const canvas = page.locator("#thinoda_canvas");
        const xml = `<mxfile><diagram name="Grid"><mxGraphModel grid="${Number(enabled)}" gridSize="20" adaptiveColors="simple"><root>
            <mxCell id="0"/><mxCell id="1" parent="0"/>
            <mxCell id="reference" value="Reference" vertex="1" parent="1"
              style="shape=rectangle;fillColor=#ED1234;strokeColor=#ED1234;">
              <mxGeometry x="0" y="0" width="100" height="60" as="geometry"/>
            </mxCell>
            </root></mxGraphModel></diagram></mxfile>`;
        await canvas.drop({
            files: { name: "grid.drawio", mimeType: "application/xml", buffer: Buffer.from(xml) },
        });
        await expect.poll(async () => (
            await colorBounds(page, await page.screenshot(), [[237, 18, 52]], 5)
        )[0].count, { timeout: 30_000 }).toBeGreaterThan(200);

        const imported = activeSheet(await saveAs(page, `grid-${enabled}-import.thinoda`));
        expect(imported.board).toEqual({ grid: enabled ? "lines" : "none", grid_size: 20, snap: enabled });
        expect(imported.canvas.objects).toHaveLength(1);

        await command(page, "Shape tool");
        await canvas.click({ position: { x: 337, y: 263 } });
        await frames(page);
        const saved = activeSheet(await saveAs(page, `grid-${enabled}-placed.thinoda`));
        expect(saved.board).toEqual(imported.board);
        const placed = saved.canvas.objects.filter((object) =>
            object.kind.shape && object.kind.shape.label !== "Reference");
        expect(placed).toHaveLength(1);
        const { min, size } = placed[0].rect;
        const center = [min.x + size.x / 2, min.y + size.y / 2];
        const offsets = center.map((value) => Math.abs(value - Math.round(value / 20) * 20));
        if (enabled) {
            expect(offsets[0]).toBeLessThan(0.001);
            expect(offsets[1]).toBeLessThan(0.001);
        } else {
            expect(Math.max(...offsets)).toBeGreaterThan(0.5);
        }
    });
}
