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

  test("multi-word search matches across different fields (AND of words)", async ({ page }) => {
    await page.goto("/");
    const search = page.locator(".lookup__search");
    const rows = page.locator(".lookup__table tbody tr");

    // "PT100" lives in `type`; "sensor" lives in `description` — no single
    // field contains both. Tokenized AND search must still match the two
    // PT100 sensor rows (supply + return).
    await search.fill("pt100 sensor");
    await expect(rows).toHaveCount(2);

    // A third word present only on the supply row narrows to 1.
    await search.fill("pt100 sensor supply");
    await expect(rows).toHaveCount(1);
  });

  test("status filter (multi-select dropdown) narrows to matching statuses", async ({ page }) => {
    await page.goto("/");
    const rows = page.locator(".lookup__table tbody tr");
    const statusOpt = (s: string) =>
      page.locator(`.lookup__filter-dd-opt[data-value="${s}"] input[type=checkbox]`);

    // Open the Status dropdown (stays open as boxes are toggled).
    await page.locator(".lookup__filter-dd-btn", { hasText: "Status" }).click();

    // bound only — fixture has 7 bound rows.
    await statusOpt("bound").check();
    await expect(rows).toHaveCount(7);

    // bound OR void (multi-select) — 7 + 3 = 10.
    await statusOpt("void").check();
    await expect(rows).toHaveCount(10);

    // void only — uncheck bound → 3.
    await statusOpt("bound").uncheck();
    await expect(rows).toHaveCount(3);

    // unbound only — fixture has 5 unbound rows.
    await statusOpt("void").uncheck();
    await statusOpt("unbound").check();
    await expect(rows).toHaveCount(5);
  });

  test("deep-link to /<ID> highlights that row in the grid", async ({ page }) => {
    // Navigate directly to a known bound ID.
    await page.goto("/ABCDEFGHJKMNPQ");

    // The row should be visible in the data grid.
    const row = page.locator('.lookup__table tbody tr[data-id="ABCDEFGHJKMNPQ"]');
    await expect(row).toBeVisible({ timeout: 10_000 });
  });
});
