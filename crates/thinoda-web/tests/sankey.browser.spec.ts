/**
 * Canvas gallery smoke coverage plus inspection of downloaded editable state.
 * egui exposes one browser canvas, not individual gallery DOM controls.
 */
import { expect, test } from "@playwright/test";
import { colorBounds, frames, saveAs, zipEntry } from "./board-browser-helpers";

test.use({
    viewport: { width: 1280, height: 800 },
    deviceScaleFactor: 1,
    serviceWorkers: "block",
    screenshot: "only-on-failure",
});
test.setTimeout(180_000);

const examples = [
    { title: "Blank Sankey", mode: "Standard", weights: [] },
    { title: "Budget flow", mode: "Standard", weights: [55000, 25000, 20000] },
    { title: "Energy flow", mode: "Standard", weights: [65000, 35000, 40000, 25000] },
    { title: "Learning stages", mode: "Alluvial", weights: [60000, 40000, 25000, 35000, 15000, 25000] },
    { title: "Delivery scenarios", mode: "Probability", weights: [80000, 20000, 90000, 10000, 50000, 50000] },
    { title: "Recycling loop", mode: "Circular", weights: [60000, 20000, 60000, 20000] },
];

test("blank Sankey shows its weight scale legend even without flow links", async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", error => errors.push(error.message));
    await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    const canvas = page.locator("#thinoda_canvas");
    await frames(page);
    for (const position of [
        { x: 220, y: 128 }, // category filter
        { x: 220, y: 246 }, // Sankey diagrams
        { x: 115, y: 252 }, // Blank Sankey
        { x: 67, y: 722 }, // Create
    ]) {
        await canvas.click({ position });
        await frames(page);
    }

    const colors = await colorBounds(
        page,
        await canvas.screenshot(),
        [[47, 154, 208], [217, 76, 76]],
        8,
    );
    expect(colors.every(region => region.count > 0)).toBe(true);
    expect(errors).toEqual([]);
});

for (const [index, example] of examples.entries()) {
    test(`user can create, render and save the ${example.title} starter`, async ({ page }) => {
        const errors: string[] = [];
        page.on("pageerror", error => errors.push(error.message));
        await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
        await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
        const canvas = page.locator("#thinoda_canvas");
        await frames(page);
        for (const position of [
            { x: 220, y: 128 }, // category filter
            { x: 220, y: 246 }, // Sankey diagrams
            { x: 115 + index * 175, y: 252 }, // six original starters
            { x: 67, y: 722 }, // Create
        ]) {
            await canvas.click({ position });
            await frames(page);
        }

        const saved = await saveAs(page, `sankey-${index}.thinoda`);
        const manifest = JSON.parse(zipEntry(saved, "manifest.json").toString("utf8"));
        const sheet = JSON.parse(zipEntry(saved, "content.json").toString("utf8")).sheets[0];
        expect(manifest.version).toBe(25);
        expect(sheet.title).toBe(example.title);
        expect(sheet.sankey.mode).toBe(example.mode);
        expect(sheet.root_mode).toBe("hidden");
        expect((sheet.canvas?.objects ?? []).map(object => object.kind.connector.sankey.weight)).toEqual(example.weights);
        expect(sheet.floating).toHaveLength(Object.keys(sheet.topics).length - 1);
        if (example.mode === "Probability") {
            expect(Object.values(sheet.sankey.sources)).toEqual([100000]);
        }
        if (example.mode === "Alluvial") {
            const names = Object.values(sheet.topics).map((topic: { title: string }) => topic.title);
            expect(names.filter(name => name === "Exploring")).toHaveLength(2);
            expect(Object.keys(sheet.sankey.stages)).toHaveLength(5);
        }
        // All examples include a 20..80% ribbon on the common numeric color scale.
        // Pixel evidence supplements (not replaces) native-state assertions.
        const colors = [[80, 139, 183], [150, 107, 129], [141, 111, 135]];
        const regions = await colorBounds(page, await canvas.screenshot(),
            colors.map(color => color.map(value => Math.round(value * 0.75 + 255 * 0.25))), 25);
        if (example.weights.length) {
            expect(regions.reduce((sum, region) => sum + region.count, 0)).toBeGreaterThan(300);
        }
        expect(errors).toEqual([]);
    });
}

test("user can edit a weight with Enter, remove the flow and add an unset flow", async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", error => errors.push(error.message));
    await page.goto(process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/");
    await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
    const canvas = page.locator("#thinoda_canvas");
    // eframe web currently discards AccessKit updates, so individual controls
    // have no DOM accessible names. Named-control coverage lives in the native
    // inspector tests; this additionally exercises actual browser keyboard input.
    for (const position of [
        { x: 220, y: 128 }, { x: 220, y: 246 },
        { x: 290, y: 252 }, { x: 67, y: 722 },
        { x: 300, y: 340 }, // Available budget
        { x: 1096, y: 69 }, // Data
    ]) {
        await canvas.click({ position });
        await frames(page);
    }
    const savedSheet = async (name: string) => JSON.parse(
        zipEntry(await saveAs(page, name), "content.json").toString("utf8"),
    ).sheets[0];
    const before = await savedSheet("before.thinoda");
    await canvas.dblclick({ position: { x: 1030, y: 692 } });
    await page.keyboard.press("Control+a");
    await page.keyboard.insertText("40");
    await page.keyboard.press("Enter");
    await frames(page);
    const edited = await savedSheet("edited.thinoda");
    expect(edited.canvas.objects[0].kind.connector.sankey.weight).toBe(40000);
    // Enter must commit once, not also invoke the canvas's insert-topic shortcut.
    expect(edited.topics).toEqual(before.topics);
    expect(edited.floating).toEqual(before.floating);

    await page.mouse.move(1180, 700);
    await page.mouse.wheel(0, 240);
    await page.evaluate(() => new Promise<void>(resolve => {
        let left = 30;
        const frame = () => --left ? requestAnimationFrame(frame) : resolve();
        requestAnimationFrame(frame);
    }));
    await canvas.click({ position: { x: 1135, y: 538 } }); // Remove link
    await frames(page);
    const removed = await savedSheet("removed.thinoda");
    expect(removed.canvas.objects.map(object => object.kind.connector.sankey.weight))
        .toEqual([25000, 20000]);
    expect(removed.topics).toEqual(before.topics);

    await canvas.click({ position: { x: 730, y: 300 } }); // Essentials
    await frames(page);
    await canvas.click({ position: { x: 1060, y: 281 } }); // Parents: Choose topic
    await frames(page);
    await canvas.click({ position: { x: 1060, y: 314 } }); // Available budget
    await frames(page);
    await canvas.click({ position: { x: 1040, y: 313 } }); // Add link
    await frames(page);
    const added = await savedSheet("added.thinoda");
    expect(added.canvas.objects.map(object => object.kind.connector.sankey.weight))
        .toEqual([25000, 20000, null]);
    expect(added.topics).toEqual(before.topics);
    expect(errors).toEqual([]);
});
