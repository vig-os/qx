// OCR text scan e2e (#171 P2) — exercises the overlay shell. The
// actual tesseract.js recognition is not run in CI (it pulls ~6 MB of
// assets from a CDN and OCR accuracy on synthetic images is unreliable);
// the text→part matching logic is covered exhaustively by the unit
// tests in src/registry/ocr-match.test.ts.

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

test.describe("OCR text scan (#171 P2)", () => {
  test("Bind tab has a Scan text button that opens the OCR overlay", async ({ page }) => {
    await page.goto("/");
    await page.locator("nav.tabs").getByRole("button", { name: "Bind" }).click();

    const ocrBtn = page.getByRole("button", { name: /Scan text/i });
    await expect(ocrBtn).toBeVisible();
    await ocrBtn.click();

    // Overlay with the drop zone appears (tesseract is lazy — no OCR yet).
    const overlay = page.locator(".scan-overlay--ocr");
    await expect(overlay).toBeVisible({ timeout: 5_000 });
    await expect(overlay.locator(".image-scan__drop-label")).toContainText("Photograph a label");
    await expect(overlay.getByRole("button", { name: /Choose image/i })).toBeVisible();
    await expect(overlay.locator(".scan-overlay__badge")).toContainText("OCR");
  });

  test("Cancel closes the OCR overlay", async ({ page }) => {
    await page.goto("/");
    await page.locator("nav.tabs").getByRole("button", { name: "Bind" }).click();
    await page.getByRole("button", { name: /Scan text/i }).click();

    const overlay = page.locator(".scan-overlay--ocr");
    await expect(overlay).toBeVisible({ timeout: 5_000 });
    await overlay.getByRole("button", { name: /Cancel/i }).click();
    await expect(overlay).toHaveCount(0);
  });
});
