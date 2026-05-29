// manufacturer_id + metadata e2e tests (#171 P0) — verify the
// manufacturer ID is searchable and metadata renders as a parsed
// Properties section in the detail card.

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

test.describe("manufacturer_id + metadata (#171)", () => {
  test("fuzzy search matches on manufacturer_id", async ({ page }) => {
    await page.goto("/");

    // SN-PT-0042 is the manufacturer_id of 3456ABCDEFGHJK in the fixture
    const search = page.locator(".lookup__search");
    await search.fill("SN-PT-0042");

    const rows = page.locator(".lookup__table tbody tr");
    await expect(rows).toHaveCount(1);
    await expect(rows.first()).toHaveAttribute("data-id", "3456ABCDEFGHJK");
  });

  test("detail card shows parsed Properties from metadata JSON", async ({ page }) => {
    await page.goto("/");

    // 3456ABCDEFGHJK has metadata {"resistance_0c":100,"accuracy_class":"A"}.
    // Both keys match PT100 typeFields, so they render with their labels.
    await page.locator("tr[data-id='3456ABCDEFGHJK']").click();

    const modal = page.locator(".detail-modal-overlay");
    await expect(modal.locator(".row-detail")).toBeVisible();

    const props = modal.locator(".row-detail__properties");
    await expect(props).toBeVisible();
    await expect(props.locator("h4")).toHaveText("Properties");

    // Keys render with their typeFields labels (R₀, Class) + values.
    const dl = props.locator(".row-detail__properties-dl");
    await expect(dl).toContainText("R₀");
    await expect(dl).toContainText("100");
    await expect(dl).toContainText("Class");
    await expect(dl).toContainText("A");
  });

  test("detail card shows manufacturer_id field", async ({ page }) => {
    await page.goto("/");

    await page.locator("tr[data-id='3456ABCDEFGHJK']").click();
    const modal = page.locator(".detail-modal-overlay");
    await expect(modal.locator(".row-detail")).toBeVisible();

    // manufacturer_id appears in the flat field list
    await expect(modal.locator(".row-detail")).toContainText("SN-PT-0042");
  });

  test("part without metadata shows no Properties section", async ({ page }) => {
    await page.goto("/");

    // 89ABCDEFGHJKMN is unbound, no metadata
    await page.locator("tr[data-id='89ABCDEFGHJKMN']").click();
    const modal = page.locator(".detail-modal-overlay");
    await expect(modal.locator(".row-detail")).toBeVisible();
    await expect(modal.locator(".row-detail__properties")).toHaveCount(0);
  });

  test("metadata is not rendered as a table column", async ({ page }) => {
    await page.goto("/");
    // json-type fields are excluded from ALL_COLUMNS, so the table
    // header never shows "Metadata".
    await expect(page.locator(".lookup__table thead")).not.toContainText("Metadata");
  });
});
