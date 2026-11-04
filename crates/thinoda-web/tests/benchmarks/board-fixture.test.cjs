"use strict";

const assert = require("node:assert/strict");
const { execFileSync } = require("node:child_process");
const { createHash } = require("node:crypto");
const { resolve } = require("node:path");
const { before, test } = require("node:test");
const { assertRoundTrip, crc32, readZip, validateBoard } = require("./board-fixture.cjs");

const generator = resolve(__dirname, "make-board-fixtures.py");
const source = resolve(__dirname, "..", "fixtures", "affinity-diagram.thinoda");
const python = process.env.PYTHON || (process.platform === "win32" ? "python" : "python3");
const code = [
    "import importlib.util,pathlib,sys",
    "s=importlib.util.spec_from_file_location('board_fixture',sys.argv[1])",
    "m=importlib.util.module_from_spec(s)",
    "s.loader.exec_module(m)",
    "sys.stdout.buffer.write(m.build_archive(pathlib.Path(sys.argv[2]),3,sys.argv[3]=='1')[0])",
].join(";");

function generate(embeddedImages) {
    return execFileSync(python, ["-B", "-c", code, generator, source, embeddedImages ? "1" : "0"], {
        maxBuffer: 8 * 1024 * 1024,
    });
}

function storedZip(entries) {
    const locals = [];
    const directory = [];
    let offset = 0;
    for (const [name, data] of entries) {
        const filename = Buffer.from(name);
        const checksum = crc32(data);
        const local = Buffer.alloc(30);
        local.writeUInt32LE(0x04034b50, 0);
        local.writeUInt16LE(20, 4);
        local.writeUInt32LE(checksum, 14);
        local.writeUInt32LE(data.length, 18);
        local.writeUInt32LE(data.length, 22);
        local.writeUInt16LE(filename.length, 26);
        locals.push(local, filename, data);

        const central = Buffer.alloc(46);
        central.writeUInt32LE(0x02014b50, 0);
        central.writeUInt16LE(20, 4);
        central.writeUInt16LE(20, 6);
        central.writeUInt32LE(checksum, 16);
        central.writeUInt32LE(data.length, 20);
        central.writeUInt32LE(data.length, 24);
        central.writeUInt16LE(filename.length, 28);
        central.writeUInt32LE(offset, 42);
        directory.push(central, filename);
        offset += local.length + filename.length + data.length;
    }
    const directorySize = directory.reduce((sum, part) => sum + part.length, 0);
    const end = Buffer.alloc(22);
    end.writeUInt32LE(0x06054b50, 0);
    end.writeUInt16LE(entries.length, 8);
    end.writeUInt16LE(entries.length, 10);
    end.writeUInt32LE(directorySize, 12);
    end.writeUInt32LE(offset, 16);
    return Buffer.concat([...locals, ...directory, end]);
}

function replaceJson(entries, name, json) {
    entries.set(name, Buffer.from(JSON.stringify(json)));
}

function replaceResource(entries, mutate) {
    const manifest = JSON.parse(entries.get("manifest.json"));
    const content = JSON.parse(entries.get("content.json"));
    const resource = manifest.resources[0];
    const oldId = resource.id;
    const data = mutate(Buffer.from(entries.get(`resources/${oldId}`)));
    resource.id = createHash("sha256").update(data).digest("hex");
    resource.size = data.length;
    for (const topic of Object.values(content.sheets[0].topics)) {
        if (topic.image === `thinoda-resource:${oldId}`) topic.image = `thinoda-resource:${resource.id}`;
    }
    entries.delete(`resources/${oldId}`);
    entries.set(`resources/${resource.id}`, data);
    replaceJson(entries, "manifest.json", manifest);
    replaceJson(entries, "content.json", content);
}

function replaceStroke(mutate) {
    const entries = readZip(embedded);
    const content = JSON.parse(entries.get("content.json"));
    mutate(content.sheets[0].canvas.objects);
    replaceJson(entries, "content.json", content);
    return storedZip([...entries]);
}

function nextFloat32(value) {
    const bytes = Buffer.alloc(4);
    bytes.writeFloatLE(value);
    assert(bytes.readFloatLE() > 0);
    bytes.writeUInt32LE(bytes.readUInt32LE() + 1);
    return bytes.readFloatLE();
}

let placeholders;
let embedded;
before(() => {
    placeholders = generate(false);
    embedded = generate(true);
});

test("placeholder and embedded fixtures are deterministic and distinct", () => {
    assert.deepEqual(generate(false), placeholders);
    assert.deepEqual(generate(true), embedded);
    const empty = validateBoard(placeholders, { strokes: 3, embeddedImages: false });
    const images = validateBoard(embedded, { strokes: 3, embeddedImages: true });
    assert.equal(empty.imageCards, 50);
    assert.equal(empty.embeddedResources, 0);
    assert.equal(images.imageCards, 50);
    assert.equal(images.embeddedResources, 50);
    assert(images.encodedImageBytes > 0);
    assert.equal(images.nominalRgbaBytes, 50 * 256 * 192 * 4);
    assertRoundTrip(images, validateBoard(embedded, { strokes: 3, embeddedImages: true }));
});

test("embedded ZIP timestamps and host metadata are fixed", () => {
    const end = embedded.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
    let offset = embedded.readUInt32LE(end + 16);
    for (let index = 0; index < embedded.readUInt16LE(end + 10); index++) {
        assert.equal(embedded[offset + 5], 0, "ZIP host metadata must not depend on the OS");
        assert.equal(embedded.readUInt16LE(offset + 12), 0);
        assert.equal(embedded.readUInt16LE(offset + 14), 0x21);
        offset += 46 + embedded.readUInt16LE(offset + 28)
            + embedded.readUInt16LE(offset + 30) + embedded.readUInt16LE(offset + 32);
    }
});

test("missing resources, changed bytes and incorrect sizes are rejected", () => {
    const missing = readZip(embedded);
    const resource = JSON.parse(missing.get("manifest.json")).resources[0];
    missing.delete(`resources/${resource.id}`);
    assert.throws(() => validateBoard(storedZip([...missing]), { strokes: 3, embeddedImages: true }),
        /Missing ZIP entry resources/);

    const tampered = readZip(embedded);
    const bytes = Buffer.from(tampered.get(`resources/${resource.id}`));
    bytes[41] ^= 1;
    tampered.set(`resources/${resource.id}`, bytes);
    assert.throws(() => validateBoard(storedZip([...tampered]), { strokes: 3, embeddedImages: true }),
        /resource digest mismatch/);

    const wrongSize = readZip(embedded);
    const manifest = JSON.parse(wrongSize.get("manifest.json"));
    manifest.resources[0].size++;
    replaceJson(wrongSize, "manifest.json", manifest);
    assert.throws(() => validateBoard(storedZip([...wrongSize]), { strokes: 3, embeddedImages: true }),
        /resource size mismatch/);
});

test("invalid image data is rejected even when its resource digest matches", () => {
    const invalid = readZip(embedded);
    replaceResource(invalid, () => Buffer.from("not a PNG"));
    assert.throws(() => validateBoard(storedZip([...invalid]), { strokes: 3, embeddedImages: true }),
        /invalid PNG signature/);

    const damaged = readZip(embedded);
    replaceResource(damaged, bytes => {
        bytes[41] ^= 1;
        return bytes;
    });
    assert.throws(() => validateBoard(storedZip([...damaged]), { strokes: 3, embeddedImages: true }),
        /PNG IDAT CRC mismatch/);
});

test("missing references, duplicate ZIP entries and truncated archives are rejected", () => {
    const missingReference = readZip(embedded);
    const content = JSON.parse(missingReference.get("content.json"));
    const card = Object.values(content.sheets[0].topics).find(topic => topic.card?.kind === "image");
    card.image = `thinoda-resource:${"f".repeat(64)}`;
    replaceJson(missingReference, "content.json", content);
    assert.throws(() => validateBoard(storedZip([...missingReference]), { strokes: 3, embeddedImages: true }),
        /image resource references differ/);

    const duplicate = [...readZip(placeholders)];
    duplicate.push(duplicate[0]);
    assert.throws(() => validateBoard(storedZip(duplicate), { strokes: 3, embeddedImages: false }),
        /duplicate ZIP entry/);
    assert.throws(() => readZip(embedded.subarray(0, embedded.length - 4)), /ZIP end record/);
});

test("incomplete boards and placeholder substitutions are rejected", () => {
    const lostStroke = readZip(embedded);
    const strokeContent = JSON.parse(lostStroke.get("content.json"));
    strokeContent.sheets[0].canvas.objects.pop();
    replaceJson(lostStroke, "content.json", strokeContent);
    assert.throws(() => validateBoard(storedZip([...lostStroke]), { strokes: 3, embeddedImages: true }),
        /incorrect stroke count/);

    const lostCard = readZip(embedded);
    const cardContent = JSON.parse(lostCard.get("content.json"));
    const cardId = Object.keys(cardContent.sheets[0].topics)
        .find(id => cardContent.sheets[0].topics[id].card?.kind === "image");
    delete cardContent.sheets[0].topics[cardId];
    replaceJson(lostCard, "content.json", cardContent);
    assert.throws(() => validateBoard(storedZip([...lostCard]), { strokes: 3, embeddedImages: true }),
        /incorrect image-card count/);
    assert.throws(() => validateBoard(placeholders, { strokes: 3, embeddedImages: true }),
        /expected 50 embedded image resources/);
});

test("round-trip checks cover every stroke point, frame and style at f32 precision", () => {
    const beforeSave = validateBoard(embedded, { strokes: 3, embeddedImages: true });
    const equivalent = replaceStroke(objects => {
        const stroke = objects[0];
        const y = stroke.kind.ink.points[1].y;
        assert.notEqual(y, Math.fround(y));
        stroke.kind.ink.points[1].y = Math.fround(y);
        stroke.kind.ink.color = "2864dcff";
        stroke.style = { stroke_width: 2, opacity: 1 };
        delete stroke.kind.ink.tool;
    });
    assertRoundTrip(beforeSave, validateBoard(equivalent, { strokes: 3, embeddedImages: true }));

    const changes = [
        ["point", stroke => { stroke.kind.ink.points[1].y = nextFloat32(stroke.kind.ink.points[1].y); }],
        ["last point", stroke => { stroke.kind.ink.points[63].x = nextFloat32(stroke.kind.ink.points[63].x); }],
        ["frame", stroke => { stroke.rect.size.x = nextFloat32(stroke.rect.size.x); }],
        ["width", stroke => { stroke.kind.ink.width = nextFloat32(stroke.kind.ink.width); }],
        ["color", stroke => { stroke.kind.ink.color = "#2864DD"; }],
        ["tool", stroke => { stroke.kind.ink.tool = "highlighter"; }],
        ["opacity", stroke => { stroke.style = { opacity: 0.5 }; }],
        ["object stroke width", stroke => { stroke.style = { stroke_width: nextFloat32(2) }; }],
        ["fill color", stroke => { stroke.style = { fill: "#112233" }; }],
        ["object color", stroke => { stroke.style = { stroke: "#AABBCC" }; }],
        ["text color", stroke => { stroke.style = { text_color: "#445566" }; }],
        ["rotation", stroke => { stroke.rotation = 2; }],
        ["band", stroke => { stroke.band = "below"; }],
        ["group", stroke => { stroke.group = 99; }],
        ["lock", stroke => { stroke.locked = true; }],
        ["description", stroke => { stroke.alt_text = "Changed"; }],
        ["pressure", stroke => { stroke.kind.ink.pressure = Array(64).fill(1); }],
    ];
    for (const [label, change] of changes) {
        const changed = replaceStroke(objects => change(objects[0]));
        const afterSave = validateBoard(changed, { strokes: 3, embeddedImages: true });
        assert.throws(() => assertRoundTrip(beforeSave, afterSave),
            /stroke 52 geometry or style changed during save/, label);
    }

    const withPressure = replaceStroke(objects => { objects[0].kind.ink.pressure = Array(64).fill(1); });
    const pressureBeforeSave = validateBoard(withPressure, { strokes: 3, embeddedImages: true });
    const changedPressure = readZip(withPressure);
    const pressureContent = JSON.parse(changedPressure.get("content.json"));
    pressureContent.sheets[0].canvas.objects[0].kind.ink.pressure[63] = nextFloat32(1);
    replaceJson(changedPressure, "content.json", pressureContent);
    const pressureAfterSave = validateBoard(storedZip([...changedPressure]), { strokes: 3, embeddedImages: true });
    assert.throws(() => assertRoundTrip(pressureBeforeSave, pressureAfterSave),
        /stroke 52 geometry or style changed during save/);

    const reordered = replaceStroke(objects => { [objects[0], objects[1]] = [objects[1], objects[0]]; });
    assert.throws(() => assertRoundTrip(beforeSave, validateBoard(reordered, { strokes: 3, embeddedImages: true })),
        /stroke order or IDs changed during save/);
});

test("round-trip checks detect changed metadata even when resources remain valid", () => {
    const changed = readZip(embedded);
    const manifest = JSON.parse(changed.get("manifest.json"));
    [manifest.resources[0].name, manifest.resources[1].name] =
        [manifest.resources[1].name, manifest.resources[0].name];
    replaceJson(changed, "manifest.json", manifest);
    const beforeSave = validateBoard(embedded, { strokes: 3, embeddedImages: true });
    const afterSave = validateBoard(storedZip([...changed]), { strokes: 3, embeddedImages: true });
    assert.throws(() => assertRoundTrip(beforeSave, afterSave), /resource changed during save/);

    const changedCard = readZip(embedded);
    const content = JSON.parse(changedCard.get("content.json"));
    const card = Object.values(content.sheets[0].topics).find(topic => topic.card?.kind === "image");
    card.image_size.x++;
    replaceJson(changedCard, "content.json", content);
    const resized = validateBoard(storedZip([...changedCard]), { strokes: 3, embeddedImages: true });
    assert.throws(() => assertRoundTrip(beforeSave, resized), /image-card layout changed during save/);
});
