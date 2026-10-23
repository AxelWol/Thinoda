const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const root = path.resolve(__dirname, "..");

test("Node scripts use the repository's Node.js 24 runtime", () => {
    const major = fs.readFileSync(path.resolve(root, "..", "..", ".node-version"), "utf8").trim();
    assert.equal(major, "24");
    assert.equal(process.versions.node.split(".")[0], major);
});

test("mobile viewport metadata leaves browser zoom available", () => {
    const html = fs.readFileSync(path.join(root, "index.html"), "utf8");
    const viewport = html.match(/<meta\s+name=["']viewport["']\s+content=["']([^"']+)["']/i);
    assert.ok(viewport, "viewport metadata is present");
    const directives = new Map(viewport[1].split(",").map((directive) => {
        const [name, value = ""] = directive.trim().split("=");
        return [name.toLowerCase(), value.trim().toLowerCase()];
    }));
    assert.equal(directives.get("width"), "device-width");
    assert.equal(directives.get("initial-scale"), "1");
    assert.equal(directives.has("maximum-scale"), false);
    assert.equal(directives.has("user-scalable"), false);
});

function installFixture({ standalone = false, register } = {}) {
    const listeners = {};
    const errors = [];
    const elements = [];
    const window = {
        navigator: {},
        matchMedia: () => ({ matches: standalone }),
        addEventListener: (name, callback) => { listeners[name] = callback; },
        isSecureContext: true,
    };
    if (register) window.navigator.serviceWorker = { register };
    const document = {
        createElement(tag) {
            const element = {
                tag, children: [], listeners: {}, disabled: false,
                append(...items) { this.children.push(...items); },
                setAttribute() {},
                addEventListener(name, callback) { this.listeners[name] = callback; },
                showModal() { this.open = true; },
                close() { this.open = false; },
                remove() {},
                focus() { this.focused = true; },
            };
            elements.push(element);
            return element;
        },
        body: { append() {} },
    };
    vm.runInNewContext(fs.readFileSync(path.join(root, "pwa.js"), "utf8"), {
        window, document, navigator: window.navigator,
        console: { error: (...args) => errors.push(args) },
    });
    return {
        window, listeners, elements, errors,
        open() { window.thinodaPwa.open("Install", "Close", "Help", "Storage warning", "Unavailable"); },
        button() { return elements.find(element => element.tag === "button"); },
        status() { return elements.filter(element => element.tag === "p").at(-1); },
    };
}

test("install is progressive, one-shot and never runs before a button click", async () => {
    const fixture = installFixture();
    fixture.open();
    assert.equal(fixture.button().disabled, true);
    let prompts = 0;
    let prevented = false;
    fixture.listeners.beforeinstallprompt({
        preventDefault() { prevented = true; },
        async prompt() { prompts++; },
        userChoice: Promise.resolve({ outcome: "dismissed" }),
    });
    assert.equal(prevented, true);
    assert.equal(prompts, 0);
    assert.equal(fixture.button().disabled, false);
    await fixture.button().listeners.click();
    assert.equal(prompts, 1);
    assert.equal(fixture.button().disabled, true);
    await fixture.button().listeners.click();
    assert.equal(prompts, 1);
    assert.equal(fixture.status().textContent, "Unavailable");
});

test("registration and asynchronous cache-install failures are visible", async () => {
    const fixture = installFixture({ register: async () => { throw new Error("No quota"); } });
    fixture.open();
    fixture.listeners.load();
    await new Promise(resolve => setImmediate(resolve));
    assert.match(fixture.status().textContent, /No quota/);
    let stateChanged;
    const worker = { state: "installing", addEventListener(name, callback) { stateChanged = callback; } };
    const failed = installFixture({
        register: async () => ({ installing: worker, addEventListener() {} }),
    });
    failed.open();
    failed.listeners.load();
    await new Promise(resolve => setImmediate(resolve));
    worker.state = "redundant";
    stateChanged();
    assert.match(failed.status().textContent, /Offline shell installation failed/);
});

test("standalone, appinstalled, unavailable browsers and prompt errors are explicit", async () => {
    const installed = installFixture({ standalone: true });
    installed.open();
    assert.equal(installed.button().disabled, true);
    const fixture = installFixture();
    fixture.listeners.beforeinstallprompt({
        preventDefault() {},
        async prompt() { throw new Error("Denied"); },
    });
    fixture.open();
    await fixture.button().listeners.click();
    assert.match(fixture.status().textContent, /Denied/);
    assert.equal(fixture.errors.length, 1);
    fixture.listeners.appinstalled();
    assert.equal(fixture.button().disabled, true);
    assert.equal(fixture.status().textContent, "Unavailable");
});

test("manifest and Apple icons resolve within a subpath with actual PNG sizes", () => {
    const manifest = JSON.parse(fs.readFileSync(path.join(root, "manifest.webmanifest")));
    const base = new URL("https://example.org/Thinoda/manifest.webmanifest");
    assert.equal(new URL(manifest.start_url, base).href, "https://example.org/Thinoda/");
    assert.equal(manifest.scope, "./");
    assert.equal(manifest.id, "./");
    assert.equal(manifest.display, "standalone");
    for (const icon of manifest.icons) {
        const png = fs.readFileSync(path.join(root, "../../assets", icon.src));
        assert.equal(`${png.readUInt32BE(16)}x${png.readUInt32BE(20)}`, icon.sizes);
        assert.equal(new URL(icon.src, base).origin, base.origin);
    }
    for (const size of ["192x192", "512x512"]) {
        assert.ok(manifest.icons.some(icon => icon.sizes === size && icon.purpose === "any"));
    }
    assert.ok(manifest.icons.some(icon => icon.purpose === "maskable"));
    const apple = fs.readFileSync(path.join(root, "../../assets/pwa-icons/apple-touch-icon.png"));
    assert.equal(apple.readUInt32BE(16), 180);
    assert.equal(apple.readUInt32BE(20), 180);
    for (const size of [152, 167]) {
        const icon = fs.readFileSync(path.join(root, `../../assets/pwa-icons/apple-touch-icon-${size}.png`));
        assert.equal(icon.readUInt32BE(16), size);
        assert.equal(icon.readUInt32BE(20), size);
    }
});

test("offline worker caches only the versioned shell and respects scope and updates", async () => {
    const listeners = {};
    const scope = "https://example.org/Thinoda/";
    const prefix = `thinoda-shell:${scope}:`;
    const deleted = [];
    let cachedRequests = [];
    const matches = [];
    const source = fs.readFileSync(path.join(root, "sw.js"), "utf8")
        .replace("__THINODA_FILES__", '["./index.html","./app.wasm","./app.js"]')
        .replace("__THINODA_REVISION__", "new");
    vm.runInNewContext(source, {
        self: {
            registration: { scope },
            addEventListener: (name, callback) => { listeners[name] = callback; },
        },
        URL, Request,
        caches: {
            async open(name) {
                assert.equal(name, `${prefix}new`);
                return {
                    async addAll(requests) { cachedRequests = requests; },
                    async match(target) { matches.push(target); return "cached shell"; },
                };
            },
            async keys() { return [`${prefix}old`, `${prefix}new`, "other-app", "thinoda-shell:https://example.org/Other/:old"]; },
            async delete(key) { deleted.push(key); },
        },
        fetch() { throw new Error("Unexpected network access"); },
    });
    let operation;
    listeners.install({ waitUntil(promise) { operation = promise; } });
    await operation;
    assert.equal(cachedRequests.length, 3);
    assert.ok(cachedRequests.every(request => request.cache === "reload"));
    listeners.activate({ waitUntil(promise) { operation = promise; } });
    await operation;
    assert.deepEqual(deleted, [`${prefix}old`]);
    async function fetchShell(url, mode = "cors", method = "GET") {
        let response;
        listeners.fetch({
            request: { url, mode, method },
            respondWith(promise) { response = promise; },
        });
        return response;
    }
    assert.equal(await fetchShell(`${scope}?launch=1`, "navigate"), "cached shell");
    assert.equal(matches.at(-1), `${scope}index.html`);
    assert.equal(await fetchShell(`${scope}app.wasm`), "cached shell");
    for (const [url, mode, method] of [
        ["https://ai.example/chat", "cors", "GET"],
        ["https://example.org/Other/app.js", "cors", "GET"],
        [`${scope}private.thinoda`, "cors", "GET"],
        [`${scope}app.js`, "cors", "POST"],
    ]) {
        assert.equal(await fetchShell(url, mode, method), undefined);
    }
    assert.ok(!source.includes("skipWaiting("));
    assert.ok(!source.includes("clients.claim("));
});
