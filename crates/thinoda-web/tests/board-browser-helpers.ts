import { expect, type Page } from "@playwright/test";
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

export async function saveAs(page: Page, name: string): Promise<Buffer> {
    await page.evaluate((fileName) => { window.prompt = () => fileName; }, name);
    const pending = page.waitForEvent("download", { timeout: 30_000 });
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Control+Shift+s");
    const download = await pending;
    expect(download.suggestedFilename()).toBe(name);
    return readFile(await download.path());
}
