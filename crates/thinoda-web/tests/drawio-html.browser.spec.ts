import { expect, test } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { deflateRawSync } from "node:zlib";
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

function activeSheet(zip: Buffer) {
    const content = JSON.parse(zipEntry(zip, "content.json").toString("utf8"));
    return content.sheets[content.active_sheet];
}

test("opens and drops embedded draw.io HTML without loading page resources", async ({ page }) => {
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
    const canvas = page.locator("#thinoda_canvas");
    await canvas.focus();
    await page.keyboard.press("Control+n");
    await frames(page);
    await page.evaluate(() => {
        Object.defineProperty(window, "showOpenFilePicker", { configurable: true, value: undefined });
    });

    const publishedHtml = await readFile(join(
        __dirname, "..", "..", "thinoda-io", "tests", "fixtures", "published-rectangle.drawio.html",
    ));
    const html = Buffer.from(publishedHtml.toString("utf8")
        .replace("</body>", '<img src="https://example.invalid/never-load.png"></body>'), "utf8");
    const pending = page.waitForEvent("filechooser");
    await command(page, "Open");
    const picker = await pending;
    const accept = await picker.element().getAttribute("accept");
    expect(accept).toContain(".html");
    expect(accept).toContain(".htm");
    await picker.setFiles({ name: "published.drawio.html", mimeType: "text/html", buffer: html });
    await expect.poll(async () => (
        await colorBounds(page, await page.screenshot(), [[213, 232, 212]], 5)
    )[0].count, { timeout: 30_000 }).toBeGreaterThan(200);
    const opened = activeSheet(await saveAs(page, "html-opened.thinoda"));
    expect(opened.title).toBe("Page-1");
    expect(opened.canvas.objects.some(
        (object: { kind: { shape?: { label?: string } } }) => object.kind.shape?.label === "Published PNG",
    )).toBe(true);

    const legacyXml = '<mxfile><diagram name="Page-1"><mxGraphModel><root><mxCell id="0"/>' +
        '<mxCell id="1" parent="0"/><mxCell id="v" value="Dropped legacy HTML" ' +
        'style="rounded=0;fillColor=#17c987;" vertex="1" parent="1">' +
        '<mxGeometry x="350" y="380" width="120" height="60" as="geometry"/></mxCell>' +
        '</root></mxGraphModel></diagram></mxfile>';
    const data = deflateRawSync(Buffer.from(encodeURIComponent(legacyXml))).toString("base64");
    const droppedHtml = Buffer.from(
        `<!doctype html><script src="https://example.invalid/never-load.js"></script>` +
        `<div class="mxgraph"><div style="width:1px;height:1px;overflow:hidden;">${data}</div></div>`,
        "utf8",
    );
    await canvas.drop({ files: { name: "dropped.htm", mimeType: "text/html", buffer: droppedHtml } });
    await expect.poll(async () => (
        await colorBounds(page, await page.screenshot(), [[23, 201, 135]], 5)
    )[0].count, { timeout: 30_000 }).toBeGreaterThan(200);
    const dropped = activeSheet(await saveAs(page, "html-dropped.thinoda"));
    expect(dropped.canvas.objects.some(
        (object: { kind: { shape?: { label?: string } } }) => object.kind.shape?.label === "Dropped legacy HTML",
    )).toBe(true);
    expect(externalRequests).toEqual([]);
});
