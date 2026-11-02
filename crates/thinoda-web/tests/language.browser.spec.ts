import { expect, test, type Page } from "@playwright/test";
import { command, frames } from "./board-browser-helpers";

const url = process.env.THINODA_WEB_URL ?? "http://127.0.0.1:8080/";
const english = "Crash recovery is unavailable. Save or download your work.";
const german = "Die Wiederherstellung nach einem Absturz ist nicht verfügbar. Speichern Sie Ihre Arbeit oder laden Sie sie herunter.";
const portuguese = "A recuperação após falha não está disponível. Guarde ou transfira o seu trabalho.";
const brazilian = "A recuperação após falha não está disponível. Salve ou baixe seu trabalho.";

test.setTimeout(180_000);

async function denyRecoveryStorage(page: Page, setting = "") {
    await page.addInitScript((preference) => {
        if (preference) {
            const text = `language = "${preference}"\n`;
            localStorage.setItem("thinoda.vfs:/thinoda/config/settings.toml", JSON.stringify({
                m: 1,
                l: text.length,
                d: btoa(text),
            }));
        }
        const original = Storage.prototype.setItem;
        Storage.prototype.setItem = function (key, value) {
            if (key.startsWith("thinoda.vfs:") && key.includes("/recovery/")) {
                throw new DOMException("synthetic quota", "QuotaExceededError");
            }
            return original.call(this, key, value);
        };
    }, setting);
}

for (const scenario of [
    { name: "German browser", locale: "de-DE", setting: "", lang: "de", alert: german },
    { name: "European Portuguese browser", locale: "pt-PT", setting: "", lang: "pt-PT", alert: portuguese },
    { name: "Brazilian Portuguese browser", locale: "pt-BR", setting: "", lang: "pt-BR", alert: brazilian },
    { name: "unsupported browser language", locale: "ja-JP", setting: "", lang: "en", alert: english },
    { name: "explicit English preference", locale: "de-DE", setting: "english", lang: "en", alert: english },
    { name: "explicit German preference", locale: "en-US", setting: "german", lang: "de", alert: german },
]) {
    test.describe(scenario.name, () => {
        test.use({
            locale: scenario.locale,
            viewport: { width: 1024, height: 768 },
            serviceWorkers: "block",
        });
        test("uses the active language for the document and recovery announcement", async ({ page }) => {
            await denyRecoveryStorage(page, scenario.setting);
            await page.goto(url);
            await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
            await expect(page.locator("html")).toHaveAttribute("lang", scenario.lang);
            const alert = page.locator("#thinoda-recovery-availability");
            await expect(alert).toHaveAttribute("lang", scenario.lang);
            await expect(alert).toHaveText(scenario.alert);
        });
    });
}

test.describe("changing a saved language preference", () => {
    test.use({
        locale: "en-US",
        viewport: { width: 1252, height: 900 },
        serviceWorkers: "block",
    });
    test("updates the document without mislabeling an existing announcement", async ({ page }) => {
        await denyRecoveryStorage(page);
        await page.goto(url);
        await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
        const root = page.locator("html");
        const alert = page.locator("#thinoda-recovery-availability");
        await expect(root).toHaveAttribute("lang", "en");
        await expect(alert).toHaveAttribute("lang", "en");
        await expect(alert).toHaveText(english);

        await command(page, "Settings");
        // Egui positions the dialog and its popup over subsequent frames.
        await page.waitForTimeout(500);
        await page.mouse.click(795, 674);
        await frames(page);
        await page.waitForTimeout(500);
        await page.mouse.click(780, 531);
        await expect(root).toHaveAttribute("lang", "de");
        await expect(alert).toHaveAttribute("lang", "en");
        await expect(alert).toHaveText(english);

        await page.reload();
        await expect(page.locator("#loading")).toBeHidden({ timeout: 120_000 });
        await expect(root).toHaveAttribute("lang", "de");
        await expect(alert).toHaveAttribute("lang", "de");
        await expect(alert).toHaveText(german);
    });
});
