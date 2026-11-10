import { expect, test, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { colorBounds, command, frames, saveAs, zipEntry } from "./board-browser-helpers";

test.use({
    viewport: { width: 1000, height: 800 },
    deviceScaleFactor: 1,
    locale: "en-US",
    colorScheme: "light",
    serviceWorkers: "block",
    screenshot: "only-on-failure",
});
test.setTimeout(180_000);

type Rounding = {
    radius: number;
    stencil: string;
    mode: string;
    rounded_perimeter?: boolean;
};

async function savedRounding(page: Page, name: string) {
    const archive = await saveAs(page, name);
    const manifest: { version: number } =
        JSON.parse(zipEntry(archive, "manifest.json").toString("utf8"));
    const workbook: {
        active_sheet: number;
        sheets: { canvas: { objects: {
            kind: { shape?: { label?: string } };
            rounding?: Rounding;
        }[] } }[];
    } = JSON.parse(zipEntry(archive, "content.json").toString("utf8"));
    const shape = workbook.sheets[workbook.active_sheet].canvas.objects.find(
        object => object.kind.shape?.label === "Browser rectangle",
    );
    if (!shape?.rounding) throw new Error("Missing imported rounded rectangle");
    return { archive, manifest, rounding: shape.rounding };
}

test("user can edit, undo and export an imported rounded rectangle", async ({ page }) => {
    await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    const canvas = page.locator("#thinoda_canvas");
    const xml = `<mxfile><diagram name="Rounding"><mxGraphModel adaptiveColors="simple"><root>
        <mxCell id="0"/><mxCell id="1" parent="0"/>
        <mxCell id="rounded" value="Browser rectangle" vertex="1" parent="1"
          style="shape=rectangle;rounded=1;arcSize=30;roundedPerimeter=1;fillColor=#D0274F;strokeColor=#B01137;">
          <mxGeometry x="200" y="200" width="100" height="60" as="geometry"/>
        </mxCell>
        </root></mxGraphModel></diagram></mxfile>`;
    await canvas.drop({
        files: { name: "rounded.drawio", mimeType: "application/xml", buffer: Buffer.from(xml) },
    });
    await frames(page);
    const color = [[208, 39, 79]] as const;
    await expect.poll(async () => (
        await colorBounds(page, await page.screenshot(), color, 5)
    )[0].count, { timeout: 30_000 }).toBeGreaterThan(500);

    const baseline = await savedRounding(page, "rounded-baseline.thinoda");
    expect(baseline.manifest.version).toBe(23);
    expect(baseline.rounding).toEqual({
        radius: 30, stencil: "standard", mode: "relative", rounded_perimeter: true,
    });

    const bounds = (await colorBounds(page, await page.screenshot(), color, 5))[0];
    const box = await canvas.boundingBox();
    if (!box) throw new Error("Missing board canvas");
    await canvas.click({ position: {
        x: (bounds.minX + bounds.maxX) / 2 - box.x,
        y: (bounds.minY + bounds.maxY) / 2 - box.y,
    } });
    await frames(page);
    // The egui inspector is rendered in the canvas rather than as DOM controls.
    await page.mouse.move(915, 710);
    await page.mouse.wheel(0, 500);
    for (let i = 0; i < 5; i++) await frames(page);
    await canvas.click({ position: { x: 738, y: 525 } });
    await frames(page);
    expect((await savedRounding(page, "rounded-absolute.thinoda")).rounding).toEqual({
        radius: 36, stencil: "standard", mode: "absolute-arc", rounded_perimeter: true,
    });

    await page.keyboard.press("Control+z");
    await frames(page);
    expect((await savedRounding(page, "rounded-undo.thinoda")).rounding)
        .toEqual(baseline.rounding);
    await page.keyboard.press("Control+Shift+z");
    await frames(page);
    expect((await savedRounding(page, "rounded-redo.thinoda")).rounding).toEqual({
        radius: 36, stencil: "standard", mode: "absolute-arc", rounded_perimeter: true,
    });

    await canvas.click({ position: { x: 960, y: 557 } });
    const input = page.locator("input[type='text']");
    await expect(input).toBeFocused();
    await input.press("Control+a");
    await input.fill("12.5");
    await input.press("Enter");
    await frames(page);
    await canvas.click({ position: { x: 738, y: 589 } });
    await frames(page);
    expect((await savedRounding(page, "rounded-edited.thinoda")).rounding).toEqual({
        radius: 25, stencil: "standard", mode: "absolute-arc",
    });

    await page.evaluate(() => { window.prompt = () => "rounded-export.drawio"; });
    const pending = page.waitForEvent("download", { timeout: 30_000 });
    await command(page, "Export draw.io");
    const exported = (await readFile(await (await pending).path())).toString("utf8");
    expect(exported).toContain(
        "shape=rectangle;rounded=1;arcSize=25;absoluteArcSize=1;roundedPerimeter=0;",
    );
});
