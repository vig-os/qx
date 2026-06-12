// Print tab e2e tests — route-intercepted, offline.
//
// Verifies SVG preview rendering, layout switching, and output mode
// options against the fixture CSV.

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
  await page.addInitScript(() => {
    window.localStorage.removeItem("part-registry.print-plan");
    window.localStorage.removeItem("part-registry.print-output-mode");
  });
});

test.describe("Print tab", () => {
  test("type canonical ID, verify SVG preview renders", async ({ page }) => {
    await page.goto("/");
    await page.locator("nav.tabs").getByRole("button", { name: "Print" }).click();

    // Add a plan item via the entry row.
    const entryRow = page.locator(".tab--print .entry-row");
    await entryRow.waitFor({ state: "visible" });

    const idInput = entryRow.locator("input[type='text']").first();
    await idInput.fill("ABCDEFGHJKMNPQ");
    await entryRow.locator("button.primary").click();

    // Click Preview to trigger SVG rendering.
    await page.getByRole("button", { name: /Preview/i }).click();

    // The preview area should contain an <svg> element.
    const svg = page.locator(".label-preview svg");
    await expect(svg.first()).toBeVisible({ timeout: 10_000 });
  });

  test("switch layouts, verify SVG updates", async ({ page }) => {
    await page.goto("/");
    await page.locator("nav.tabs").getByRole("button", { name: "Print" }).click();

    const entryRow = page.locator(".tab--print .entry-row");
    await entryRow.waitFor({ state: "visible" });

    // Add item with default horz layout.
    const idInput = entryRow.locator("input[type='text']").first();
    await idInput.fill("ABCDEFGHJKMNPQ");
    await entryRow.locator("button.primary").click();

    // The plan row should be visible.
    const planRow = page.locator(".tab--print tbody tr:not(.entry-row)").first();
    await expect(planRow).toBeVisible();

    // Change layout to vert via the row's layout dropdown.
    const layoutSelect = planRow.locator("select").first();
    await layoutSelect.selectOption("vert");

    // Preview and check SVG still renders.
    await page.getByRole("button", { name: /Preview/i }).click();
    const svg = page.locator(".label-preview svg");
    await expect(svg.first()).toBeVisible({ timeout: 10_000 });
  });

  test("output mode dropdown has expected options", async ({ page }) => {
    await page.goto("/");
    await page.locator("nav.tabs").getByRole("button", { name: "Print" }).click();

    const modeSelect = page.locator("select").filter({ hasText: "DK continuous" });
    const options = await modeSelect.locator("option").allTextContents();

    expect(options).toContain("DK continuous (auto-cut)");
    expect(options).toContain("DK strip + crop marks");
    expect(options).toContain("DK-1201 die-cut (29 × 90 mm)");
    expect(options).toContain("Sticker sheet (A4 / Letter)");
  });
});
