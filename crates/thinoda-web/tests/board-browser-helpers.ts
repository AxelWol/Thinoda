import { chromium, expect, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { inflateRawSync } from "node:zlib";

export function zipEntry(zip: Buffer, name: string): Buffer {
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

export async function frames(page: Page) {
    await page.evaluate(() => new Promise<void>((resolve) => {
        requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
    }));
}

export async function drag(
    page: Page,
    from: [number, number],
    to: [number, number],
    steps = 8,
) {
    await page.mouse.move(...from);
    await page.mouse.down();
    await frames(page);
    await page.mouse.move(...to, { steps });
    await frames(page);
    await page.mouse.up();
    await frames(page);
}

export async function touchEvents(
    page: Page,
    events: { type: "touchstart" | "touchmove" | "touchend" | "touchcancel"; x: number; y: number }[],
) {
    await page.evaluate((events) => {
        const canvas = document.querySelector("#thinoda_canvas");
        if (!(canvas instanceof HTMLCanvasElement)) throw new Error("Missing board canvas");
        const createTouch = Reflect.get(document, "createTouch");
        const createTouchList = Reflect.get(document, "createTouchList");
        const legacy = typeof createTouch === "function" && typeof createTouchList === "function";
        for (const { type, x, y } of events) {
            const touch: Touch = legacy
                ? createTouch.call(document, window, canvas, 1, x, y, x, y, x, y)
                : new Touch({
                    identifier: 1, target: canvas, clientX: x, clientY: y,
                    pageX: x, pageY: y, screenX: x, screenY: y,
                });
            if (touch.clientX !== x || touch.clientY !== y) {
                throw new Error("Synthetic touch has incorrect client coordinates");
            }
            const active = type === "touchend" || type === "touchcancel" ? [] : [touch];
            const list = (items: Touch[]) => legacy ? createTouchList.call(document, ...items) : items;
            canvas.dispatchEvent(new TouchEvent(type, {
                bubbles: true,
                cancelable: true,
                touches: list(active),
                targetTouches: list(active),
                changedTouches: list([touch]),
            }));
        }
    }, events);
}

export async function command(page: Page, name: string) {
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+Shift+p");
    await frames(page);
    const input = page.locator("input[type='text']");
    await expect(input).toBeFocused();
    await input.pressSequentially(name);
    // Let egui update palette matches before submitting.
    await page.waitForTimeout(300);
    await input.press("Enter");
    await frames(page);
}

export async function imagePicker(page: Page, file?: { name: string; mimeType: string; buffer: Buffer }) {
    const pending = page.waitForEvent("filechooser");
    await command(page, "Image card");
    const picker = await pending;
    await picker.setFiles(file ?? []);
}

export async function jpeg(page: Page, side: number, quality: number): Promise<Buffer> {
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

export async function largeJpeg(page: Page): Promise<Buffer> {
    // Browser JPEG encoders produce different sizes for the same quality.
    let image = await jpeg(page, 1950, 0.98);
    for (const quality of [0.95, 0.9, 0.85]) {
        if (image.length <= 8 * 1024 * 1024) break;
        image = await jpeg(page, 1950, quality);
    }
    expect(image.length).toBeGreaterThan(4 * 1024 * 1024);
    expect(image.length).toBeLessThanOrEqual(8 * 1024 * 1024);
    return image;
}

export async function saveAs(page: Page, name: string): Promise<Buffer> {
    await page.evaluate((fileName) => { window.prompt = () => fileName; }, name);
    const pending = page.waitForEvent("download", { timeout: 30_000 });
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+Shift+s");
    const download = await pending;
    expect(download.suggestedFilename()).toBe(name);
    return readFile(await download.path());
}

export async function colorBounds(
    page: Page,
    screenshot: Buffer,
    colors: readonly (readonly number[])[],
    tolerance = 25,
) {
    return page.evaluate(async ({ screenshot, colors, tolerance }) => {
        const image = new Image();
        image.src = `data:image/png;base64,${screenshot}`;
        await image.decode();
        const canvas = document.createElement("canvas");
        canvas.width = image.naturalWidth;
        canvas.height = image.naturalHeight;
        const context = canvas.getContext("2d");
        if (!context) throw new Error("Cannot inspect the rendered image");
        context.drawImage(image, 0, 0);
        const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
        const bounds = colors.map(() => ({
            count: 0, minX: canvas.width, minY: canvas.height, maxX: -1, maxY: -1,
        }));
        for (let y = 0; y < canvas.height; y++) {
            for (let x = 0; x < canvas.width; x++) {
                const pixel = (y * canvas.width + x) * 4;
                for (let i = 0; i < colors.length; i++) {
                    if (colors[i].every((value, channel) => Math.abs(pixels[pixel + channel] - value) < tolerance)) {
                        const region = bounds[i];
                        region.count++;
                        region.minX = Math.min(region.minX, x);
                        region.minY = Math.min(region.minY, y);
                        region.maxX = Math.max(region.maxX, x);
                        region.maxY = Math.max(region.maxY, y);
                    }
                }
            }
        }
        return bounds;
    }, { screenshot: screenshot.toString("base64"), colors, tolerance });
}

export async function withPdfViewer(bytes: Buffer, inspect: (viewer: Page) => Promise<void>): Promise<void> {
    // The full Chromium build renders PDFs; headless-shell downloads them instead.
    const renderer = await chromium.launch({
        channel: "chromium",
        executablePath: process.env.THINODA_PDF_BROWSER_EXECUTABLE || undefined,
        headless: true,
        args: ["--enable-unsafe-swiftshader"],
    });
    try {
        const viewer = await renderer.newPage({
            viewport: { width: 1252, height: 1278 },
            deviceScaleFactor: 1,
        });
        const url = "http://127.0.0.1:9/downloaded.pdf";
        await viewer.route(url, (route) => route.fulfill({
            status: 200, contentType: "application/pdf", body: bytes,
        }));
        const response = await viewer.goto(`${url}#toolbar=0&navpanes=0&view=Fit`);
        expect(response?.status()).toBe(200);
        expect(await viewer.evaluate(() => document.contentType)).toBe("application/pdf");
        await inspect(viewer);
    } finally {
        await renderer.close();
    }
}
