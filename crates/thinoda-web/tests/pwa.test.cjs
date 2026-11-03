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

test("startup loader announces real phases and respects reduced motion", () => {
    const html = fs.readFileSync(path.join(root, "index.html"), "utf8");
    const loader = html.match(/<div id="loading"[^>]*>/)?.[0];
    assert.match(loader || "", /role="status"/);
    assert.match(loader || "", /aria-live="polite"/);
    assert.match(html, /id="loading-message">Loading Thinoda…<\/span>/);
    assert.match(html, /@keyframes loading-pulse/);
    assert.match(html, /@media \(prefers-reduced-motion: reduce\)[\s\S]*?#loading img \{ animation: none; \}/);

    const startup = fs.readFileSync(path.join(root, "src", "lib.rs"), "utf8");
    assert.match(startup, /set_loading_phase\("Preparing browser documents…"\)/);
    assert.match(startup, /set_loading_phase\("Starting Thinoda…"\)/);
    assert.doesNotMatch(startup, /set_attribute\("aria-busy", "true"\)/);
    assert.match(startup, /"role", "alert"/);
    assert.match(startup, /Reload Thinoda/);
});

function bootstrapFixture({ initiallyMissing = false } = {}) {
    const html = fs.readFileSync(path.join(root, "index.html"), "utf8");
    const source = html.match(/<script id="thinoda-bootstrap-guard">([\s\S]*?)<\/script>/)?.[1];
    assert.ok(source, "bootstrap guard is present");
    const listeners = {};
    const loading = {
        attributes: {}, children: [], textContent: "Loading", className: "",
        setAttribute(name, value) { this.attributes[name] = value; },
        append(...children) { this.children.push(...children); },
    };
    let mounted = !initiallyMissing;
    const document = {
        readyState: initiallyMissing ? "loading" : "complete",
        getElementById: () => mounted ? loading : null,
        addEventListener(name, callback) { listeners[`document:${name}`] = callback; },
        createElement: tag => ({ tag, textContent: "" }),
    };
    const window = {
        WebAssembly,
        location: { href: "https://example.org/Thinoda/?keep=1#section" },
        sessionStorage: {
            values: new Map(),
            setItem(key, value) { this.values.set(key, value); },
        },
        addEventListener(name, callback) { listeners[name] = callback; },
    };
    vm.runInNewContext(source, { window, document, URL });
    return {
        window, listeners, loading,
        mount() {
            mounted = true;
            document.readyState = "interactive";
            listeners["document:DOMContentLoaded"]();
        },
    };
}

test("bootstrap failures expose an accessible reload without masking unrelated errors", () => {
    const unrelated = bootstrapFixture();
    unrelated.listeners.error({
        target: { id: "another-script" },
    });
    assert.equal(unrelated.loading.className, "");

    for (const fail of [
        fixture => fixture.listeners.error({ target: { id: "thinoda-app-bootstrap" } }),
        fixture => fixture.window.thinodaBootstrapFailed(),
    ]) {
        const fixture = bootstrapFixture();
        fail(fixture);
        assert.equal(fixture.loading.className, "error");
        assert.equal(fixture.loading.attributes.role, "alert");
        assert.equal(fixture.loading.attributes["aria-live"], "assertive");
        assert.equal(fixture.loading.children[0].textContent, "Thinoda could not load");
        assert.equal(fixture.loading.children[2].textContent, "Reload Thinoda");
        const retry = new URL(fixture.loading.children[2].href);
        assert.equal(retry.searchParams.get("keep"), "1");
        assert.ok(retry.searchParams.get("__thinoda_retry"));
        assert.equal(retry.hash, "#section");
        assert.equal(fixture.window.sessionStorage.values.get("thinoda-bootstrap-retry"),
            retry.searchParams.get("__thinoda_retry"));
    }

    const started = bootstrapFixture();
    started.listeners.TrunkApplicationStarted();
    started.window.thinodaBootstrapFailed();
    assert.equal(started.loading.className, "");

    const blockedStorage = bootstrapFixture();
    blockedStorage.window.sessionStorage.setItem = () => { throw new Error("storage blocked"); };
    blockedStorage.window.thinodaBootstrapFailed();
    assert.ok(new URL(blockedStorage.loading.children[2].href).searchParams.get("__thinoda_retry"));

    const early = bootstrapFixture({ initiallyMissing: true });
    early.listeners.error({ target: { id: "thinoda-app-bootstrap" } });
    assert.equal(early.loading.className, "");
    early.mount();
    assert.equal(early.loading.attributes.role, "alert");

    const trunk = fs.readFileSync(path.join(root, "Trunk.toml"), "utf8");
    assert.match(trunk, /new URL\('\{base\}\{js\}', document\.baseURI\)/);
    assert.match(trunk, /script\.integrity = preload\.integrity/);
    assert.match(trunk, /import\(moduleUrl\.href\)/);
    assert.match(trunk, /bindings\.default\(\{ module_or_path: '\{base\}\{wasm\}' \}\)/);
    assert.match(trunk, /window\.wasmBindings = bindings/);
    assert.match(trunk, /TrunkApplicationStarted/);
    assert.match(trunk, /catch \(error\)[\s\S]*thinodaBootstrapFailed/);
});

test("the bootstrap verifies the app module on initial and retry loads", async () => {
    const trunk = fs.readFileSync(path.join(root, "Trunk.toml"), "utf8");
    const source = trunk.match(/pattern_script = '''\s*<script[^>]*>([\s\S]*?)<\/script>\s*'''/)?.[1];
    assert.ok(source, "Trunk bootstrap pattern is present");
    const executable = source.replace("await import(moduleUrl.href)", "await loadBindings(moduleUrl.href)");
    assert.notEqual(executable, source, "the bindings import is present");

    async function run({ retry = false, integrity = "sha384-example", failScript = false } = {}) {
        const href = `https://example.org/Thinoda/?keep=1${retry ? "&__thinoda_retry=again" : ""}#section`;
        const moduleHref = new URL("{base}{js}", href).href;
        const scripts = [];
        const imports = [];
        const events = [];
        let failed = false;
        let replacedUrl;
        const bindings = { default: async () => "wasm" };
        const document = {
            baseURI: href,
            querySelectorAll: () => [{ href: moduleHref, integrity, crossOrigin: "anonymous" }],
            createElement: tag => {
                assert.equal(tag, "script");
                return {};
            },
            head: {
                append(script) {
                    scripts.push(script);
                    if (failScript) script.onerror();
                    else script.onload();
                },
            },
        };
        const window = {
            location: { href },
            sessionStorage: { getItem: () => null, removeItem: () => {} },
            history: { state: null, replaceState: (_, __, value) => { replacedUrl = value; } },
            thinodaBootstrapFailed: () => { failed = true; },
        };
        const loadBindings = async url => {
            imports.push(url);
            return bindings;
        };
        await vm.runInNewContext(`(async () => { ${executable} })()`, {
            URL, window, document, loadBindings,
            console: { warn() {}, error() {} },
            CustomEvent: class { constructor(type) { this.type = type; } },
            dispatchEvent: event => { events.push(event.type); },
        });
        return { scripts, imports, events, failed, replacedUrl, bindings: window.wasmBindings };
    }

    for (const retry of [false, true]) {
        const result = await run({ retry });
        assert.equal(result.failed, false);
        assert.equal(result.scripts.length, 1);
        assert.equal(result.scripts[0].type, "module");
        assert.equal(result.scripts[0].integrity, "sha384-example");
        assert.equal(result.scripts[0].crossOrigin, "anonymous");
        assert.equal(result.imports.length, 1);
        assert.equal(result.imports[0], result.scripts[0].src);
        assert.equal(new URL(result.imports[0]).searchParams.has("__thinoda_retry"), retry);
        assert.deepEqual(result.events, ["TrunkApplicationStarted"]);
        assert.ok(result.bindings);
        if (retry) {
            const cleaned = new URL(result.replacedUrl);
            assert.equal(cleaned.searchParams.get("keep"), "1");
            assert.equal(cleaned.searchParams.has("__thinoda_retry"), false);
            assert.equal(cleaned.hash, "#section");
        }
    }
    for (const options of [{ integrity: "" }, { failScript: true }]) {
        const result = await run(options);
        assert.equal(result.failed, true);
        assert.equal(result.imports.length, 0);
        assert.equal(result.events.length, 0);
    }
});

test("the bootstrap signal settles only on Wasm initialization or a scoped failure", async () => {
    const success = bootstrapFixture();
    let settled = false;
    success.window.thinodaBootstrapSettled.then(() => { settled = true; });
    success.listeners.error({ target: { id: "another-script" } });
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(settled, false);
    success.listeners.TrunkApplicationStarted();
    await success.window.thinodaBootstrapSettled;
    assert.equal(settled, true);

    const early = bootstrapFixture({ initiallyMissing: true });
    early.listeners.error({ target: { id: "thinoda-app-bootstrap" } });
    await early.window.thinodaBootstrapSettled;
    assert.equal(early.loading.className, "");
    early.mount();
    assert.equal(early.loading.attributes.role, "alert");
});

function installFixture({ standalone = false, register, build, bootstrap, storage = new Map(), storageFails = false } = {}) {
    const listeners = {};
    const errors = [];
    const elements = [];
    const timers = [];
    const workerListeners = {};
    const documentListeners = {};
    let now = 0;
    const window = {
        navigator: {},
        matchMedia: () => ({ matches: standalone }),
        addEventListener: (name, callback) => { listeners[name] = callback; },
        isSecureContext: true,
        setInterval(callback) { timers.push(callback); },
        localStorage: {
            getItem(key) { if (storageFails) throw Error("blocked"); return storage.get(key) || null; },
            setItem(key, value) { if (storageFails) throw Error("quota"); storage.set(key, value); },
        },
    };
    if (bootstrap !== undefined) window.thinodaBootstrapSettled = bootstrap;
    if (register) window.navigator.serviceWorker = {
        register, controller: {},
        addEventListener(name, callback) { workerListeners[name] = callback; },
    };
    const document = {
        querySelector() { return build ? { content: build } : null; },
        addEventListener(name, callback) { documentListeners[name] = callback; },
        visibilityState: "visible",
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
        Date: { now: () => now },
    });
    return {
        window, listeners, elements, errors, timers, storage, workerListeners, documentListeners,
        advance(ms) { now += ms; },
        open() { window.thinodaPwa.open("Install", "Close", "Help", "Storage warning", "Unavailable"); },
        button() { return elements.find(element => element.tag === "button"); },
        status() { return elements.filter(element => element.tag === "p").at(-1); },
    };
}

test("offline registration waits for the bootstrap signal and survives a rejected signal", async () => {
    let resolveBootstrap;
    let registrations = 0;
    const bootstrap = new Promise(resolve => { resolveBootstrap = resolve; });
    const r = registrationFixture();
    const fixture = installFixture({
        bootstrap, register: async () => { registrations++; return r.registration; },
    });
    fixture.listeners.load();
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(registrations, 0);
    resolveBootstrap();
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(registrations, 1);

    const alreadySettled = installFixture({
        bootstrap: Promise.resolve(),
        register: async () => { registrations++; return r.registration; },
    });
    alreadySettled.listeners.load();
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(registrations, 2);

    const rejected = installFixture({
        bootstrap: Promise.reject(new Error("tracking unavailable")),
        register: async () => { registrations++; return r.registration; },
    });
    rejected.listeners.load();
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(registrations, 3);
    assert.match(rejected.errors[0][0], /bootstrap tracking failed/);
});

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
        register: async () => ({
            installing: worker, addEventListener() {}, scope: "https://example.org/Thinoda/",
            async update() {},
        }),
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
        .replace("__THINODA_FILES__", '["./index.html","./app.wasm","./app.js","./thinoda-web-abcd.js"]')
        .replace("__THINODA_VERSION__", "0.18.0")
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
    assert.equal(cachedRequests.length, 4);
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
    assert.equal(await fetchShell(`${scope}thinoda-web-abcd.js?__thinoda_retry=retry`), "cached shell");
    assert.equal(matches.at(-1), `${scope}thinoda-web-abcd.js`);
    for (const [url, mode, method] of [
        ["https://ai.example/chat", "cors", "GET"],
        ["https://example.org/Other/app.js", "cors", "GET"],
        [`${scope}private.thinoda`, "cors", "GET"],
        [`${scope}app.js`, "cors", "POST"],
        [`${scope}app.js?__thinoda_retry=retry`, "cors", "GET"],
        [`${scope}thinoda-web-abcd.js?__thinoda_retry=retry&other=1`, "cors", "GET"],
        [`${scope}thinoda-web-unlisted.js?__thinoda_retry=retry`, "cors", "GET"],
    ]) {
        assert.equal(await fetchShell(url, mode, method), undefined);
    }
    assert.ok(!source.includes("skipWaiting("));
    assert.ok(!source.includes("clients.claim("));
    let metadata;
    listeners.message({ data: { type: "thinoda-build" }, source: {
        postMessage(message) { metadata = message; },
    } });
    assert.equal(metadata.revision, "new");
    assert.equal(metadata.version, "0.18.0");
});

const settle = () => new Promise(resolve => setImmediate(resolve));
const oldBuild = "0.17.0:1111111111111111";
const newBuild = "0.18.0:2222222222222222";
const scope = "https://example.org/Thinoda/";
function registrationFixture() {
    let checks = 0;
    const registration = {
        scope, waiting: { postMessage() {} }, addEventListener() {},
        async update() { checks++; },
    };
    return { registration, checks: () => checks };
}
async function boot(build, storage, extra = {}) {
    const r = registrationFixture();
    const fixture = installFixture({ build, storage, register: async () => r.registration, ...extra });
    const notices = [];
    fixture.window.thinodaPwa.watchUpdates(message => notices.push(JSON.parse(message)));
    fixture.listeners.load();
    await settle();
    return { ...fixture, ...r, notices };
}
function ready(fixture, version = "0.18.0", revision = "2222222222222222") {
    fixture.workerListeners.message({
        source: fixture.registration.waiting,
        data: { type: "thinoda-build", version, revision },
    });
}

test("background checks are throttled, single-flight and resume-aware", async () => {
    const fixture = await boot(oldBuild);
    assert.equal(fixture.checks(), 1);
    fixture.listeners.focus();
    fixture.listeners.online();
    fixture.documentListeners.visibilitychange();
    await settle();
    assert.equal(fixture.checks(), 1);
    fixture.advance(60 * 60 * 1000);
    let finish;
    fixture.registration.update = () => new Promise(resolve => { finish = resolve; });
    const request = fixture.listeners.pageshow();
    fixture.advance(60 * 60 * 1000);
    fixture.listeners.focus();
    assert.equal(fixture.checks(), 1);
    finish();
    await request;
    fixture.registration.update = async () => { throw Error("offline"); };
    fixture.advance(60 * 60 * 1000);
    await fixture.timers[0]();
    const errors = fixture.errors.length;
    await fixture.listeners.online();
    assert.equal(fixture.errors.length, errors);
    fixture.advance(2 * 60 * 1000);
    await fixture.listeners.online();
    assert.equal(fixture.errors.length, errors + 1);
    fixture.advance(-24 * 60 * 60 * 1000);
    await fixture.listeners.focus();
    assert.equal(fixture.errors.length, errors + 2); // Clock rollback must not suppress checks indefinitely.
    assert.deepEqual(fixture.notices, []);
});

test("repeated web check failures back off to one hour and recover to hourly checks", async () => {
    let attempts = 0;
    let fail = true;
    const r = registrationFixture();
    r.registration.update = async () => {
        attempts++;
        if (fail) throw Error("offline");
    };
    const fixture = await boot(oldBuild, new Map(), { register: async () => r.registration });
    assert.equal(attempts, 1);
    for (const delay of [2, 4, 8, 16, 32, 60, 60]) {
        const before = attempts;
        fixture.advance(delay * 60 * 1000 - 1);
        await fixture.listeners.online();
        assert.equal(attempts, before);
        fixture.advance(1);
        await fixture.listeners.online();
        assert.equal(attempts, before + 1);
    }
    fail = false;
    fixture.advance(60 * 60 * 1000);
    await fixture.listeners.online();
    const recovered = attempts;
    fixture.advance(60 * 60 * 1000 - 1);
    await fixture.listeners.focus();
    assert.equal(attempts, recovered);
    fixture.advance(1);
    await fixture.listeners.focus();
    assert.equal(attempts, recovered + 1);
});

test("waiting is READY, only an actually changed boot is APPLIED, and old tabs cannot roll it back", async () => {
    const storage = new Map();
    const first = await boot(oldBuild, storage);
    assert.deepEqual(first.notices, []); // Initial install is not an update.
    ready(first);
    assert.equal(first.notices.at(-1).kind, "ready");
    assert.equal(first.notices.some(n => n.kind === "applied"), false);
    const oldTab = await boot(oldBuild, storage);
    assert.deepEqual(oldTab.notices, []);
    const changed = await boot(newBuild, storage);
    assert.equal(changed.notices.at(-1).kind, "applied");
    assert.equal(changed.notices.at(-1).version, "0.18.0");
    // A delayed READY message in the older tab cannot erase the new build's seen marker.
    ready(first);
    const secondTab = await boot(newBuild, storage);
    assert.deepEqual(secondTab.notices, []);
    const stale = await boot(oldBuild, storage);
    assert.deepEqual(stale.notices, []);
    const again = await boot(newBuild, storage);
    assert.deepEqual(again.notices, []);
    const otherScope = registrationFixture();
    otherScope.registration.scope = "https://example.org/Other/";
    const other = await boot(newBuild, storage, { register: async () => otherScope.registration });
    assert.deepEqual(other.notices, []);
});

test("failed storage, malformed and spoofed worker messages never create false success", async () => {
    const fixture = await boot(oldBuild, new Map(), { storageFails: true });
    ready(fixture);
    assert.equal(fixture.notices.at(-1).kind, "ready");
    const next = await boot(newBuild, fixture.storage, { storageFails: true });
    assert.deepEqual(next.notices, []);
    const valid = await boot(oldBuild);
    for (const data of [
        { type: "thinoda-build", version: "javascript:alert(1)", revision: "2222222222222222" },
        { type: "thinoda-build", version: "0.18.0", revision: "<bad>" },
        { type: "thinoda-build", version: "0.17.0", revision: "1111111111111111" },
    ]) valid.workerListeners.message({ source: valid.registration.waiting, data });
    valid.workerListeners.message({ source: {}, data: {
        type: "thinoda-build", version: "0.18.0", revision: "2222222222222222",
    } });
    assert.deepEqual(valid.notices, []);
});

test("same-version rebuilt shells use immutable boot identity and buffer notices for Rust", async () => {
    const storage = new Map();
    const first = await boot(oldBuild, storage);
    ready(first, "0.17.0");
    const r = registrationFixture();
    const fixture = installFixture({
        build: "0.17.0:2222222222222222", storage, register: async () => r.registration,
    });
    fixture.listeners.load();
    await settle();
    // Rust can subscribe after registration has already finished.
    const notices = [];
    fixture.window.thinodaPwa.watchUpdates(message => notices.push(JSON.parse(message)));
    assert.equal(notices.length, 1);
    assert.equal(notices[0].kind, "applied");
    assert.equal(notices[0].version, "0.17.0");
    assert.equal(notices[0].revision, "2222222222222222");
    // A changed waiting worker cannot change the loaded HTML's identity.
    fixture.workerListeners.message({
        source: r.registration.waiting,
        data: { type: "thinoda-build", version: "0.19.0", revision: "3333333333333333" },
    });
    assert.equal(notices.at(-1).kind, "ready");
    assert.equal(notices.filter(notice => notice.kind === "applied").length, 1);
});

test("failed applied-state persistence and corrupt storage suppress success", async () => {
    const first = await boot(oldBuild);
    ready(first);
    const next = await boot(newBuild, first.storage, { storageFails: true });
    assert.deepEqual(next.notices, []);
    const corrupt = new Map([[`thinoda-update:${scope}`, "{invalid json"]]);
    const reset = await boot(newBuild, corrupt);
    assert.deepEqual(reset.notices, []);
    assert.equal(JSON.parse(corrupt.get(`thinoda-update:${scope}`)).baseline.version, "0.18.0");
});

test("a failed shell download rejects installation rather than reporting completion", async () => {
    const listeners = {};
    const source = fs.readFileSync(path.join(root, "sw.js"), "utf8")
        .replace("__THINODA_FILES__", '["./index.html","./app.wasm"]')
        .replace("__THINODA_REVISION__", "2222222222222222")
        .replace("__THINODA_VERSION__", "0.18.0");
    vm.runInNewContext(source, {
        self: { registration: { scope }, addEventListener(name, callback) { listeners[name] = callback; } },
        URL, Request,
        caches: { async open() { return { async addAll() { throw Error("offline or quota"); } }; } },
    });
    let install;
    listeners.install({ waitUntil(operation) { install = operation; } });
    await assert.rejects(install, /offline or quota/);
});
