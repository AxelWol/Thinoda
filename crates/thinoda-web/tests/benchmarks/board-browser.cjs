"use strict";

const assert = require("node:assert/strict");
const { IMAGE_HEIGHT, IMAGE_MARKER, IMAGE_WIDTH } = require("./board-fixture.cjs");

async function decodeImages(page, board) {
    const images = [...board.resources.values()].map(resource => resource.bytes.toString("base64"));
    const dimensions = await page.evaluate(async images => {
        const sizes = [];
        for (const encoded of images) {
            const binary = atob(encoded);
            const bytes = Uint8Array.from(binary, char => char.charCodeAt(0));
            const bitmap = await createImageBitmap(new Blob([bytes], { type: "image/png" }));
            sizes.push([bitmap.width, bitmap.height]);
            bitmap.close();
        }
        return sizes;
    }, images);
    assert.equal(dimensions.length, board.embeddedResources);
    assert(dimensions.every(([width, height]) => width === IMAGE_WIDTH && height === IMAGE_HEIGHT),
        "browser decoded an image at unexpected dimensions");
    return dimensions.length;
}

async function markerPixels(page, screenshot) {
    return page.evaluate(async ({ screenshot, marker }) => {
        const image = new Image();
        image.src = `data:image/png;base64,${screenshot}`;
        await image.decode();
        const canvas = document.createElement("canvas");
        canvas.width = image.naturalWidth;
        canvas.height = image.naturalHeight;
        const context = canvas.getContext("2d");
        if (!context) throw new Error("Cannot inspect the benchmark screenshot");
        context.drawImage(image, 0, 0);
        const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
        let count = 0;
        for (let index = 0; index < pixels.length; index += 4) {
            if (marker.every((value, channel) => Math.abs(pixels[index + channel] - value) < 25)) count++;
        }
        return count;
    }, { screenshot: screenshot.toString("base64"), marker: IMAGE_MARKER });
}

async function waitForMarker(page) {
    const deadline = Date.now() + 60_000;
    do {
        const count = await markerPixels(page, await page.screenshot());
        if (count >= 30) return count;
        await page.waitForTimeout(500);
    } while (Date.now() < deadline);
    throw new Error("embedded image marker did not become visible on the board");
}

module.exports = { decodeImages, markerPixels, waitForMarker };
