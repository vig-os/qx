// Deploy config e2e tests — verify that deploy-config.json settings
// propagate correctly to the FE at runtime.
//
// Uses window.__PART_REGISTRY_CONFIG__ runtime override (injected via
// addInitScript before page load) to test different configurations
// against the same build. No rebuild needed per test.

import { expect, test, type Page } from "@playwright/test";
import { readFileSync } from "fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const FIXTURE_CSV = readFileSync(
  resolve(__dirname, "fixtures/registry.csv"),
  "utf-8",
);

/** Inject a partial deploy config override before page load. */
async function withConfig(page: Page, override: Record<string, unknown>) {
  await page.addInitScript((cfg) => {
    (window as any).__PART_REGISTRY_CONFIG__ = cfg;
  }, override);
}

test.beforeEach(async ({ page }) => {
  await page.route("**/registry.csv*", async (route) => {
    await route.fulfill({
      status: 200,
      headers: { "content-type": "text/csv" },
      body: FIXTURE_CSV,
    });
  });
  // Clear user preferences so defaults from config apply
  await page.addInitScript(() => {
    window.localStorage.clear();
    window.sessionStorage.clear();
  });
});

// ---- Code type filtering ----

test.describe("labels.allowedCodeTypes", () => {
  test("restricts code type dropdown to allowed types only", async ({ page }) => {
    await withConfig(page, {
      labels: { allowedCodeTypes: ["micro_qr"] },
    });
    await page.goto("/");
    await page.locator("nav.tabs").getByRole("button", { name: "Print" }).click();

    const codeTypeSelect = page.locator(".label-settings__field select").first();
    await codeTypeSelect.waitFor({ state: "visible" });

    const options = await codeTypeSelect.locator("option").allTextContents();
    expect(options).toHaveLength(1);
    expect(options[0]).toContain("Micro QR");

    // Should be disabled when only 1 option
    await expect(codeTypeSelect).toBeDisabled();
  });

  test("shows all types when config allows all", async ({ page }) => {
    await withConfig(page, {
      labels: { allowedCodeTypes: ["standard_qr", "micro_qr", "data_matrix"] },
    });
    await page.goto("/");
    await page.locator("nav.tabs").getByRole("button", { name: "Print" }).click();

    const codeTypeSelect = page.locator(".label-settings__field select").first();
    await codeTypeSelect.waitFor({ state: "visible" });

    const options = await codeTypeSelect.locator("option").allTextContents();
    expect(options.length).toBeGreaterThanOrEqual(3);
    await expect(codeTypeSelect).toBeEnabled();
  });
});

// ---- Payload format filtering ----

test.describe("labels.allowedPayloadFormats", () => {
  test("restricts payload dropdown to allowed formats", async ({ page }) => {
    await withConfig(page, {
      labels: { allowedPayloadFormats: ["id_only", "prefixed_id"] },
    });
    await page.goto("/");
    await page.locator("nav.tabs").getByRole("button", { name: "Print" }).click();

    // Payload dropdown is the second select in label-settings__field
    const payloadSelect = page.locator(".label-settings__field select").nth(1);
    await payloadSelect.waitFor({ state: "visible" });

    const options = await payloadSelect.locator("option").allTextContents();
    expect(options).toHaveLength(2);
    expect(options.some((o) => o.includes("Raw ID"))).toBe(true);
    expect(options.some((o) => o.includes("Prefixed"))).toBe(true);
    expect(options.some((o) => o.includes("URL"))).toBe(false);
  });
});

// ---- Feature flags ----

test.describe("features", () => {
  test("enableMintTab: false hides the Mint tab", async ({ page }) => {
    await withConfig(page, {
      features: { enableMintTab: false },
    });
    await page.goto("/");
    // Wait for tabs to render
    await page.locator("nav.tabs .tab-btn").first().waitFor({ state: "visible" });

    const tabTexts = await page.locator("nav.tabs .tab-btn").allTextContents();
    const tabNames = tabTexts.map((t) => t.replace(/\d+/g, "").trim());

    expect(tabNames).toContain("Lookup");
    expect(tabNames).toContain("Print");
    expect(tabNames).toContain("Bind");
    expect(tabNames).not.toContain("Mint");
  });

  test("enablePrintTab: false hides the Print tab", async ({ page }) => {
    await withConfig(page, {
      features: { enablePrintTab: false },
    });
    await page.goto("/");
    await page.locator("nav.tabs .tab-btn").first().waitFor({ state: "visible" });

    const tabTexts = await page.locator("nav.tabs .tab-btn").allTextContents();
    const tabNames = tabTexts.map((t) => t.replace(/\d+/g, "").trim());

    expect(tabNames).toContain("Lookup");
    expect(tabNames).not.toContain("Print");
    expect(tabNames).toContain("Bind");
    expect(tabNames).toContain("Mint");
  });
});

// ---- Presentation ----

test.describe("presentation", () => {
  test("appTitle overrides the page title", async ({ page }) => {
    await withConfig(page, {
      presentation: { appTitle: "ExoPET Asset Tracker" },
    });
    await page.goto("/");

    const title = page.locator(".shell__title");
    await expect(title).toContainText("ExoPET Asset Tracker");
  });
});

// ---- Default values ----

test.describe("defaults", () => {
  test("defaultCodeType is pre-selected in the dropdown", async ({ page }) => {
    await withConfig(page, {
      labels: {
        allowedCodeTypes: ["standard_qr", "micro_qr", "data_matrix"],
        defaultCodeType: "data_matrix",
      },
    });
    await page.goto("/");
    await page.locator("nav.tabs").getByRole("button", { name: "Print" }).click();

    const codeTypeSelect = page.locator(".label-settings__field select").first();
    await codeTypeSelect.waitFor({ state: "visible" });
    await expect(codeTypeSelect).toHaveValue("data_matrix");
  });
});

// ---- Payload format in live preview ----

test.describe("payload format preview", () => {
  test("shows encoded payload string in live preview", async ({ page }) => {
    // Pre-set a plan item before page load
    await page.addInitScript(() => {
      const plan = [{ id: "ABCDEFGHJKMNPQ", layoutId: "horz", size: 11, copies: 1, extras: {} }];
      window.localStorage.setItem("qx.print-plan", JSON.stringify(plan));
    });
    await page.goto("/");
    await page.locator("nav.tabs .tab-btn >> text=Print").click();

    // Wait for live preview to render (debounced at 200ms)
    const preview = page.locator(".label-preview--live");
    await expect(preview.locator("code")).toContainText("Payload:", { timeout: 5000 });
    await expect(preview.locator("code")).toContainText("chars");
  });
});
