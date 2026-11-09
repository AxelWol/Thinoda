import { expect, test, type Page } from "@playwright/test";
import { command, frames, saveAs, zipEntry } from "./board-browser-helpers";

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
    canvas: {
        objects: {
            kind: {
                shape?: { label?: string };
                connector?: {
                    arrow_start: boolean;
                    arrow_end: boolean;
                    start_marker?: { kind: string };
                    end_marker?: { kind: string };
                    start_marker_size?: number;
                    end_marker_size?: number;
                    start_marker_unfilled?: boolean;
                    end_marker_unfilled?: boolean;
                };
            };
        }[];
    };
};
type Content = { active_sheet: number; sheets: Sheet[] };

function contentFrom(zip: Buffer): Content {
    return JSON.parse(zipEntry(zip, "content.json").toString("utf8"));
}

function publishedLink() {
    const xml = `<mxfile><diagram name="Linked page"><mxGraphModel><root>`
        + `<mxCell id="0"/><mxCell id="1" parent="0"/>`
        + `<mxCell id="shape" value="Imported link shape" style="dashed=1;" vertex="1" parent="1">`
        + `<mxGeometry x="20" y="30" width="100" height="60" as="geometry"/>`
        + `</mxCell></root></mxGraphModel></diagram></mxfile>`;
    return `https://example.invalid/published#R${encodeURIComponent(xml)}`;
}

async function clickAction(page: Page, x: number, y: number) {
    await page.mouse.move(x, y);
    await expect.poll(() => page.locator("#thinoda_canvas").evaluate(
        (canvas) => (canvas as HTMLCanvasElement).style.cursor,
    )).toBe("pointer");
    await page.mouse.click(x, y);
    await frames(page);
}

async function enterLink(page: Page, link = publishedLink()) {
    await command(page, "Import draw.io link");
    await page.keyboard.insertText(link);
    await frames(page);
    await page.keyboard.press("Tab");
    await expect(page.locator("#thinoda_canvas")).toBeFocused();
    await page.keyboard.press("Enter");
    await frames(page);
}

test("imports published links locally with cancelable conversion consent", async ({ page }) => {
    const externalRequests: string[] = [];
    await page.route("**/*", async (route) => {
        const url = new URL(route.request().url());
        if (!["127.0.0.1", "localhost", "[::1]"].includes(url.hostname)) {
            externalRequests.push(url.href);
            await route.abort();
        } else {
            await route.continue();
        }
    });

    await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+n");
    await frames(page);
    await command(page, "Hide central topic");
    await command(page, "Shape tool");
    await page.mouse.click(650, 400);
    await frames(page);
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("F2");
    await frames(page);
    await expect(page.locator("input[type='text']")).toBeFocused();
    await page.locator("input[type='text']").pressSequentially("Existing source");
    await page.keyboard.press("Enter");
    await frames(page);

    const before = contentFrom(await saveAs(page, "link-source.thinoda"));
    expect(before.sheets[before.active_sheet].canvas.objects.some(
        (object) => object.kind.shape?.label === "Existing source",
    )).toBe(true);

    await enterLink(page);
    await page.keyboard.press("Escape");
    await frames(page);
    const canceled = contentFrom(await saveAs(page, "link-after-cancel.thinoda"));
    expect(canceled.sheets[canceled.active_sheet]).toEqual(before.sheets[before.active_sheet]);

    await enterLink(page);
    await clickAction(page, 670, 832);
    const imported = contentFrom(await saveAs(page, "link-imported.thinoda"));
    const sheet = imported.sheets[imported.active_sheet];
    expect(sheet.title).toBe("Linked page");
    expect(sheet.canvas.objects.some(
        (object) => object.kind.shape?.label === "Imported link shape",
    )).toBe(true);
    expect(externalRequests).toEqual([]);
});

test("imports supported marker styles into a versioned native workbook", async ({ page }) => {
    const xml = `<mxfile><diagram name="Marker page"><mxGraphModel><root>`
        + `<mxCell id="0"/><mxCell id="1" parent="0"/>`
        + `<mxCell id="edge" style="startArrow=block;endArrow=open;strokeWidth=4;"`
        + ` edge="1" parent="1"><mxGeometry relative="1" as="geometry">`
        + `<mxPoint x="150" y="200" as="sourcePoint"/>`
        + `<mxPoint x="450" y="200" as="targetPoint"/>`
        + `</mxGeometry></mxCell></root></mxGraphModel></diagram></mxfile>`;
    await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+n");
    await frames(page);
    await enterLink(page, `#R${encodeURIComponent(xml)}`);

    const archive = await saveAs(page, "link-markers.thinoda");
    const content = contentFrom(archive);
    const sheet = content.sheets[content.active_sheet];
    expect(sheet.title).toBe("Marker page");
    const connector = sheet.canvas.objects.find((object) => object.kind.connector)?.kind.connector;
    expect(connector).toMatchObject({
        arrow_start: true,
        arrow_end: true,
        start_marker: { kind: "block" },
        end_marker: { kind: "open" },
    });
    const manifest: { version: number } = JSON.parse(zipEntry(archive, "manifest.json").toString("utf8"));
    expect(manifest.version).toBe(11);
});

test("imports classic and thin markers without changing legacy arrows", async ({ page }) => {
    const xml = `<mxfile><diagram name="Classic and thin"><mxGraphModel><root>`
        + `<mxCell id="0"/><mxCell id="1" parent="0"/>`
        + `<mxCell id="thin" style="startArrow=classicThin;endArrow=openThin;" edge="1" parent="1">`
        + `<mxGeometry relative="1" as="geometry"><mxPoint x="150" y="200" as="sourcePoint"/>`
        + `<mxPoint x="450" y="200" as="targetPoint"/></mxGeometry></mxCell>`
        + `<mxCell id="default" edge="1" parent="1">`
        + `<mxGeometry relative="1" as="geometry"><mxPoint x="150" y="300" as="sourcePoint"/>`
        + `<mxPoint x="450" y="300" as="targetPoint"/></mxGeometry></mxCell>`
        + `<mxCell id="legacy" style="endArrow=classic;thinodaLegacyEndArrow=1;" edge="1" parent="1">`
        + `<mxGeometry relative="1" as="geometry"><mxPoint x="150" y="400" as="sourcePoint"/>`
        + `<mxPoint x="450" y="400" as="targetPoint"/></mxGeometry></mxCell>`
        + `</root></mxGraphModel></diagram></mxfile>`;
    await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+n");
    await frames(page);
    await enterLink(page, `#R${encodeURIComponent(xml)}`);

    const archive = await saveAs(page, "link-classic-thin.thinoda");
    const content = contentFrom(archive);
    const sheet = content.sheets[content.active_sheet];
    expect(sheet.title).toBe("Classic and thin");
    const connectors = sheet.canvas.objects
        .map((object) => object.kind.connector)
        .filter((connector) => connector !== undefined);
    expect(connectors).toHaveLength(3);
    expect(connectors).toEqual(expect.arrayContaining([
        expect.objectContaining({
            arrow_start: true,
            arrow_end: true,
            start_marker: { kind: "classic-thin" },
            end_marker: { kind: "open-thin" },
        }),
        expect.objectContaining({
            arrow_end: true,
            end_marker: { kind: "classic" },
        }),
    ]));
    expect(connectors.some((connector) => connector?.arrow_end && !connector.end_marker)).toBe(true);
    const manifest: { version: number } = JSON.parse(zipEntry(archive, "manifest.json").toString("utf8"));
    expect(manifest.version).toBe(12);
});

test("imports independent marker sizes into a versioned native workbook", async ({ page }) => {
    const xml = `<mxfile><diagram name="Sized markers"><mxGraphModel><root>`
        + `<mxCell id="0"/><mxCell id="1" parent="0"/>`
        + `<mxCell id="edge" style="startArrow=classic;startSize=12.5;endArrow=open;endSize=0;"`
        + ` edge="1" parent="1"><mxGeometry relative="1" as="geometry">`
        + `<mxPoint x="150" y="200" as="sourcePoint"/>`
        + `<mxPoint x="450" y="200" as="targetPoint"/>`
        + `</mxGeometry></mxCell></root></mxGraphModel></diagram></mxfile>`;
    await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+n");
    await frames(page);
    await enterLink(page, `#R${encodeURIComponent(xml)}`);

    const archive = await saveAs(page, "link-sized-markers.thinoda");
    const content = contentFrom(archive);
    const sheet = content.sheets[content.active_sheet];
    expect(sheet.title).toBe("Sized markers");
    const connector = sheet.canvas.objects.find((object) => object.kind.connector)?.kind.connector;
    expect(connector).toMatchObject({
        start_marker: { kind: "classic" },
        start_marker_size: 12.5,
        end_marker: { kind: "open" },
        end_marker_size: 0,
    });
    const manifest: { version: number } = JSON.parse(zipEntry(archive, "manifest.json").toString("utf8"));
    expect(manifest.version).toBe(13);
});

test("imports independent hollow marker fills into a versioned native workbook", async ({ page }) => {
    const xml = `<mxfile><diagram name="Hollow markers"><mxGraphModel><root>`
        + `<mxCell id="0"/><mxCell id="1" parent="0"/>`
        + `<mxCell id="start" style="startArrow=block;startFill=0;endArrow=classic;endFill=1;"`
        + ` edge="1" parent="1"><mxGeometry relative="1" as="geometry">`
        + `<mxPoint x="150" y="200" as="sourcePoint"/><mxPoint x="450" y="200" as="targetPoint"/>`
        + `</mxGeometry></mxCell>`
        + `<mxCell id="end" style="startArrow=open;startFill=1;endArrow=classicThin;endFill=0;"`
        + ` edge="1" parent="1"><mxGeometry relative="1" as="geometry">`
        + `<mxPoint x="150" y="300" as="sourcePoint"/><mxPoint x="450" y="300" as="targetPoint"/>`
        + `</mxGeometry></mxCell></root></mxGraphModel></diagram></mxfile>`;
    await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+n");
    await frames(page);
    await enterLink(page, `#R${encodeURIComponent(xml)}`);

    const archive = await saveAs(page, "link-hollow-markers.thinoda");
    const content = contentFrom(archive);
    const sheet = content.sheets[content.active_sheet];
    expect(sheet.title).toBe("Hollow markers");
    const connectors = sheet.canvas.objects
        .map((object) => object.kind.connector)
        .filter((connector) => connector !== undefined);
    expect(connectors).toHaveLength(2);
    const start = connectors.find((connector) => connector.start_marker?.kind === "block");
    const end = connectors.find((connector) => connector.end_marker?.kind === "classic-thin");
    expect(start).toMatchObject({
        start_marker_unfilled: true,
        end_marker: { kind: "classic" },
    });
    expect(start?.end_marker_unfilled).toBeUndefined();
    expect(end).toMatchObject({
        start_marker: { kind: "open" },
        end_marker_unfilled: true,
    });
    expect(end?.start_marker_unfilled).toBeUndefined();
    const manifest: { version: number } = JSON.parse(zipEntry(archive, "manifest.json").toString("utf8"));
    expect(manifest.version).toBe(14);
});

test("imports filled and hollow diamond markers into a versioned native workbook", async ({ page }) => {
    const xml = `<mxfile><diagram name="Diamond markers"><mxGraphModel><root>`
        + `<mxCell id="0"/><mxCell id="1" parent="0"/>`
        + `<mxCell id="edge" style="startArrow=diamond;startSize=12.5;startFill=0;endArrow=diamondThin;endSize=8;endFill=1;"`
        + ` edge="1" parent="1"><mxGeometry relative="1" as="geometry">`
        + `<mxPoint x="150" y="200" as="sourcePoint"/><mxPoint x="450" y="200" as="targetPoint"/>`
        + `</mxGeometry></mxCell></root></mxGraphModel></diagram></mxfile>`;
    await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+n");
    await frames(page);
    await enterLink(page, `#R${encodeURIComponent(xml)}`);

    const archive = await saveAs(page, "link-diamond-markers.thinoda");
    const content = contentFrom(archive);
    const sheet = content.sheets[content.active_sheet];
    expect(sheet.title).toBe("Diamond markers");
    const connector = sheet.canvas.objects.find((object) => object.kind.connector)?.kind.connector;
    expect(connector).toMatchObject({
        arrow_start: true,
        arrow_end: true,
        start_marker: { kind: "diamond" },
        start_marker_size: 12.5,
        start_marker_unfilled: true,
        end_marker: { kind: "diamond-thin" },
        end_marker_size: 8,
    });
    expect(connector?.end_marker_unfilled).toBeUndefined();
    const manifest: { version: number } = JSON.parse(zipEntry(archive, "manifest.json").toString("utf8"));
    expect(manifest.version).toBe(15);
});

test("imports filled and hollow oval markers into a versioned native workbook", async ({ page }) => {
    const xml = `<mxfile><diagram name="Oval markers"><mxGraphModel><root>`
        + `<mxCell id="0"/><mxCell id="1" parent="0"/>`
        + `<mxCell id="edge" style="startArrow=oval;startSize=12.5;startFill=0;endArrow=oval;endSize=8;endFill=1;"`
        + ` edge="1" parent="1"><mxGeometry relative="1" as="geometry">`
        + `<mxPoint x="150" y="200" as="sourcePoint"/><mxPoint x="450" y="200" as="targetPoint"/>`
        + `</mxGeometry></mxCell></root></mxGraphModel></diagram></mxfile>`;
    await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+n");
    await frames(page);
    await enterLink(page, `#R${encodeURIComponent(xml)}`);

    const archive = await saveAs(page, "link-oval-markers.thinoda");
    const content = contentFrom(archive);
    const sheet = content.sheets[content.active_sheet];
    expect(sheet.title).toBe("Oval markers");
    const connector = sheet.canvas.objects.find((object) => object.kind.connector)?.kind.connector;
    expect(connector).toMatchObject({
        arrow_start: true,
        arrow_end: true,
        start_marker: { kind: "oval" },
        start_marker_size: 12.5,
        start_marker_unfilled: true,
        end_marker: { kind: "oval" },
        end_marker_size: 8,
    });
    expect(connector?.end_marker_unfilled).toBeUndefined();
    const manifest: { version: number } = JSON.parse(zipEntry(archive, "manifest.json").toString("utf8"));
    expect(manifest.version).toBe(16);
});

test("imports open half-circle and oval markers into a versioned native workbook", async ({ page }) => {
    const xml = `<mxfile><diagram name="Half-circle markers"><mxGraphModel><root>`
        + `<mxCell id="0"/><mxCell id="1" parent="0"/>`
        + `<mxCell id="edge" style="startArrow=halfCircle;startSize=12.5;startFill=0;endArrow=oval;endSize=8;endFill=1;"`
        + ` edge="1" parent="1"><mxGeometry relative="1" as="geometry">`
        + `<mxPoint x="150" y="200" as="sourcePoint"/><mxPoint x="450" y="200" as="targetPoint"/>`
        + `</mxGeometry></mxCell></root></mxGraphModel></diagram></mxfile>`;
    await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+n");
    await frames(page);
    await enterLink(page, `#R${encodeURIComponent(xml)}`);

    const archive = await saveAs(page, "link-half-circle-markers.thinoda");
    const content = contentFrom(archive);
    const sheet = content.sheets[content.active_sheet];
    expect(sheet.title).toBe("Half-circle markers");
    const connector = sheet.canvas.objects.find((object) => object.kind.connector)?.kind.connector;
    expect(connector).toMatchObject({
        arrow_start: true,
        arrow_end: true,
        start_marker: { kind: "half-circle" },
        start_marker_size: 12.5,
        start_marker_unfilled: true,
        end_marker: { kind: "oval" },
        end_marker_size: 8,
    });
    expect(connector?.end_marker_unfilled).toBeUndefined();
    const manifest: { version: number } = JSON.parse(zipEntry(archive, "manifest.json").toString("utf8"));
    expect(manifest.version).toBe(17);
});

test("imports bar markers into a versioned native workbook", async ({ page }) => {
    const xml = `<mxfile><diagram name="Bar markers"><mxGraphModel><root>`
        + `<mxCell id="0"/><mxCell id="1" parent="0"/>`
        + `<mxCell id="edge" style="startArrow=baseDash;startSize=12.5;startFill=0;endArrow=dash;endSize=0;endFill=1;"`
        + ` edge="1" parent="1"><mxGeometry relative="1" as="geometry">`
        + `<mxPoint x="150" y="200" as="sourcePoint"/><mxPoint x="450" y="200" as="targetPoint"/>`
        + `</mxGeometry></mxCell></root></mxGraphModel></diagram></mxfile>`;
    await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+n");
    await frames(page);
    await enterLink(page, `#R${encodeURIComponent(xml)}`);

    const archive = await saveAs(page, "link-bar-markers.thinoda");
    const content = contentFrom(archive);
    const sheet = content.sheets[content.active_sheet];
    expect(sheet.title).toBe("Bar markers");
    const connector = sheet.canvas.objects.find((object) => object.kind.connector)?.kind.connector;
    expect(connector).toMatchObject({
        arrow_start: true,
        arrow_end: true,
        start_marker: { kind: "base-dash" },
        start_marker_size: 12.5,
        start_marker_unfilled: true,
        end_marker: { kind: "dash" },
        end_marker_size: 0,
    });
    expect(connector?.end_marker_unfilled).toBeUndefined();
    const manifest: { version: number } = JSON.parse(zipEntry(archive, "manifest.json").toString("utf8"));
    expect(manifest.version).toBe(18);
});
