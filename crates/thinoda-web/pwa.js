(() => {
    "use strict";
    let pendingInstall;
    let installed = false;
    let panel;
    let refreshPanel = () => {};
    let startupError = "";

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
                    const observe = () => {
                        const worker = registration.installing;
                        worker?.addEventListener("statechange", () => {
                            if (worker.state === "redundant") {
                                startupError = "Offline shell installation failed.";
                                console.error(startupError);
                                refreshPanel();
                            }
                        });
                    };
                    observe();
                    registration.addEventListener("updatefound", observe);
                })
                .catch(error => {
                    console.error("Thinoda offline setup failed", error);
                    startupError = String(error);
                    refreshPanel();
                });
        });
    }
})();
