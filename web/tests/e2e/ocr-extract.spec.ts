// Mint-from-label overlay e2e (#176 P1) — exercises the overlay shell.
// Actual tesseract.js recognition is not run in CI (pulls ~6 MB from a
// CDN, accuracy on synthetic images is unreliable); the field-extraction
// logic is covered by src/registry/ocr-extract.test.ts.

import { expect, test } from "@playwright/test";
import { readFileSync } from "fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const FIXTURE_CSV = readFileSync(
  resolve(__dirname, "fixtures/registry.csv"),
  "utf-8",
);

test.beforeEach(async ({ page }) => {
  await page.route("**/registry.csv*", async (route) => {
    await route.fulfill({
      status: 200,
      headers: { "content-type": "text/csv" },
      body: FIXTURE_CSV,
    });
  });
  await page.addInitScript(() => window.localStorage.clear());
});

test.describe("Mint from label (#176 P1)", () => {
  test("Mint from label button opens the overlay", async ({ page }) => {
    await page.goto("/");
    await page.locator("nav.tabs").getByRole("button", { name: "Bind" }).click();

    const btn = page.getByRole("button", { name: /Mint from label/i });
    await expect(btn).toBeVisible();
    await btn.click();

    const overlay = page.locator(".scan-overlay--mint");
    await expect(overlay).toBeVisible({ timeout: 5_000 });
    await expect(overlay.locator(".image-scan__drop-label")).toContainText("Photograph a label");
    await expect(overlay.locator(".scan-overlay__badge")).toContainText("Mint from label");
    // Confirm is disabled until at least one field has a value.
    await expect(overlay.getByRole("button", { name: /Mint \+ bind/i })).toBeDisabled();
  });

  test("Cancel closes the overlay", async ({ page }) => {
    await page.goto("/");
    await page.locator("nav.tabs").getByRole("button", { name: "Bind" }).click();
    await page.getByRole("button", { name: /Mint from label/i }).click();

    const overlay = page.locator(".scan-overlay--mint");
    await expect(overlay).toBeVisible({ timeout: 5_000 });
    await overlay.getByRole("button", { name: /Cancel/i }).click();
    await expect(overlay).toHaveCount(0);
  });
});
