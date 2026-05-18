// Lookup tab e2e tests — route-intercepted, offline.
//
// Verifies the data grid, fuzzy search, status filter chips, and
// deep-link routing against the 15-row fixture CSV.

import { expect, test } from "@playwright/test";
import { readFileSync } from "fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
import { FIELD_KEYS, STATUSES } from "./helpers/contract";

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
});

test.describe("Lookup data-grid (fixture)", () => {
  test("renders all rows from the fixture CSV", async ({ page }) => {
    await page.goto("/");

    // The fixture has 15 data rows.
    const rows = page.locator(".lookup__table tbody tr");
    await expect(rows).toHaveCount(15);
  });

  test("fuzzy search filters rows by vendor name", async ({ page }) => {
    await page.goto("/");

    const search = page.locator(".lookup__search");
    await search.fill("TC Direct");

    // TC Direct appears in 2 rows (supply-T PT100 + return-T PT100).
    const rows = page.locator(".lookup__table tbody tr");
    await expect(rows).toHaveCount(2);
  });

  test("status filter chips narrow to matching status", async ({ page }) => {
    await page.goto("/");

    // Click the "bound" filter chip — fixture has 7 bound rows.
    await page.locator(".chip--filter", { hasText: /^bound$/ }).click();
    const boundRows = page.locator(".lookup__table tbody tr");
    await expect(boundRows).toHaveCount(7);

    // Click the "void" filter chip — fixture has 3 void rows.
    await page.locator(".chip--filter", { hasText: /^void$/ }).click();
    const voidRows = page.locator(".lookup__table tbody tr");
    await expect(voidRows).toHaveCount(3);

    // Click the "unbound" filter chip — fixture has 5 unbound rows.
    await page.locator(".chip--filter", { hasText: /^unbound$/ }).click();
    const unboundRows = page.locator(".lookup__table tbody tr");
    await expect(unboundRows).toHaveCount(5);
  });

  test("deep-link to /<ID> highlights that row in the grid", async ({ page }) => {
    // Navigate directly to a known bound ID.
    await page.goto("/ABCDEFGHJKMNPQ");

    // The row should be visible in the data grid.
    const row = page.locator('.lookup__table tbody tr[data-id="ABCDEFGHJKMNPQ"]');
    await expect(row).toBeVisible({ timeout: 10_000 });
  });
});
