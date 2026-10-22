// The build hook inserts a content revision and the exact compiled app shell.
const FILES = __THINODA_FILES__;
const REVISION = "__THINODA_REVISION__";
const PREFIX = `thinoda-shell:${self.registration.scope}:`;
const CACHE = PREFIX + REVISION;
const urls = FILES.map(path => new URL(path, self.registration.scope).href);

self.addEventListener("install", event => {
    event.waitUntil((async () => {
        const cache = await caches.open(CACHE);
        await cache.addAll(urls.map(url => new Request(url, { cache: "reload" })));
    })());
    // No skipWaiting: open documents keep their current, matching JS/WASM shell.
});
self.addEventListener("activate", event => {
    event.waitUntil((async () => {
        for (const key of await caches.keys()) {
            if (key.startsWith(PREFIX) && key !== CACHE) await caches.delete(key);
        }
    })());
});
self.addEventListener("fetch", event => {
    const request = event.request;
    if (request.method !== "GET") return;
    const url = new URL(request.url);
    const root = new URL(self.registration.scope);
    if (url.origin !== root.origin || !url.pathname.startsWith(root.pathname)) return;
    const target = request.mode === "navigate"
        && (url.pathname === root.pathname || url.pathname === `${root.pathname}index.html`)
        ? new URL("index.html", root).href
        : url.href;
    if (!urls.includes(target)) return; // Never cache documents, AI or unrelated sites.
    event.respondWith((async () => {
        const cache = await caches.open(CACHE);
        return (await cache.match(target)) || fetch(request);
    })());
});
