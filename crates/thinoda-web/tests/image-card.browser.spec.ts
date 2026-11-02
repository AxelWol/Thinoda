/**
 * Browser regression for image cards on rootless boards.
 *
 * Serve a freshly built Thinoda web bundle, then run with @playwright/test:
 *   THINODA_WEB_URL=http://127.0.0.1:8080 playwright test image-card.browser.spec.ts
 *
 * The test generates its own synthetic JPEGs. It deliberately does not assume
 * where documents are stored, so it can run with either browser storage backend.
 */
import { expect, test, type Page } from "@playwright/test";
import { command, frames, imagePicker, jpeg, saveAs, zipEntry } from "./board-browser-helpers";

test.use({
    viewport: { width: 1252, height: 1278 },
    deviceScaleFactor: 1,
    screenshot: "only-on-failure",
});
test.setTimeout(120_000);

function documentJson(zip: Buffer, name: string) {
    return JSON.parse(zipEntry(zip, name).toString("utf8"));
}

async function boardCenter(page: Page): Promise<Buffer> {
    return page.screenshot({
        type: "png",
        scale: "css",
        clip: { x: 300, y: 480, width: 360, height: 350 },
    });
}

async function changedPixels(page: Page, before: Buffer, after: Buffer): Promise<number> {
    return page.evaluate(async ({ before, after }) => {
        const load = async (data: string) => {
            const image = new Image();
            image.src = `data:image/png;base64,${data}`;
            await image.decode();
            return image;
        };
        const [left, right] = await Promise.all([load(before), load(after)]);
        const canvas = document.createElement("canvas");
        canvas.width = left.width;
        canvas.height = left.height;
        const context = canvas.getContext("2d")!;
        context.drawImage(left, 0, 0);
        const a = context.getImageData(0, 0, canvas.width, canvas.height).data;
        context.drawImage(right, 0, 0);
        const b = context.getImageData(0, 0, canvas.width, canvas.height).data;
        let changed = 0;
        for (let i = 0; i < a.length; i += 4) {
            if (Math.abs(a[i] - b[i]) + Math.abs(a[i + 1] - b[i + 1])
                + Math.abs(a[i + 2] - b[i + 2]) > 24) changed++;
        }
        return changed;
    }, { before: before.toString("base64"), after: after.toString("base64") });
}

test("large picker uploads remain transient and rootless image cards undo atomically", async ({ page }) => {
    await page.addInitScript(() => {
        const setItem = Storage.prototype.setItem;
        (window as any).__imageStagingWrites = [];
        Storage.prototype.setItem = function (key, value) {
            if (key.includes("/ImageUploads/")) (window as any).__imageStagingWrites.push(key);
            return setItem.call(this, key, value);
        };
    });
    await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(page.locator("#loading")).toBeHidden({ timeout: 60_000 });
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+n");
    await frames(page);
    await command(page, "Hide central topic");
    await page.keyboard.press("Escape");
    await frames(page);

    const baseline = await saveAs(page, "image-baseline.thinoda");
    const baselineContent = documentJson(baseline, "content.json");
    expect(baselineContent.sheets[0].root_mode).toBe("hidden");
    expect(documentJson(baseline, "manifest.json").resources).toHaveLength(0);

    // A canceled picker must not create a topic or a resource.
    await imagePicker(page);
    await page.waitForTimeout(300);
    const canceled = await saveAs(page, "image-canceled.thinoda");
    expect(documentJson(canceled, "content.json").sheets).toEqual(baselineContent.sheets);
    expect(documentJson(canceled, "manifest.json").resources).toHaveLength(0);

    const large = await jpeg(page, 1950, 0.98);
    expect(large.length).toBeGreaterThan(4 * 1024 * 1024);
    expect(large.length).toBeLessThanOrEqual(8 * 1024 * 1024);
    const largeFile = { name: "synthetic-large.jpg", mimeType: "image/jpeg", buffer: large };

    // A rejected browser read must clear the request and leave the board intact.
    await page.evaluate(() => {
        const original = Blob.prototype.arrayBuffer;
        (window as any).__imageReadFailed = false;
        (window as any).__restoreImageRead = () => { Blob.prototype.arrayBuffer = original; };
        Blob.prototype.arrayBuffer = function () {
            if (this instanceof File && this.name === "synthetic-large.jpg") {
                (window as any).__imageReadFailed = true;
                return Promise.reject(new Error("synthetic read failure"));
            }
            return original.call(this);
        };
    });
    await imagePicker(page, largeFile);
    await page.waitForFunction(() => (window as any).__imageReadFailed);
    await page.waitForTimeout(300);
    await page.evaluate(() => (window as any).__restoreImageRead());
    const failed = await saveAs(page, "image-read-failed.thinoda");
    expect(documentJson(failed, "content.json").sheets).toEqual(baselineContent.sheets);
    expect(documentJson(failed, "manifest.json").resources).toHaveLength(0);

    // The full-size JPEG must visibly appear even though its base64 representation
    // is larger than typical localStorage quota. Inspect only the board center so
    // a status toast cannot satisfy this assertion.
    const before = await boardCenter(page);
    await imagePicker(page, largeFile);
    await expect.poll(async () => changedPixels(page, before, await boardCenter(page)), {
        timeout: 20_000,
    }).toBeGreaterThan(5_000);
    expect(await page.evaluate(() => (window as any).__imageStagingWrites)).toEqual([]);
    expect(await page.evaluate(() => Object.keys(localStorage).filter((key) => key.includes("/ImageUploads/"))))
        .toEqual([]);

    // One undo removes the card and its resource. Saving the small result gives
    // a structural assertion without depending on the large-document backend.
    await page.keyboard.press("Control+z");
    await frames(page);
    const undone = await saveAs(page, "image-undone.thinoda");
    expect(documentJson(undone, "content.json").sheets).toEqual(baselineContent.sheets);
    expect(documentJson(undone, "manifest.json").resources).toHaveLength(0);

    // A smaller image can be saved on both old and new storage backends; inspect
    // the actual native archive, including the exact resource bytes.
    const small = await jpeg(page, 64, 0.9);
    await imagePicker(page, { name: "synthetic-small.jpg", mimeType: "image/jpeg", buffer: small });
    await page.waitForTimeout(500);
    const saved = await saveAs(page, "image-small.thinoda");
    const sheet = documentJson(saved, "content.json").sheets[0];
    const cards: any[] = Object.values(sheet.topics).filter((topic: any) => topic.card?.kind === "image");
    expect(sheet.root_mode).toBe("hidden");
    expect(cards).toHaveLength(1);
    expect(sheet.floating).toContain(cards[0].id);
    expect(cards[0].position).toEqual({ x: 0, y: 0 });
    const resources = documentJson(saved, "manifest.json").resources;
    expect(resources).toHaveLength(1);
    expect(resources[0]).toMatchObject({ name: "synthetic-small.jpg", media_type: "image/jpeg", size: small.length });
    expect(cards[0].image).toBe(`thinoda-resource:${resources[0].id}`);
    expect(zipEntry(saved, `resources/${resources[0].id}`)).toEqual(small);
});
