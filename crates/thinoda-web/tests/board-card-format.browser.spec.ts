import { expect, test, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { command, frames, saveAs, zipEntry } from "./board-browser-helpers";

test.use({
    locale: "en-US",
    serviceWorkers: "block",
    screenshot: "only-on-failure",
});
test.setTimeout(180_000);

async function insertCard(page: Page, kind: string, title: string) {
    await command(page, kind);
    await page.locator("#thinoda_canvas").focus();
    await page.keyboard.press("Enter");
    await frames(page);
    const input = page.locator("input[type='text']");
    await expect(input).toBeFocused();
    await input.pressSequentially(title);
    await frames(page);
}

async function boldDraft(page: Page) {
    await page.keyboard.press("Alt+F10");
    await frames(page);
    await page.keyboard.press("Space");
    await frames(page);
    await page.keyboard.press("Alt+F10");
    await frames(page);
    await expect(page.locator("input[type='text']")).toBeFocused();
}

for (const viewport of [{ width: 1280, height: 800 }, { width: 800, height: 600 }]) {
    test.describe(`${viewport.width}x${viewport.height}`, () => {
        test.use({ viewport, deviceScaleFactor: 1 });

        test("pointer-created cards retain their first titles while formatting", async ({ page }) => {
            const url = process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/";
            const origin = new URL(url).origin;
            await page.route("**/*", (route) =>
                new URL(route.request().url()).origin === origin
                    ? route.continue()
                    : route.abort(),
            );
            await page.goto(url);
            await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
            await page.locator("#thinoda_canvas").focus();
            await page.keyboard.press("Control+n");
            await frames(page);
            await command(page, "Hide central topic");
            await page.keyboard.press("Escape");
            await frames(page);

            const input = page.locator("input[type='text']");
            for (const [tool, x, title] of [
                ["Sticky note", 220, "Pointer sticky title"],
                ["Text card", 410, "Pointer text title"],
            ] as const) {
                await command(page, tool);
                await page.mouse.click(x, 400);
                await page.keyboard.type(title);
                await frames(page);
                await expect(input).toBeFocused();
                await boldDraft(page);
                await input.pressSequentially(" continued");
                await page.keyboard.press("Enter");
                await frames(page);
            }

            const archive = await saveAs(page, "pointer-cards.thinoda");
            const saved: {
                sheets: {
                    topics: Record<string, {
                        title: string;
                        card?: { kind: string };
                        style?: { bold?: boolean };
                    }>;
                }[];
            } = JSON.parse(zipEntry(archive, "content.json").toString("utf8"));
            const cards = Object.values(saved.sheets[0].topics);
            const sticky = cards.filter((card) => card.card?.kind === "sticky");
            const text = cards.filter((card) => card.card?.kind === "text");
            expect(sticky).toHaveLength(1);
            expect(text).toHaveLength(1);
            expect(sticky[0]).toMatchObject({
                title: "Pointer sticky title continued",
                style: { bold: true },
            });
            expect(text[0]).toMatchObject({
                title: "Pointer text title continued",
                style: { bold: true },
            });
        });

        test("formats card drafts and saves an active font-size edit", async ({ page }) => {
            await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
            await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
            await page.locator("#thinoda_canvas").focus();
            await page.keyboard.press("Control+n");
            await frames(page);
            await command(page, "Hide central topic");
            await page.keyboard.press("Escape");
            await frames(page);

            const input = page.locator("input[type='text']");
            await insertCard(page, "Sticky note", "Browser draft");
            await boldDraft(page);
            await input.pressSequentially(" continued");
            await page.keyboard.press("Enter");
            await frames(page);

            await insertCard(page, "Text card", "Selected browser text");
            await page.keyboard.press("Control+a");
            await frames(page);
            await boldDraft(page);
            await input.pressSequentially("Replacement");
            await page.keyboard.press("Enter");
            await frames(page);

            await page.keyboard.press("F2");
            await frames(page);
            await expect(input).toBeFocused();
            await page.keyboard.press("Control+End");
            await input.pressSequentially(" size");
            await page.keyboard.press("Alt+F10");
            await frames(page);
            for (let index = 0; index < 8; index++) {
                await page.keyboard.press("Tab");
                await frames(page);
            }
            await expect(input).toBeFocused();
            await page.keyboard.press("Control+a");
            await input.pressSequentially("27");
            await frames(page);

            await page.evaluate(() => { window.prompt = () => "formatted-cards.thinoda"; });
            const pending = page.waitForEvent("download", { timeout: 30_000 });
            await page.keyboard.press("Control+Shift+s");
            const download = await pending;
            expect(download.suggestedFilename()).toBe("formatted-cards.thinoda");
            const archive = await readFile(await download.path());
            const saved: {
                sheets: {
                    topics: Record<string, {
                        title: string;
                        card?: { kind: string };
                        style?: { bold?: boolean; font_size?: number };
                    }>;
                }[];
            } = JSON.parse(zipEntry(archive, "content.json").toString("utf8"));
            const cards = Object.values(saved.sheets[0].topics);
            expect(cards.find((card) => card.card?.kind === "sticky")).toMatchObject({
                title: "Browser draft continued",
                style: { bold: true },
            });
            expect(cards.find((card) => card.card?.kind === "text")).toMatchObject({
                title: "Replacement size",
                style: { bold: true, font_size: 27 },
            });
        });

        test("whole-card alignment moves a single-line title in exported SVG", async ({ page }) => {
            const url = process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/";
            const origin = new URL(url).origin;
            await page.route("**/*", (route) =>
                new URL(route.request().url()).origin === origin
                    ? route.continue()
                    : route.abort(),
            );
            await page.goto(url);
            await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
            const canvas = page.locator("#thinoda_canvas");
            await canvas.focus();
            await page.keyboard.press("Control+n");
            await frames(page);
            await command(page, "Hide central topic");
            await page.keyboard.press("Escape");
            await frames(page);
            await insertCard(page, "Sticky note", "Browser alignment");

            const positions: number[] = [];
            for (const [align, tabs] of [["left", 4], ["right", 6]] as const) {
                if (align === "right") {
                    await canvas.focus();
                    await page.keyboard.press("F2");
                    await frames(page);
                }
                await page.keyboard.press("Alt+F10");
                await frames(page);
                for (let index = 0; index < tabs; index++) {
                    await page.keyboard.press("Tab");
                    await frames(page);
                }
                await page.keyboard.press("Space");
                await frames(page);
                await page.keyboard.press("Alt+F10");
                await frames(page);
                await expect(page.locator("input[type='text']")).toBeFocused();
                await page.keyboard.press("Enter");
                await frames(page);

                const archive = await saveAs(page, `alignment-${align}.thinoda`);
                const saved: {
                    sheets: { topics: Record<string, { title: string; style?: { align?: string } }> }[];
                } = JSON.parse(zipEntry(archive, "content.json").toString("utf8"));
                expect(Object.values(saved.sheets[0].topics).find(
                    (topic) => topic.title === "Browser alignment",
                )?.style?.align).toBe(align);

                const name = `alignment-${align}.svg`;
                await page.evaluate((fileName) => { window.prompt = () => fileName; }, name);
                const pending = page.waitForEvent("download", { timeout: 30_000 });
                await command(page, "Export SVG");
                const download = await pending;
                expect(download.suggestedFilename()).toBe(name);
                const svg = (await readFile(await download.path())).toString("utf8");
                const text = svg.match(/<text\b[^>]*>Browser alignment<\/text>/)?.[0];
                expect(text).toBeDefined();
                const x = text?.match(/\bx="([^"]+)"/)?.[1];
                expect(x).toBeDefined();
                positions.push(Number(x));
            }
            expect(positions[1] - positions[0]).toBeGreaterThan(10);
        });
    });
}
