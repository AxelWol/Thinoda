"use strict";

const assert = require("node:assert/strict");
const { createHash } = require("node:crypto");
const { inflateRawSync, inflateSync } = require("node:zlib");

const IMAGE_COUNT = 50;
const IMAGE_WIDTH = 256;
const IMAGE_HEIGHT = 192;
const IMAGE_MARKER = [17, 241, 141];
const MAX_ENTRY_BYTES = 64 * 1024 * 1024;
const crcTable = Uint32Array.from({ length: 256 }, (_, value) => {
    for (let bit = 0; bit < 8; bit++) {
        value = (value >>> 1) ^ ((value & 1) ? 0xedb88320 : 0);
    }
    return value >>> 0;
});

function crc32(bytes) {
    let crc = 0xffffffff;
    for (const byte of bytes) crc = (crc >>> 8) ^ crcTable[(crc ^ byte) & 0xff];
    return (crc ^ 0xffffffff) >>> 0;
}

function range(bytes, offset, length, label) {
    assert(Number.isSafeInteger(offset) && Number.isSafeInteger(length)
        && offset >= 0 && length >= 0 && offset + length <= bytes.length, `truncated ${label}`);
}

function readZip(zip) {
    assert(Buffer.isBuffer(zip), "archive must be a Buffer");
    const end = zip.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
    assert(end >= 0, "missing ZIP directory");
    range(zip, end, 22, "ZIP end record");
    assert.equal(end + 22 + zip.readUInt16LE(end + 20), zip.length, "invalid ZIP end record");
    assert.equal(zip.readUInt16LE(end + 4), 0, "multi-disk ZIP is unsupported");
    assert.equal(zip.readUInt16LE(end + 6), 0, "multi-disk ZIP is unsupported");
    const count = zip.readUInt16LE(end + 10);
    assert.equal(zip.readUInt16LE(end + 8), count, "inconsistent ZIP entry count");
    assert(count <= IMAGE_COUNT + 2, "too many ZIP entries");
    const directorySize = zip.readUInt32LE(end + 12);
    let offset = zip.readUInt32LE(end + 16);
    assert.equal(offset + directorySize, end, "invalid ZIP directory bounds");
    const directoryStart = offset;
    const entries = new Map();
    for (let i = 0; i < count; i++) {
        range(zip, offset, 46, "ZIP directory entry");
        assert.equal(zip.readUInt32LE(offset), 0x02014b50, "invalid ZIP directory entry");
        const flags = zip.readUInt16LE(offset + 8);
        const method = zip.readUInt16LE(offset + 10);
        const expectedCrc = zip.readUInt32LE(offset + 16);
        const compressedSize = zip.readUInt32LE(offset + 20);
        const size = zip.readUInt32LE(offset + 24);
        const nameLength = zip.readUInt16LE(offset + 28);
        const extraLength = zip.readUInt16LE(offset + 30);
        const commentLength = zip.readUInt16LE(offset + 32);
        const localOffset = zip.readUInt32LE(offset + 42);
        range(zip, offset + 46, nameLength + extraLength + commentLength, "ZIP directory name");
        assert.equal(flags & 1, 0, "encrypted ZIP entry");
        assert(size <= MAX_ENTRY_BYTES, "ZIP entry exceeds validation limit");
        const name = zip.toString("utf8", offset + 46, offset + 46 + nameLength);
        assert(!entries.has(name), `duplicate ZIP entry ${name}`);

        range(zip, localOffset, 30, "ZIP local entry");
        assert.equal(zip.readUInt32LE(localOffset), 0x04034b50, "invalid ZIP local entry");
        assert.equal(zip.readUInt16LE(localOffset + 6), flags, "ZIP flags differ");
        assert.equal(zip.readUInt16LE(localOffset + 8), method, "ZIP methods differ");
        const localNameLength = zip.readUInt16LE(localOffset + 26);
        const localExtraLength = zip.readUInt16LE(localOffset + 28);
        range(zip, localOffset + 30, localNameLength + localExtraLength, "ZIP local name");
        assert.equal(
            zip.toString("utf8", localOffset + 30, localOffset + 30 + localNameLength),
            name,
            "ZIP entry names differ",
        );
        const start = localOffset + 30 + localNameLength + localExtraLength;
        range(zip, start, compressedSize, "ZIP entry data");
        assert(start + compressedSize <= directoryStart, "ZIP entry overlaps directory");
        const compressed = zip.subarray(start, start + compressedSize);
        let data;
        if (method === 0) data = Buffer.from(compressed);
        else if (method === 8) data = inflateRawSync(compressed, { maxOutputLength: MAX_ENTRY_BYTES });
        else throw new Error(`Unsupported ZIP compression method ${method}`);
        assert.equal(data.length, size, `ZIP entry size mismatch: ${name}`);
        assert.equal(crc32(data), expectedCrc, `ZIP entry CRC mismatch: ${name}`);
        entries.set(name, data);
        offset += 46 + nameLength + extraLength + commentLength;
    }
    assert.equal(offset, end, "ZIP directory count does not match entries");
    return entries;
}

function required(entries, name) {
    const data = entries.get(name);
    assert(data, `Missing ZIP entry ${name}`);
    return data;
}

function sha256(bytes) {
    return createHash("sha256").update(bytes).digest("hex");
}

function validatePng(bytes) {
    assert(bytes.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10])),
        "invalid PNG signature");
    let offset = 8;
    const chunks = [];
    while (offset < bytes.length) {
        range(bytes, offset, 12, "PNG chunk");
        const size = bytes.readUInt32BE(offset);
        range(bytes, offset + 8, size + 4, "PNG chunk data");
        const type = bytes.toString("ascii", offset + 4, offset + 8);
        const data = bytes.subarray(offset + 8, offset + 8 + size);
        assert.equal(crc32(bytes.subarray(offset + 4, offset + 8 + size)),
            bytes.readUInt32BE(offset + 8 + size), `PNG ${type} CRC mismatch`);
        chunks.push({ type, data });
        offset += 12 + size;
    }
    assert.deepEqual(chunks.map(chunk => chunk.type), ["IHDR", "IDAT", "IEND"], "invalid PNG chunks");
    const header = chunks[0].data;
    assert.equal(header.length, 13, "invalid PNG header");
    assert.equal(header.readUInt32BE(0), IMAGE_WIDTH, "unexpected PNG width");
    assert.equal(header.readUInt32BE(4), IMAGE_HEIGHT, "unexpected PNG height");
    assert.deepEqual([...header.subarray(8)], [8, 2, 0, 0, 0], "unexpected PNG format");
    assert.equal(chunks[2].data.length, 0, "invalid PNG end");
    const rowLength = 1 + IMAGE_WIDTH * 3;
    const expectedLength = rowLength * IMAGE_HEIGHT;
    const pixels = inflateSync(chunks[1].data, { maxOutputLength: expectedLength });
    assert.equal(pixels.length, expectedLength, "invalid PNG pixel data");
    for (let y = 0; y < IMAGE_HEIGHT; y++) {
        assert.equal(pixels[y * rowLength], 0, "unexpected PNG row filter");
        if (y < IMAGE_HEIGHT / 2) {
            for (let x = 0; x < IMAGE_WIDTH / 2; x++) {
                const pixel = y * rowLength + 1 + x * 3;
                assert(pixels[pixel] === IMAGE_MARKER[0]
                    && pixels[pixel + 1] === IMAGE_MARKER[1]
                    && pixels[pixel + 2] === IMAGE_MARKER[2], "missing PNG marker");
            }
        }
    }
}

function canonicalColor(value) {
    if (value == null) return null;
    assert.equal(typeof value, "string", "invalid stroke color");
    let hex = value.trim().replace(/^#+/, "");
    assert.match(hex, /^(?:[0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i, "invalid stroke color");
    if (hex.length === 3) hex = [...hex].map(nibble => nibble + nibble).join("");
    if (hex.length === 6) hex += "ff";
    return hex.toUpperCase();
}

function strokeFingerprint(object) {
    const ink = object.kind.ink;
    const style = object.style === undefined ? {} : object.style;
    assert(style && typeof style === "object" && !Array.isArray(style), "invalid object style");
    const tool = ink.tool === undefined ? "pen" : ink.tool;
    const band = object.band === undefined ? "above" : object.band;
    assert(["pen", "highlighter"].includes(tool), "invalid ink tool");
    assert(ink.color != null, "missing ink color");
    assert(["above", "below"].includes(band), "invalid object band");
    const pressure = ink.pressure ?? null;
    assert(pressure === null || (Array.isArray(pressure) && pressure.length === ink.points.length),
        "invalid stroke pressure");
    const locked = object.locked === undefined ? false : object.locked;
    const altText = object.alt_text === undefined ? "" : object.alt_text;
    assert.equal(typeof locked, "boolean", "invalid stroke lock");
    assert.equal(typeof altText, "string", "invalid stroke description");

    const values = [
        ink.width,
        object.rect.min.x, object.rect.min.y,
        object.rect.size.x, object.rect.size.y,
        object.rotation === undefined ? 0 : object.rotation,
        style.stroke_width === undefined ? 2 : style.stroke_width,
        style.opacity === undefined ? 1 : style.opacity,
    ];
    for (const point of ink.points) values.push(point.x, point.y);
    if (pressure !== null) values.push(...pressure);
    const geometry = Buffer.allocUnsafe(values.length * 4);
    values.forEach((value, index) => {
        assert(typeof value === "number" && Number.isFinite(value) && Number.isFinite(Math.fround(value)),
            "invalid stroke geometry or style number");
        geometry.writeFloatLE(Math.fround(value), index * 4);
    });
    const metadata = JSON.stringify({
        tool,
        color: canonicalColor(ink.color),
        hasPressure: pressure !== null,
        band,
        group: object.group ?? null,
        locked,
        altText,
        fill: canonicalColor(style.fill),
        stroke: canonicalColor(style.stroke),
        textColor: canonicalColor(style.text_color),
    });
    // Rust stores geometry and widths as f32; JSON can spell the same f32 differently.
    return createHash("sha256").update(metadata).update("\0").update(geometry).digest("hex");
}

function validateBoard(zip, { strokes, embeddedImages }) {
    const entries = readZip(zip);
    const manifest = JSON.parse(required(entries, "manifest.json").toString("utf8"));
    const workbook = JSON.parse(required(entries, "content.json").toString("utf8"));
    assert.equal(manifest.format, "thinoda");
    assert.equal(manifest.version, 6);
    assert(Array.isArray(manifest.resources), "missing resource manifest");
    assert.equal(workbook.sheets.length, 1, "fixture must have one sheet");
    const sheet = workbook.sheets[0];
    assert.equal(sheet.title, `Performance ${strokes}`);
    assert.equal(sheet.root_mode, "hidden");
    assert.equal(Object.keys(sheet.resources?.entries ?? {}).length, 0, "inline resources are not native archive entries");
    const objects = sheet.canvas.objects;
    assert.equal(objects.length, strokes, "incorrect stroke count");
    assert(objects.every(object => object.kind?.ink?.points?.length === 64), "invalid ink stroke");
    const strokeIds = objects.map(object => object.id);
    assert(strokeIds.every(id => Number.isSafeInteger(id) && id > 0), "invalid ink ID");
    assert.equal(new Set(strokeIds).size, strokes, "duplicate ink IDs");
    const strokeStates = new Map(objects.map(object => [object.id, strokeFingerprint(object)]));

    const cards = Object.values(sheet.topics).filter(topic => topic.card?.kind === "image");
    assert.equal(cards.length, IMAGE_COUNT, "incorrect image-card count");
    assert.equal(Object.keys(sheet.topics).length, IMAGE_COUNT + 1, "unexpected extra topics");
    assert.equal(sheet.topics[String(sheet.root)]?.id, sheet.root, "missing root topic");
    const cardIds = cards.map(card => card.id);
    assert.equal(new Set(cardIds).size, IMAGE_COUNT, "duplicate image-card IDs");
    assert(!cardIds.includes(sheet.root), "root cannot be an image card");
    assert.deepEqual([...sheet.floating].sort((a, b) => a - b),
        [...cardIds].sort((a, b) => a - b), "image cards must be floating");

    const resources = new Map();
    const names = new Set();
    for (const resource of manifest.resources) {
        assert.equal(resource.sheet, 0, "resource belongs to another sheet");
        assert.match(resource.id, /^[0-9a-f]{64}$/, "invalid resource ID");
        assert(!resources.has(resource.id), "duplicate resource manifest entry");
        assert.equal(resource.media_type, "image/png", "resource must be a PNG");
        assert.match(resource.name, /^synthetic-\d{2}\.png$/, "unexpected resource name");
        assert(!names.has(resource.name), "duplicate resource name");
        names.add(resource.name);
        const bytes = required(entries, `resources/${resource.id}`);
        assert.equal(resource.size, bytes.length, "resource size mismatch");
        assert.equal(sha256(bytes), resource.id, "resource digest mismatch");
        validatePng(bytes);
        resources.set(resource.id, { ...resource, bytes });
    }

    const references = new Map(cards.map(card => [card.id, card.image ?? null]));
    const cardStates = new Map(cards.map(card => [card.id, {
        title: card.title,
        position: card.position,
        cardSize: card.card.size,
        imageSize: card.image_size ?? null,
    }]));
    if (embeddedImages) {
        assert.equal(resources.size, IMAGE_COUNT, "expected 50 embedded image resources");
        const ids = [...references.values()].map(reference => {
            assert.match(reference, /^thinoda-resource:[0-9a-f]{64}$/, "missing embedded image reference");
            return reference.slice("thinoda-resource:".length);
        });
        assert.equal(new Set(ids).size, IMAGE_COUNT, "image cards must reference distinct resources");
        assert.deepEqual([...ids].sort(), [...resources.keys()].sort(), "image resource references differ");
    } else {
        assert.equal(resources.size, 0, "placeholder fixture contains resources");
        assert([...references.values()].every(reference => reference === null), "placeholder has an image reference");
    }
    assert.equal(entries.size, resources.size + 2, "unexpected or unlisted ZIP entries");
    return {
        strokes,
        embeddedImages,
        archiveSha256: sha256(zip),
        archiveBytes: zip.length,
        imageCards: cards.length,
        embeddedResources: resources.size,
        encodedImageBytes: [...resources.values()].reduce((sum, resource) => sum + resource.bytes.length, 0),
        nominalRgbaBytes: resources.size * IMAGE_WIDTH * IMAGE_HEIGHT * 4,
        strokeIds,
        strokeStates,
        references,
        cardStates,
        resources,
    };
}

function assertRoundTrip(expected, actual) {
    assert.equal(actual.strokes, expected.strokes);
    assert.equal(actual.embeddedImages, expected.embeddedImages);
    assert.deepEqual(actual.strokeIds, expected.strokeIds, "stroke order or IDs changed during save");
    for (const [id, fingerprint] of expected.strokeStates) {
        assert.equal(actual.strokeStates.get(id), fingerprint, `stroke ${id} geometry or style changed during save`);
    }
    assert.deepEqual([...actual.references].sort((a, b) => a[0] - b[0]),
        [...expected.references].sort((a, b) => a[0] - b[0]), "image-card references changed during save");
    assert.deepEqual([...actual.cardStates].sort((a, b) => a[0] - b[0]),
        [...expected.cardStates].sort((a, b) => a[0] - b[0]), "image-card layout changed during save");
    assert.deepEqual([...actual.resources.keys()].sort(), [...expected.resources.keys()].sort(),
        "resource IDs changed during save");
    for (const [id, resource] of expected.resources) {
        assert.deepEqual(actual.resources.get(id), resource, `resource changed during save: ${id}`);
    }
}

module.exports = {
    IMAGE_COUNT,
    IMAGE_HEIGHT,
    IMAGE_MARKER,
    IMAGE_WIDTH,
    assertRoundTrip,
    crc32,
    readZip,
    validateBoard,
};
