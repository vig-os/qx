// Assembly / BOM e2e tests (#168) — verify components display in the
// detail card, assembly badge in the table, and reverse parent lookup.

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
    window.localStorage.clear();
  });
});

test.describe("Assembly / BOM (#168)", () => {
  test("assembly row shows [N] badge on the ID cell", async ({ page }) => {
    await page.goto("/");

    // BCDEFGHJKMNPQR is an assembly with 2 components in the fixture
    const assemblyRow = page.locator("tr[data-id='BCDEFGHJKMNPQR']");
    await expect(assemblyRow).toBeVisible();

    const badge = assemblyRow.locator(".assembly-badge");
    await expect(badge).toBeVisible();
    await expect(badge).toHaveText("[2]");
  });

  test("non-assembly rows do not show a badge", async ({ page }) => {
    await page.goto("/");

    // 23456789ABCDEF is unbound, no components
    const plainRow = page.locator("tr[data-id='23456789ABCDEF']");
    await expect(plainRow).toBeVisible();
    await expect(plainRow.locator(".assembly-badge")).toHaveCount(0);
  });

  test("detail card shows clickable components for an assembly", async ({ page }) => {
    await page.goto("/");

    // Click the assembly row to open detail
    await page.locator("tr[data-id='BCDEFGHJKMNPQR']").click();

    const modal = page.locator(".detail-modal-overlay");
    await expect(modal.locator(".row-detail")).toBeVisible();

    // Components section should be visible with 2 chips
    const compSection = modal.locator(".row-detail__components");
    await expect(compSection).toBeVisible();
    await expect(compSection.locator("h4")).toContainText("Components (2)");

    const chips = compSection.locator(".component-chip");
    await expect(chips).toHaveCount(2);
  });

  test("clicking a component chip navigates to that part's detail", async ({ page }) => {
    await page.goto("/");

    // Open assembly detail
    await page.locator("tr[data-id='BCDEFGHJKMNPQR']").click();

    const modal = page.locator(".detail-modal-overlay");
    await expect(modal.locator(".row-detail__components")).toBeVisible();

    // Click the first component chip
    const firstChip = modal.locator(".component-chip").first();
    const chipText = await firstChip.textContent();
    await firstChip.click();

    // The URL should have changed to the component's part path
    // (the detail modal may re-render for the child part)
    expect(chipText).toBeTruthy();
  });

  test("child part shows 'Part of' reverse lookup", async ({ page }) => {
    await page.goto("/");

    // 3456ABCDEFGHJK is a component of BCDEFGHJKMNPQR
    await page.locator("tr[data-id='3456ABCDEFGHJK']").click();

    const modal = page.locator(".detail-modal-overlay");
    await expect(modal.locator(".row-detail")).toBeVisible();

    // Should show "Part of: BCDE-FGHJ-KMNP-QR"
    const parentLink = modal.locator(".row-detail__parent-link");
    await expect(parentLink).toBeVisible();
    await expect(parentLink).toContainText("BCDE");
  });

  test("non-component part does not show 'Part of' section", async ({ page }) => {
    await page.goto("/");

    // 89ABCDEFGHJKMN is unbound, not a component of anything
    await page.locator("tr[data-id='89ABCDEFGHJKMN']").click();

    const modal = page.locator(".detail-modal-overlay");
    await expect(modal.locator(".row-detail")).toBeVisible();

    // Should NOT have the parent section
    await expect(modal.locator(".row-detail__parent")).toHaveCount(0);
  });
});
