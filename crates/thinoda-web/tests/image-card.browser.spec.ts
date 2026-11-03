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

test.describe("rootless image-card export", () => {
    test.use({ serviceWorkers: "block" });

    test("SVG, PNG and JPEG render the embedded image without modifying the board", async ({ page }) => {
        await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
        await expect(page.locator("#loading")).toBeHidden({ timeout: 60_000 });
        await page.locator("#thinoda_canvas").focus();
        await page.keyboard.press("Control+n");
        await frames(page);
        await command(page, "Hide central topic");
        await page.keyboard.press("Escape");
        await frames(page);

        const fixture = await page.evaluate(() => {
            const canvas = document.createElement("canvas");
            canvas.width = canvas.height = 64;
            const context = canvas.getContext("2d");
            if (!context) throw new Error("Cannot create the image fixture");
            const quadrants: [number, number, string][] = [
                [0, 0, "#1db5f5"], [32, 0, "#f3a322"],
                [0, 32, "#dd3399"], [32, 32, "#39ca71"],
            ];
            for (const [x, y, color] of quadrants) {
                context.fillStyle = color;
                context.fillRect(x, y, 32, 32);
            }
            return canvas.toDataURL("image/jpeg", 0.92).split(",")[1];
        });
        const image = Buffer.from(fixture, "base64");
        const beforeImage = await boardCenter(page);
        await imagePicker(page, { name: "export-image.jpg", mimeType: "image/jpeg", buffer: image });
        await expect.poll(async () => changedPixels(page, beforeImage, await boardCenter(page)), {
            timeout: 20_000,
        }).toBeGreaterThan(500);

        const before = await saveAs(page, "image-export-before.thinoda");
        const beforeContent = documentJson(before, "content.json");
        const beforeManifest = documentJson(before, "manifest.json");
        const sheet = beforeContent.sheets[0];
        const cards = Object.values(sheet.topics) as { id: number; image?: string; card?: { kind: string } }[];
        const imageCards = cards.filter((topic) => topic.card?.kind === "image");
        expect(sheet.root_mode).toBe("hidden");
        expect(imageCards).toHaveLength(1);
        expect(beforeManifest.resources).toHaveLength(1);
        const resource = beforeManifest.resources[0];
        expect(imageCards[0].image).toBe(`thinoda-resource:${resource.id}`);
        expect(zipEntry(before, `resources/${resource.id}`)).toEqual(image);

        const exportFile = async (label: string, name: string) => {
            await page.evaluate((fileName) => { window.prompt = () => fileName; }, name);
            const pending = page.waitForEvent("download", { timeout: 30_000 });
            await command(page, label);
            const download = await pending;
            expect(download.suggestedFilename()).toBe(name);
            return readFile(await download.path());
        };
        const svg = await exportFile("Export SVG", "image-card.svg");
        const scene = await page.evaluate((source) => {
            const doc = new DOMParser().parseFromString(source, "image/svg+xml");
            const embedded = Array.from(doc.querySelectorAll("defs image"));
            const references = Array.from(doc.querySelectorAll("use"));
            const image = embedded[0];
            const href = image?.getAttributeNS("http://www.w3.org/1999/xlink", "href");
            const reference = references.find((element) =>
                element.getAttributeNS("http://www.w3.org/1999/xlink", "href") === `#${image?.id}`);
            return {
                error: doc.querySelector("parsererror")?.textContent ?? null,
                imageCount: embedded.length,
                width: image?.getAttribute("width"),
                height: image?.getAttribute("height"),
                href,
                transform: reference?.getAttribute("transform"),
                viewBox: doc.documentElement.getAttribute("viewBox"),
            };
        }, svg.toString("utf8"));
        expect(scene.error).toBeNull();
        expect(scene.imageCount).toBe(1);
        expect(scene.width).toBe("64");
        expect(scene.height).toBe("64");
        expect(scene.href).toBe(`data:image/jpeg;base64,${image.toString("base64")}`);
        expect(scene.transform).toBeTruthy();
        expect(scene.viewBox).toBeTruthy();
        const transform = /^translate\(([-\d.]+)\s+([-\d.]+)\)\s+scale\(([-\d.]+)\)$/
            .exec(scene.transform!);
        if (!transform || !scene.viewBox) throw new Error("Missing image placement in exported SVG");
        const geometry = {
            x: Number(transform[1]), y: Number(transform[2]), scale: Number(transform[3]),
            viewBox: scene.viewBox.trim().split(/\s+/).map(Number),
        };
        expect(geometry.viewBox).toHaveLength(4);
        expect([...geometry.viewBox, geometry.x, geometry.y, geometry.scale].every(Number.isFinite)).toBe(true);
        expect(geometry.scale).toBeGreaterThan(0);

        const inspect = async (bytes: Buffer, mime: string) => page.evaluate(
            async ({ data, mime, geometry }) => {
                const blob = new Blob([Uint8Array.from(atob(data), (character) => character.charCodeAt(0))], { type: mime });
                const url = URL.createObjectURL(blob);
                try {
                    const image = new Image();
                    image.src = url;
                    await image.decode();
                    const canvas = document.createElement("canvas");
                    canvas.width = image.naturalWidth;
                    canvas.height = image.naturalHeight;
                    const context = canvas.getContext("2d");
                    if (!context) throw new Error("Cannot inspect the exported image");
                    context.drawImage(image, 0, 0);
                    return [[0.25, 0.25], [0.75, 0.25], [0.25, 0.75], [0.75, 0.75]]
                        .map(([x, y]) => {
                            const px = Math.floor((geometry.x + x * 64 * geometry.scale - geometry.viewBox[0])
                                / geometry.viewBox[2] * canvas.width);
                            const py = Math.floor((geometry.y + y * 64 * geometry.scale - geometry.viewBox[1])
                                / geometry.viewBox[3] * canvas.height);
                            if (px < 0 || px >= canvas.width || py < 0 || py >= canvas.height) {
                                throw new Error("Exported image lies outside the viewport");
                            }
                            return Array.from(context.getImageData(px, py, 1, 1).data);
                        });
                } finally {
                    URL.revokeObjectURL(url);
                }
            },
            { data: bytes.toString("base64"), mime, geometry },
        );
        const png = await exportFile("Export PNG", "image-card.png");
        const jpg = await exportFile("Export JPEG", "image-card.jpg");
        expect(png.subarray(0, 8)).toEqual(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]));
        expect(jpg.subarray(0, 3)).toEqual(Buffer.from([0xff, 0xd8, 0xff]));
        const expected = [[29, 181, 245], [243, 163, 34], [221, 51, 153], [57, 202, 113]];
        for (const [bytes, mime] of [[svg, "image/svg+xml"], [png, "image/png"], [jpg, "image/jpeg"]] as const) {
            const samples = await inspect(bytes, mime);
            for (let i = 0; i < expected.length; i++) {
                for (let channel = 0; channel < 3; channel++) {
                    expect(Math.abs(samples[i][channel] - expected[i][channel]), `${mime} quadrant ${i}`)
                        .toBeLessThan(30);
                }
                expect(samples[i][3]).toBe(255);
            }
        }

        const after = await saveAs(page, "image-export-after.thinoda");
        expect(documentJson(after, "content.json")).toEqual(beforeContent);
        expect(documentJson(after, "manifest.json")).toEqual(beforeManifest);
        expect(zipEntry(after, `resources/${resource.id}`)).toEqual(image);
    });
});
