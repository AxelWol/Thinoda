(() => {
    "use strict";
    let pendingInstall;
    let installed = false;
    let panel;
    let refreshPanel = () => {};
    let startupError = "";
    let updateListener;
    const notices = [];
    const emit = notice => {
        const message = JSON.stringify(notice);
        if (updateListener) updateListener(message);
        else notices.push(message);
    };
    // This metadata is embedded into the loaded HTML by the post-build hook.
    // Never use the active worker as the identity of the running JS/WASM.
    const identity = document.querySelector?.('meta[name="thinoda-build"]')?.content;
    const [version, revision] = (identity || "").split(":");
    const validBuild = build => build
        && /^\d+\.\d+\.\d+$/.test(build.version)
        && /^[a-f0-9]{16}$/.test(build.revision);
    const running = { version, revision };

    window.addEventListener("beforeinstallprompt", event => {
        event.preventDefault();
        pendingInstall = event;
        refreshPanel();
    });
    window.addEventListener("appinstalled", () => {
        installed = true;
        pendingInstall = undefined;
        refreshPanel();
    });
    const standalone = () => installed
        || window.matchMedia("(display-mode: standalone)").matches
        || window.navigator.standalone === true;

    // Keep the browser prompt inside a real DOM click, not the canvas render loop.
    window.thinodaPwa = {
        watchUpdates(callback) {
            updateListener = callback;
            for (const notice of notices.splice(0)) callback(notice);
        },
        open(title, close, instructions, storage, unavailable) {
            if (panel?.open) return;
            panel?.remove();
            panel = document.createElement("dialog");
            panel.className = "pwa-install";
            panel.setAttribute("aria-label", title);
            for (const name of ["keydown", "keyup"]) {
                panel.addEventListener(name, event => event.stopPropagation());
            }
            const heading = document.createElement("h2");
            heading.textContent = title;
            const help = document.createElement("p");
            help.textContent = instructions;
            const warning = document.createElement("p");
            warning.textContent = storage;
            const status = document.createElement("p");
            status.setAttribute("role", "status");
            const install = document.createElement("button");
            install.textContent = title;
            const dismiss = document.createElement("button");
            dismiss.textContent = close;
            dismiss.addEventListener("click", () => panel.close());
            refreshPanel = () => {
                install.disabled = !pendingInstall || standalone();
                status.textContent = startupError
                    ? `${unavailable}\n${startupError}`
                    : (install.disabled ? unavailable : "");
            };
            install.addEventListener("click", async () => {
                const event = pendingInstall;
                if (!event) return;
                pendingInstall = undefined;
                refreshPanel();
                try {
                    await event.prompt();
                    await event.userChoice;
                } catch (error) {
                    console.error("Thinoda PWA installation failed", error);
                    status.textContent = `${unavailable}\n${String(error)}`;
                }
            });
            panel.append(heading, help, warning, status, install, dismiss);
            document.body.append(panel);
            refreshPanel();
            panel.showModal();
            (install.disabled ? dismiss : install).focus();
        }
    };

    if ("serviceWorker" in navigator && window.isSecureContext) {
        window.addEventListener("load", () => {
            navigator.serviceWorker.register("./sw.js", { scope: "./", updateViaCache: "none" })
                .then(registration => {
                    const key = `thinoda-update:${registration.scope}`;
                    const decode = text => {
                        try { return JSON.parse(text); } catch { return null; }
                    };
                    if (validBuild(running)) {
                        try {
                            const storage = window.localStorage;
                            const record = decode(storage.getItem(key));
                            const seenKey = `${key}:seen:${revision}`;
                            const seen = storage.getItem(seenKey);
                            const pending = decode(storage.getItem(`${key}:pending:${revision}`));
                            // Per-build markers only advance from absent to present. No
                            // mutable "current build" pointer for concurrent old tabs to
                            // overwrite. Write seen even for unobserved boots, silently.
                            if (!validBuild(record?.baseline)) {
                                storage.setItem(key, JSON.stringify({ baseline: running }));
                                storage.setItem(seenKey, version);
                            } else {
                                storage.setItem(seenKey, version);
                                if (seen === null && pending?.version === version
                                    && pending?.revision === revision
                                    && record.baseline.revision !== revision) {
                                    emit({ kind: "applied", ...running });
                                }
                            }
                        } catch { /* Missing/blocked storage must never infer success. */ }
                    }
                    const ready = () => {
                        if (registration.waiting && navigator.serviceWorker.controller) {
                            registration.waiting.postMessage({ type: "thinoda-build" });
                        }
                    };
                    navigator.serviceWorker.addEventListener("message", event => {
                        const build = event.data;
                        if (event.source !== registration.waiting
                            || build?.type !== "thinoda-build"
                            || !validBuild(build) || !validBuild(running)
                            || build.revision === revision) return;
                        try {
                            window.localStorage.setItem(`${key}:pending:${build.revision}`,
                                JSON.stringify({ version: build.version, revision: build.revision }));
                        } catch { /* Ready is true even when boot tracking is unavailable. */ }
                        emit({ kind: "ready", version: build.version, revision: build.revision });
                    });
                    let lastCheck = -Infinity;
                    let inFlight = false;
                    let failures = 0;
                    let delay = 60 * 60 * 1000;
                    const check = async () => {
                        const elapsed = Date.now() - lastCheck;
                        if (inFlight || (elapsed >= 0 && elapsed < delay)) return;
                        inFlight = true;
                        lastCheck = Date.now();
                        try {
                            await registration.update();
                            failures = 0;
                            delay = 60 * 60 * 1000;
                            ready();
                        } catch (error) {
                            failures = Math.min(failures + 1, 6);
                            delay = Math.min(60 * 60 * 1000, 60 * 1000 * 2 ** failures);
                            console.error("Thinoda update check failed", error);
                        } finally { inFlight = false; }
                    };
                    // Minute timer only checks eligibility; network is hourly, and resume
                    // signals share the same throttle and single-flight guard.
                    window.setInterval(check, 60 * 1000);
                    window.addEventListener("focus", check);
                    window.addEventListener("pageshow", check);
                    window.addEventListener("online", check);
                    document.addEventListener?.("visibilitychange", () => {
                        if (document.visibilityState === "visible") check();
                    });
                    const observe = () => {
                        const worker = registration.installing;
                        worker?.addEventListener("statechange", () => {
                            if (worker.state === "redundant") {
                                startupError = "Offline shell installation failed.";
                                console.error(startupError);
                                refreshPanel();
                            }
                            if (worker.state === "installed") ready();
                        });
                    };
                    observe();
                    registration.addEventListener("updatefound", observe);
                    ready();
                    check();
                })
                .catch(error => {
                    console.error("Thinoda offline setup failed", error);
                    startupError = String(error);
                    refreshPanel();
                });
        });
    }
})();
