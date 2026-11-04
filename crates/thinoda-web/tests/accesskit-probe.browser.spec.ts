import { expect, test, type Page } from "@playwright/test";

test.setTimeout(120_000);

async function openProbe(page: Page) {
    await page.goto("/");
    const button = page.getByRole("button", { name: "Synthetic action 0" });
    await expect(button).toBeEnabled({ timeout: 60_000 });
    return page.locator("#probe-action");
}

async function focusSyntheticInput(page: Page) {
    const debug = page.locator("#probe-debug");
    const canvas = page.locator("#probe-canvas");
    const bounds = await canvas.boundingBox();
    if (!bounds) throw new Error("Missing probe canvas");
    const x = Number(await debug.getAttribute("data-input-x"));
    const y = Number(await debug.getAttribute("data-input-y"));
    expect(Number.isFinite(x) && Number.isFinite(y) && x > 0 && y > 0).toBe(true);
    await page.mouse.click(bounds.x + x, bounds.y + y);
    await expect(page.locator("input[type='text']")).toBeFocused();
}

async function frames(page: Page) {
    await page.evaluate(() => new Promise<void>(resolve => {
        requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
    }));
}

async function expectChromiumAccessibilityButton(page: Page, name: string, disabled: boolean) {
    const session = await page.context().newCDPSession(page);
    try {
        const { nodes } = await session.send("Accessibility.getFullAXTree");
        const buttons = nodes.filter(node =>
            !node.ignored && node.role?.value === "button" && node.name?.value === name,
        );
        expect(buttons).toHaveLength(1);
        const isDisabled = buttons[0].properties?.some(property =>
            property.name === "disabled" && property.value.value === true,
        ) ?? false;
        expect(isDisabled).toBe(disabled);
    } finally {
        await session.detach();
    }
}

test("the test button is not exposed before the first AccessKit tree", async ({ page }) => {
    let release!: () => void;
    let requested!: () => void;
    const gate = new Promise<void>(resolve => { release = resolve; });
    const wasmRequested = new Promise<void>(resolve => { requested = resolve; });
    await page.route(/\.wasm(?:[?#].*)?$/, async route => {
        requested();
        await gate;
        await route.continue();
    });
    try {
        await page.goto("/", { waitUntil: "commit" });
        await page.locator("#probe-action").waitFor({ state: "attached" });
        await wasmRequested;
        await expect(page.locator("#probe-action")).toBeHidden();
        await expect(page.locator("#probe-action")).toBeDisabled();
        await expect(page.getByRole("button", { name: /Synthetic action/ })).toHaveCount(0);
    } finally {
        release();
    }
    await expect(page.getByRole("button", { name: "Synthetic action 0" }))
        .toBeEnabled({ timeout: 60_000 });
});

test("the test button stays hidden if its action listener cannot be installed", async ({ page }) => {
    await page.addInitScript(() => {
        const original = EventTarget.prototype.addEventListener;
        EventTarget.prototype.addEventListener = function (type, listener, options) {
            if (this instanceof HTMLElement && this.id === "probe-action" && type === "click") {
                throw new Error("Synthetic listener failure");
            }
            return original.call(this, type, listener, options);
        };
    });
    await page.goto("/");
    await expect(page.locator("#probe-debug")).toHaveAttribute("data-count", "0", {
        timeout: 60_000,
    });
    await expect(page.locator("#probe-action")).toBeHidden();
    await expect(page.locator("#probe-action")).toBeDisabled();
    await expect(page.getByRole("button", { name: /Synthetic action/ })).toHaveCount(0);
});

test("a detached test button cannot dispatch a queued or later click", async ({ page }) => {
    const button = await openProbe(page);
    await button.evaluate((element: HTMLButtonElement) => {
        element.click();
        const replacement = element.cloneNode(false) as HTMLButtonElement;
        replacement.hidden = true;
        replacement.disabled = true;
        element.replaceWith(replacement);
        element.click();
    });
    await frames(page);
    await expect(page.locator("#probe-debug")).toHaveAttribute("data-count", "0");
    await expect(page.locator("#probe-action")).toBeHidden();
    await expect(page.locator("#probe-action")).toBeDisabled();
});

test("the synthetic DOM button sends actions back and tracks the disabled state", async ({ page, browserName }) => {
    const button = await openProbe(page);
    await expect(button).toHaveAccessibleName("Synthetic action 0");
    if (browserName === "chromium") {
        await expectChromiumAccessibilityButton(page, "Synthetic action 0", false);
    }
    await button.click();
    await expect(button).toHaveAccessibleName("Synthetic action 1");
    await expect(page.locator("#probe-debug")).toHaveAttribute("data-count", "1");
    // WebKit need not focus a button when it is clicked with a pointer.
    await button.focus();
    await expect(button).toBeFocused();

    await button.press("Enter");
    await expect(button).toHaveAccessibleName("Synthetic action 2");
    await expect(button).toBeDisabled();
    await expect(page.locator("#probe-debug")).toHaveAttribute("data-count", "2");
    if (browserName === "chromium") {
        await expectChromiumAccessibilityButton(page, "Synthetic action 2", true);
    }

    await button.evaluate(element => element.dispatchEvent(new MouseEvent("click", { bubbles: true })));
    await frames(page);
    await expect(page.locator("#probe-debug")).toHaveAttribute("data-count", "2");
});

test("rapid synthetic activations are delivered on separate frames", async ({ page }) => {
    const button = await openProbe(page);
    await button.evaluate((element: HTMLButtonElement) => {
        element.click();
        element.click();
    });
    await expect(button).toHaveAccessibleName("Synthetic action 2");
    await expect(button).toBeDisabled();
    await expect(page.locator("#probe-debug")).toHaveAttribute("data-count", "2");
});

test("the native semantic button supports Space activation", async ({ page }) => {
    const button = await openProbe(page);
    await button.focus();
    await page.keyboard.press("Space");
    await expect(button).toHaveAccessibleName("Synthetic action 1");
    await expect(page.locator("#probe-debug")).toHaveAttribute("data-count", "1");
});

test("Tab navigation can enter the synthetic semantic button", async ({ page }) => {
    const button = await openProbe(page);
    const canvas = page.locator("#probe-canvas");
    await canvas.focus();
    await expect(canvas).toBeFocused();
    let entered = false;
    for (let i = 0; i < 3; i++) {
        await page.keyboard.press("Tab");
        await page.evaluate(() => new Promise<void>(resolve => requestAnimationFrame(() => resolve())));
        if (await button.evaluate(element => document.activeElement === element)) {
            entered = true;
            break;
        }
    }
    expect(entered).toBe(true);
    await page.keyboard.press("Space");
    await expect(button).toHaveAccessibleName("Synthetic action 1");
    await expect(page.locator("#probe-debug")).toHaveAttribute("data-count", "1");
});

test("Tab from the synthetic editor reaches the semantic button without changing the draft", async ({ page }) => {
    const button = await openProbe(page);
    const debug = page.locator("#probe-debug");
    await focusSyntheticInput(page);
    await page.keyboard.insertText("cafe");
    await expect(debug).toHaveAttribute("data-text-probe", "first");
    await page.keyboard.press("Tab");
    await expect(button).toBeFocused();
    await expect(debug).toHaveAttribute("data-text-length", "4");
    await button.press("Enter");
    await expect(debug).toHaveAttribute("data-count", "1");
    await expect(debug).toHaveAttribute("data-text-probe", "first");
});

test("Shift+Tab from the synthetic button returns to the editor", async ({ page }) => {
    const button = await openProbe(page);
    const debug = page.locator("#probe-debug");
    await focusSyntheticInput(page);
    await page.keyboard.insertText("cafe");
    await expect(debug).toHaveAttribute("data-text-probe", "first");
    await page.keyboard.press("Tab");
    await expect(button).toBeFocused();
    await page.keyboard.press("Shift+Tab");
    await expect(page.locator("input[type='text']")).toBeFocused();
    await page.keyboard.insertText("猫");
    await expect(debug).toHaveAttribute("data-text-probe", "continued");
});

test("immediate synthetic text reaches an unblocked editor focus request", async ({ page }) => {
    const button = await openProbe(page);
    const debug = page.locator("#probe-debug");
    await focusSyntheticInput(page);
    await page.keyboard.insertText("cafe");
    await expect(debug).toHaveAttribute("data-text-probe", "first");
    await page.keyboard.press("Tab");
    await expect(button).toBeFocused();
    await page.evaluate(() => {
        const agent = document.querySelector<HTMLInputElement>("input[type='text']")!;
        agent.focus();
        agent.value += "猫";
        agent.dispatchEvent(new InputEvent("input", {
            bubbles: true, inputType: "insertText", data: "猫",
        }));
    });
    await expect(debug).toHaveAttribute("data-text-probe", "continued");
    await expect(debug).toHaveAttribute("data-input-focused", "true");
});

test("a pending click can drop synthetic text sent during the focus return", async ({ page }) => {
    const button = await openProbe(page);
    const debug = page.locator("#probe-debug");
    await focusSyntheticInput(page);
    await page.keyboard.insertText("cafe");
    await expect(debug).toHaveAttribute("data-text-probe", "first");
    await page.keyboard.press("Tab");
    await expect(button).toBeFocused();
    await page.evaluate(() => {
        const button = document.querySelector<HTMLButtonElement>("#probe-action")!;
        const agent = document.querySelector<HTMLInputElement>("input[type='text']")!;
        button.click();
        agent.focus();
        agent.value += "猫";
        agent.dispatchEvent(new InputEvent("input", {
            bubbles: true, inputType: "insertText", data: "猫",
        }));
    });
    await expect(debug).toHaveAttribute("data-count", "1");
    await frames(page);
    await expect(debug).toHaveAttribute("data-text-probe", "first");
    await expect(debug).toHaveAttribute("data-text-length", "4");
});

test("returning through the semantic button collapses a selected text range", async ({ page }) => {
    const button = await openProbe(page);
    const debug = page.locator("#probe-debug");
    await focusSyntheticInput(page);
    await page.keyboard.insertText("cafe");
    await expect(debug).toHaveAttribute("data-text-probe", "first");
    await expect(debug).toHaveAttribute("data-input-focused", "true");
    await page.keyboard.press("Shift+ArrowLeft");
    // Keep this selection test separate from the mixed-event focus hazard below.
    await expect(debug).toHaveAttribute("data-selection", "3:4");
    await expect(debug).toHaveAttribute("data-arrow-left-released", "true");
    await page.keyboard.press("Tab");
    await expect(debug).toHaveAttribute("data-input-focused", "false");
    await expect(debug).toHaveAttribute("data-selection", "3:3");
    // Isolate selection from the separately characterized DOM focus race.
    await button.focus();
    await expect(button).toBeFocused();
    await page.keyboard.press("Shift+Tab");
    await expect(page.locator("input[type='text']")).toBeFocused();
    await expect(debug).toHaveAttribute("data-input-focused", "true");
    await expect(debug).toHaveAttribute("data-selection", "3:3");
    await page.keyboard.insertText("猫");
    await expect(debug).toHaveAttribute("data-text-probe", "selection-lost");
    await expect(debug).toHaveAttribute("data-text-length", "5");
});

test("a mixed keyboard frame can leave focus on the canvas instead of the semantic button", async ({ page }) => {
    const button = await openProbe(page);
    const debug = page.locator("#probe-debug");
    await focusSyntheticInput(page);
    await page.keyboard.insertText("cafe");
    await expect(debug).toHaveAttribute("data-text-probe", "first");
    await page.evaluate(() => {
        const agent = document.querySelector<HTMLInputElement>("input[type='text']")!;
        const events: Array<[string, string, number, boolean]> = [
            ["keydown", "ArrowLeft", 37, true],
            ["keyup", "ArrowLeft", 37, true],
            ["keydown", "Tab", 9, false],
            ["keyup", "Tab", 9, false],
        ];
        for (const [type, key, keyCode, shiftKey] of events) {
            agent.dispatchEvent(new KeyboardEvent(type, {
                bubbles: true, cancelable: true, key, code: key, keyCode, shiftKey,
            }));
        }
    });
    await frames(page);
    await expect(page.locator("#probe-canvas")).toBeFocused();
    await expect(button).not.toBeFocused();
    await expect(debug).toHaveAttribute("data-input-focused", "false");
    await expect(debug).toHaveAttribute("data-text-probe", "first");
});

test("a focus request is canceled if focus immediately moves elsewhere", async ({ page }) => {
    const button = await openProbe(page);
    const debug = page.locator("#probe-debug");
    await focusSyntheticInput(page);
    await page.keyboard.press("Tab");
    await expect(button).toBeFocused();
    await expect(debug).toHaveAttribute("data-input-focused", "false");
    await page.evaluate(() => {
        document.querySelector<HTMLInputElement>("input[type='text']")!.focus();
        document.querySelector<HTMLCanvasElement>("#probe-canvas")!.focus();
    });
    await frames(page);
    await expect(page.locator("#probe-canvas")).toBeFocused();
    await expect(debug).toHaveAttribute("data-input-focused", "false");
});

test("a quick synthetic return to the canvas can select and replace text", async ({ page }) => {
    await openProbe(page);
    const debug = page.locator("#probe-debug");
    await focusSyntheticInput(page);
    await page.keyboard.insertText("cafe");
    await expect(debug).toHaveAttribute("data-text-probe", "first");
    await page.waitForTimeout(350);

    const elapsed = await page.evaluate(() => {
        const canvas = document.getElementById("probe-canvas") as HTMLCanvasElement;
        const button = document.getElementById("probe-action") as HTMLButtonElement;
        const debug = document.getElementById("probe-debug")!;
        const bounds = canvas.getBoundingClientRect();
        const position = {
            clientX: bounds.x + Number(debug.getAttribute("data-input-x")),
            clientY: bounds.y + Number(debug.getAttribute("data-input-y")),
        };
        const clickCanvas = () => {
            for (const [type, buttons] of [["pointerdown", 1], ["pointerup", 0]] as const) {
                canvas.dispatchEvent(new PointerEvent(type, {
                    ...position, bubbles: true, button: 0, buttons, pointerId: 1, pointerType: "mouse",
                }));
            }
        };
        const started = performance.now();
        clickCanvas();
        button.focus();
        button.click();
        const elapsed = performance.now() - started;
        clickCanvas();
        return elapsed;
    });

    test.skip(elapsed >= 250, `Synthetic clicks were too slow to exercise the 300 ms interval: ${elapsed} ms`);
    await expect(debug).toHaveAttribute("data-count", "1");
    await expect(debug).toHaveAttribute("data-last-input-click", "double");
    await expect(page.locator("input[type='text']")).toBeFocused();
    await page.keyboard.insertText("猫");
    await expect(debug).toHaveAttribute("data-text-length", "1");
    await expect(debug).toHaveAttribute("data-text-probe", "other");
});

test("the browser text agent can resume after the separate DOM button takes focus", async ({ page }) => {
    const button = await openProbe(page);
    const debug = page.locator("#probe-debug");
    await focusSyntheticInput(page);
    await page.keyboard.insertText("cafe");
    await expect(debug).toHaveAttribute("data-text-length", "4");
    await expect(debug).toHaveAttribute("data-text-probe", "first");
    await expect(debug).toHaveAttribute("data-host-focused", "true");

    await button.focus();
    await button.press("Enter");
    await expect(button).toHaveAccessibleName("Synthetic action 1");
    await expect(button).toBeFocused();
    // The existing web backend recognizes only its canvas and text agent as
    // host focus. The proof deliberately does not pretend to solve this.
    await expect(debug).toHaveAttribute("data-host-focused", "false");

    // Let egui's 300 ms double-click interval expire before the next canvas
    // click; the DOM click is not part of egui's pointer click sequence.
    await page.waitForTimeout(350);
    await focusSyntheticInput(page);
    await page.keyboard.insertText("猫");
    await expect(debug).toHaveAttribute("data-text-length", "5");
    await expect(debug).toHaveAttribute("data-text-probe", "continued");
    await expect(debug).toHaveAttribute("data-host-focused", "true");
});
