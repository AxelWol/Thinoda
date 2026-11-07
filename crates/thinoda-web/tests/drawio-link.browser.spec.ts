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
    canvas: { objects: { kind: { shape?: { label?: string } } }[] };
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

async function enterLink(page: Page) {
    await command(page, "Import draw.io link");
    await page.keyboard.insertText(publishedLink());
    await frames(page);
    await clickAction(page, 850, 820);
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
