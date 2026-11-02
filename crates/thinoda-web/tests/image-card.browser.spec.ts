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
import { readFile } from "node:fs/promises";
import { inflateRawSync } from "node:zlib";

test.use({
    viewport: { width: 1252, height: 1278 },
    deviceScaleFactor: 1,
    screenshot: "only-on-failure",
});
test.setTimeout(120_000);

// Read the named entry from a small, single-disk native document ZIP. Both
// stored and deflated entries are supported; no third-party ZIP package is needed.
function zipEntry(zip: Buffer, name: string): Buffer {
    const end = zip.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
    if (end < 0) throw new Error("Missing ZIP directory");
    const count = zip.readUInt16LE(end + 10);
    let offset = zip.readUInt32LE(end + 16);
    for (let i = 0; i < count; i++) {
        if (zip.readUInt32LE(offset) !== 0x02014b50) throw new Error("Invalid ZIP directory");
        const method = zip.readUInt16LE(offset + 10);
        const size = zip.readUInt32LE(offset + 20);
        const nameLength = zip.readUInt16LE(offset + 28);
        const extraLength = zip.readUInt16LE(offset + 30);
        const commentLength = zip.readUInt16LE(offset + 32);
        const localOffset = zip.readUInt32LE(offset + 42);
        const entryName = zip.toString("utf8", offset + 46, offset + 46 + nameLength);
        if (entryName === name) {
            if (zip.readUInt32LE(localOffset) !== 0x04034b50) throw new Error("Invalid ZIP entry");
            const start = localOffset + 30 + zip.readUInt16LE(localOffset + 26)
                + zip.readUInt16LE(localOffset + 28);
            const bytes = zip.subarray(start, start + size);
            if (method === 0) return bytes;
            if (method === 8) return inflateRawSync(bytes);
            throw new Error(`Unsupported ZIP compression method ${method}`);
        }
        offset += 46 + nameLength + extraLength + commentLength;
    }
    throw new Error(`Missing ZIP entry ${name}`);
}

function documentJson(zip: Buffer, name: string) {
    return JSON.parse(zipEntry(zip, name).toString("utf8"));
}

async function frames(page: Page) {
    await page.evaluate(() => new Promise<void>((resolve) => {
        requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
    }));
}

async function command(page: Page, name: string) {
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+Shift+p");
    await frames(page);
    const input = page.locator("input[type='text']");
    await expect(input).toBeVisible();
    await input.pressSequentially(name);
    // egui updates palette matches on its next frame; submitting immediately can
    // execute the previous first result instead of the typed command.
    await page.waitForTimeout(300);
    await input.press("Enter");
    await frames(page);
}

async function imagePicker(page: Page, file?: { name: string; mimeType: string; buffer: Buffer }) {
    const pending = page.waitForEvent("filechooser");
    await command(page, "Image card");
    const picker = await pending;
    await picker.setFiles(file ?? []);
}

async function saveAs(page: Page, name: string): Promise<Buffer> {
    await page.evaluate((fileName) => { window.prompt = () => fileName; }, name);
    const pending = page.waitForEvent("download", { timeout: 30_000 });
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+Shift+s");
    const download = await pending;
    expect(download.suggestedFilename()).toBe(name);
    return readFile(await download.path());
}

async function jpeg(page: Page, side: number, quality: number): Promise<Buffer> {
    const dataUrl = await page.evaluate(({ side, quality }) => {
        const canvas = document.createElement("canvas");
        canvas.width = canvas.height = side;
        const context = canvas.getContext("2d")!;
        const pixels = context.createImageData(side, side);
        let state = 0x75a3b4c9;
        for (let i = 0; i < pixels.data.length; i += 4) {
            state ^= state << 13;
            state ^= state >>> 17;
            state ^= state << 5;
            pixels.data[i] = state & 255;
            pixels.data[i + 1] = (state >>> 8) & 255;
            pixels.data[i + 2] = (state >>> 16) & 255;
            pixels.data[i + 3] = 255;
        }
        context.putImageData(pixels, 0, 0);
        return canvas.toDataURL("image/jpeg", quality);
    }, { side, quality });
    return Buffer.from(dataUrl.split(",")[1], "base64");
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
