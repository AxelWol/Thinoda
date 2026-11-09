import { expect, test } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { colorBounds, command, frames, saveAs, zipEntry } from "./board-browser-helpers";

test.use({
    viewport: { width: 1000, height: 800 },
    deviceScaleFactor: 1,
    locale: "en-US",
    colorScheme: "light",
    serviceWorkers: "block",
});
test.setTimeout(180_000);

type Point = { x: number; y: number };
type Port = { position: Point; offset: Point; perimeter: boolean };
type Sheet = {
    board: { snap: boolean; grid_size: number };
    canvas: {
        objects: {
            id: number;
            kind: {
                shape?: { label?: string };
                connector?: {
                    from: { object?: number; point?: Point };
                    to: { object?: number; point?: Point };
                    from_port?: Port;
                    to_port?: Port;
                };
            };
        }[];
    };
};

function activeSheet(archive: Buffer): Sheet {
    const workbook: { active_sheet: number; sheets: Sheet[] } =
        JSON.parse(zipEntry(archive, "content.json").toString("utf8"));
    return workbook.sheets[workbook.active_sheet];
}

for (const mode of ["alt", "shift", "alt-shift"] as const) {
    test(`connector ${mode} drop keeps the intended endpoint through undo and redo`, async ({ page }) => {
        await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
        await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
        const canvas = page.locator("#thinoda_canvas");
        const xml = `<mxfile><diagram name="Connectors"><mxGraphModel grid="1" gridSize="20" adaptiveColors="simple"><root>
            <mxCell id="0"/><mxCell id="1" parent="0"/>
            <mxCell id="source" value="Source" vertex="1" parent="1"
              style="shape=rectangle;fillColor=#D0274F;strokeColor=#D0274F;">
              <mxGeometry x="0" y="0" width="120" height="80" as="geometry"/>
            </mxCell>
            <mxCell id="target" value="Target" vertex="1" parent="1"
              style="shape=rectangle;fillColor=#287AC4;strokeColor=#287AC4;">
              <mxGeometry x="320" y="140" width="120" height="80" as="geometry"/>
            </mxCell>
            </root></mxGraphModel></diagram></mxfile>`;
        await canvas.drop({
            files: { name: "connectors.drawio", mimeType: "application/xml", buffer: Buffer.from(xml) },
        });
        const colors = [[208, 39, 79], [40, 122, 196]] as const;
        const bounds = async () => colorBounds(page, await page.screenshot(), colors, 5);
        await expect.poll(async () => (await bounds()).every(region => region.count > 500), {
            timeout: 30_000,
        }).toBe(true);

        const baseline = activeSheet(await saveAs(page, `connectors-${mode}-before.thinoda`));
        expect(baseline.board).toMatchObject({ snap: true, grid_size: 20 });
        const source = baseline.canvas.objects.find(object => object.kind.shape?.label === "Source");
        const target = baseline.canvas.objects.find(object => object.kind.shape?.label === "Target");
        if (!source || !target) throw new Error("Missing imported connector targets");

        await command(page, "Connector tool");
        const [sourceBounds, targetBounds] = await bounds();
        const start = {
            x: sourceBounds.maxX + 1,
            y: (sourceBounds.minY + sourceBounds.maxY + 1) / 2,
        };
        const end = {
            x: Math.round(targetBounds.minX + (targetBounds.maxX - targetBounds.minX) * 0.59),
            y: Math.round(targetBounds.minY + (targetBounds.maxY - targetBounds.minY) * 0.61),
        };
        await page.mouse.move(start.x, start.y);
        await page.mouse.down();
        await frames(page);
        await page.mouse.move((start.x + end.x) / 2, (start.y + end.y) / 2, { steps: 6 });
        await frames(page);
        if (mode.includes("shift")) await page.keyboard.down("Shift");
        if (mode.includes("alt")) await page.keyboard.down("Alt");
        await page.mouse.move(end.x, end.y, { steps: 6 });
        await frames(page);
        await page.mouse.up();
        if (mode.includes("alt")) await page.keyboard.up("Alt");
        if (mode.includes("shift")) await page.keyboard.up("Shift");
        await frames(page);

        const saved = activeSheet(await saveAs(page, `connectors-${mode}-after.thinoda`));
        const edges = saved.canvas.objects.flatMap(object => object.kind.connector ? [object.kind.connector] : []);
        expect(edges).toHaveLength(1);
        const [edge] = edges;
        expect(edge.from).toEqual({ object: source.id });
        expect(edge.from_port).toEqual({
            position: { x: 1, y: 0.5 },
            offset: { x: 0, y: 0 },
            perimeter: true,
        });
        if (mode === "alt") {
            expect(edge.to.point).toBeDefined();
            expect(edge.to.object).toBeUndefined();
            expect(edge.to_port).toBeUndefined();
            const point = edge.to.point!;
            expect(Math.max(...[point.x, point.y].map(value =>
                Math.abs(value - Math.round(value / 20) * 20)))).toBeGreaterThan(1);
        } else {
            expect(edge.to).toEqual({ object: target.id });
            if (mode === "shift") {
                expect(edge.to_port?.perimeter).toBe(false);
                expect(edge.to_port?.position.x).toBeGreaterThan(0.5);
                expect(edge.to_port?.position.x).toBeLessThan(0.7);
                expect(edge.to_port?.position.y).toBeGreaterThan(0.5);
                expect(edge.to_port?.position.y).toBeLessThan(0.7);
            } else {
                expect(edge.to_port).toBeUndefined();
            }
        }

        await canvas.focus();
        await page.keyboard.press("Control+z");
        await frames(page);
        expect(activeSheet(await saveAs(page, `connectors-${mode}-undo.thinoda`))).toEqual(baseline);
        await canvas.focus();
        await page.keyboard.press("Control+Shift+z");
        await frames(page);
        expect(activeSheet(await saveAs(page, `connectors-${mode}-redo.thinoda`))).toEqual(saved);
    });
}

test("double-ellipse crosses create persistent connectors with an elliptic draw.io perimeter", async ({ page }) => {
    await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    const canvas = page.locator("#thinoda_canvas");
    const xml = `<mxfile><diagram name="Double ellipses"><mxGraphModel adaptiveColors="simple"><root>
        <mxCell id="0"/><mxCell id="1" parent="0"/>
        <mxCell id="source" value="Source" vertex="1" parent="1"
          style="shape=doubleEllipse;perimeter=ellipsePerimeter;fillColor=#D0274F;strokeColor=#D0274F;fontColor=#000000;margin=8.5;">
          <mxGeometry x="0" y="0" width="120" height="80" as="geometry"/>
        </mxCell>
        <mxCell id="target" value="Target" vertex="1" parent="1"
          style="shape=doubleEllipse;perimeter=ellipsePerimeter;fillColor=#287AC4;strokeColor=#287AC4;fontColor=#000000;">
          <mxGeometry x="320" y="140" width="120" height="80" as="geometry"/>
        </mxCell>
        </root></mxGraphModel></diagram></mxfile>`;
    await canvas.drop({
        files: { name: "double-ellipses.drawio", mimeType: "application/xml", buffer: Buffer.from(xml) },
    });
    const colors = [[208, 39, 79], [40, 122, 196]] as const;
    const bounds = async () => colorBounds(page, await page.screenshot(), colors, 5);
    await expect.poll(async () => (await bounds()).every(region => region.count > 500), {
        timeout: 30_000,
    }).toBe(true);
    const baseline = activeSheet(await saveAs(page, "double-ellipses-before.thinoda"));
    const source = baseline.canvas.objects.find(object => object.kind.shape?.label === "Source");
    const target = baseline.canvas.objects.find(object => object.kind.shape?.label === "Target");
    if (!source || !target) throw new Error("Missing imported double ellipses");

    await command(page, "Connector tool");
    const [sourceBounds, targetBounds] = await bounds();
    const diagonal = (region: typeof sourceBounds, direction: number) => ({
        x: Math.round((region.minX + region.maxX) / 2
            + direction * (region.maxX - region.minX) / (2 * Math.sqrt(2))),
        y: Math.round((region.minY + region.maxY) / 2
            + direction * (region.maxY - region.minY) / (2 * Math.sqrt(2))),
    });
    const start = diagonal(sourceBounds, 1);
    const end = diagonal(targetBounds, -1);
    await page.mouse.move(start.x, start.y);
    await page.mouse.down();
    await frames(page);
    await page.mouse.move(end.x, end.y, { steps: 10 });
    await frames(page);
    await page.mouse.up();
    await frames(page);

    const saved = activeSheet(await saveAs(page, "double-ellipses-after.thinoda"));
    const edges = saved.canvas.objects.flatMap(object => object.kind.connector ? [object.kind.connector] : []);
    expect(edges).toHaveLength(1);
    expect(edges[0]).toMatchObject({
        from: { object: source.id },
        to: { object: target.id },
        from_port: { position: { x: 1, y: 1 }, offset: { x: 0, y: 0 }, perimeter: true },
        to_port: { position: { x: 0, y: 0 }, offset: { x: 0, y: 0 }, perimeter: true },
    });

    await page.evaluate(() => { window.prompt = () => "double-ellipses-export.drawio"; });
    const pending = page.waitForEvent("download", { timeout: 30_000 });
    await command(page, "Export draw.io");
    // The canvas-painted loss dialog has no DOM buttons; confirm at this viewport.
    await page.mouse.move(727, 483);
    await expect.poll(() => canvas.evaluate(
        element => (element as HTMLCanvasElement).style.cursor,
    )).toBe("pointer");
    await page.mouse.click(727, 483);
    await frames(page);
    const exported = (await readFile(await (await pending).path())).toString("utf8");
    expect(exported.match(/shape=doubleEllipse;perimeter=ellipsePerimeter;/g)).toHaveLength(2);
    expect(exported).toContain("exitX=1;exitY=1;");
    expect(exported).toContain("entryX=0;entryY=0;");

    await canvas.focus();
    await page.keyboard.press("Control+z");
    await frames(page);
    expect(activeSheet(await saveAs(page, "double-ellipses-undo.thinoda"))).toEqual(baseline);
    await canvas.focus();
    await page.keyboard.press("Control+Shift+z");
    await frames(page);
    expect(activeSheet(await saveAs(page, "double-ellipses-redo.thinoda"))).toEqual(saved);
});
