// Startup stays gated: these choices never clear storage or enable unsafe saves.
export function renderStorageRecovery(loading, kind) {
    const document = loading.ownerDocument;
    const add = (tag, text, parent = loading) => {
        const element = document.createElement(tag);
        element.textContent = text;
        parent.append(element);
        return element;
    };
    add("h2", "Let's protect your saved documents");
    const explanations = {
        integrity: "Thinoda could not read or check one of your saved documents. "
            + "This does not mean all your documents are lost.",
        permission: "Your browser is not allowing Thinoda to use its document storage.",
        quota: "Your browser has run out of space for Thinoda's saved documents.",
        blocked: "Another Thinoda tab is preventing this tab from preparing your saved documents.",
        unavailable: "Document storage is not available in this browser window.",
        other: "Thinoda cannot access your saved documents right now.",
    };
    add("p", explanations[kind] ?? explanations.other);
    add("p", "Thinoda has not deleted your saved data. The editor has not opened, "
        + "so it cannot accidentally replace a document that could not be checked. "
        + "Do not clear this site's browser data: that can permanently remove documents.");
    add("h3", "What would you like to do?");
    const steps = add("ol", "");
    const retryHelp = {
        permission: "Allow this site to store data in your browser's privacy settings, then retry.",
        quota: "Free some space on your device without clearing Thinoda's site data, then retry.",
        blocked: "Close other Thinoda tabs or windows, then retry here.",
    };
    add("li", (retryHelp[kind] ?? "Close other Thinoda tabs, then retry here.")
        + " Reload Thinoda below checks again; it does not erase your documents.", steps);
    add("li", "If retrying does not help, save the older browser copies below. "
        + "This downloads a recovery archive without changing anything in the browser. "
        + "It contains only older local copies, not newer saves or every saved document. "
        + "Keep any .thinoda files you have already downloaded too.", steps);
    const download = add("button", "Download older browser copies");
    download.type = "button";
    const status = add("p", "");
    status.setAttribute("role", "status");
    status.setAttribute("aria-live", "polite");
    download.addEventListener("click", () => {
        try {
            const entries = [];
            for (let i = 0; i < localStorage.length; i++) {
                const key = localStorage.key(i);
                if (key === "thinoda.vfs:/Documents" || key?.startsWith("thinoda.vfs:/Documents/")) {
                    entries.push([key, localStorage.getItem(key)]);
                }
            }
            if (!entries.length) {
                status.textContent = "No older browser copies are available to download. "
                    + "Your newer saved data has not been changed. Keep this browser profile "
                    + "and any downloaded documents, and ask for help with recovery.";
                return;
            }
            const archive = {
                format: "thinoda-legacy-browser-recovery",
                version: 1,
                entries: entries.sort(([a], [b]) => a.localeCompare(b)),
            };
            const blob = new Blob([JSON.stringify(archive)], { type: "application/json" });
            const url = URL.createObjectURL(blob);
            try {
                const link = document.createElement("a");
                link.href = url;
                link.download = "thinoda-older-browser-copies.json";
                loading.append(link);
                link.click();
                link.remove();
            } finally {
                setTimeout(() => URL.revokeObjectURL(url), 60_000);
            }
            status.textContent = "Recovery download requested. Keep the JSON file for help with recovery; "
                + "it is not a document to open in Thinoda. It contains older copies only, "
                + "not newer saves. Your browser data has not been changed.";
        } catch {
            console.error("Thinoda could not download older browser copies");
            status.textContent = "The recovery download could not be created. No browser data was changed. "
                + "Keep this browser profile and any downloaded documents, and ask for help with recovery.";
        }
    });
    const details = add("details", "");
    add("summary", "If neither option works", details);
    add("p", "Keep this browser profile and do not clear Thinoda's site data. "
        + "Ask for help with document recovery and mention this storage screen. "
        + "The recovery archive may contain private document content: share it only with someone you trust.",
    details);
}
