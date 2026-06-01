// Bind tab e2e tests — route-intercepted, offline.
//
// Verifies the bind form renders editable fields from the contract,
// queue mechanics, and preflight integration.

import { expect, test } from "@playwright/test";
import { readFileSync } from "fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
import { EDITABLE_KEYS, REGISTRY_HEADER } from "./helpers/contract";

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
  // Clear the bind queue between tests.
  await page.addInitScript(() => {
    window.localStorage.removeItem("part-registry.bind-queue");
  });
});

test.describe("Bind tab", () => {
  test("renders entry controls — Add row and Scan buttons", async ({ page }) => {
    await page.goto("/");

    const tabBar = page.locator("nav.tabs");
    await tabBar.getByRole("button", { name: "Bind" }).click();

    // Entry row has "+ Add row" and "Scan" buttons
    const entryRow = page.locator(".entry-row");
    await entryRow.waitFor({ state: "visible" });
    await expect(entryRow.getByRole("button", { name: /Add row/i })).toBeVisible();
    await expect(entryRow.getByRole("button", { name: /Scan/i })).toBeVisible();
  });

  test("add row creates a bind row with editable fields", async ({ page }) => {
    await page.goto("/");

    const tabBar = page.locator("nav.tabs");
    await tabBar.getByRole("button", { name: "Bind" }).click();

    // Click "+ Add row" to create a blank bind row
    const addBtn = page.locator(".entry-row").getByRole("button", { name: /Add row/i });
    await addBtn.click();

    // A queue row should appear with editable inputs
    const queueRow = page.locator(".queue-row--bind");
    await expect(queueRow).toHaveCount(1, { timeout: 5_000 });

    // Should have ID input + editable field inputs
    const inputs = queueRow.locator("input");
    const count = await inputs.count();
    expect(count).toBeGreaterThanOrEqual(2); // ID + at least one field
  });

  test("preflight banner appears after adding a row with an ID", async ({ page }) => {
    await page.goto("/");

    const tabBar = page.locator("nav.tabs");
    await tabBar.getByRole("button", { name: "Bind" }).click();

    // Add a row and fill the ID
    const addBtn = page.locator(".entry-row").getByRole("button", { name: /Add row/i });
    await addBtn.click();

    const queueRow = page.locator(".queue-row--bind");
    await expect(queueRow).toHaveCount(1, { timeout: 5_000 });

    // Fill the ID in the new row
    const idInput = queueRow.locator(".id-cell input").first();
    await idInput.fill("2345-6789-ABCD-EF");
    await idInput.dispatchEvent("change");

    // Preflight card should render after ID is set.
    const card = page.locator(".preflight-card");
    await expect(card).toBeVisible({ timeout: 5_000 });
  });

  test("PR2: bind queue is in a horizontal-scroll container with a filter bar", async ({ page }) => {
    await page.goto("/");
    await page.locator("nav.tabs").getByRole("button", { name: "Bind" }).click();
    await page.locator(".entry-row").getByRole("button", { name: /Add row/i }).click();
    await expect(page.locator(".queue-row--bind")).toHaveCount(1, { timeout: 5_000 });

    // The wide queue table is wrapped in the shared scroll container so it
    // scrolls instead of cramming all columns (fixes the readability issue).
    const scroll = page.locator(".tab--bind .data-table-scroll");
    await expect(scroll).toBeVisible();
    await expect(scroll.locator("table.bind-queue")).toBeVisible();
    // Scroll content is wider than the visible box (the columns are legible,
    // not crushed) — proving horizontal overflow, not a squeezed table.
    const overflows = await scroll.evaluate(
      (e) => e.scrollWidth > e.clientWidth + 4,
    );
    expect(overflows).toBe(true);

    // The shared filter bar (search + Kind multi-select) is present.
    const bar = page.locator(".tab--bind .filter-bar");
    await expect(bar.locator(".queue-filter-search")).toBeVisible();
    await expect(bar.locator(".filter-dd-btn", { hasText: "Kind" })).toBeVisible();
  });

  test("PR3: vendor is a fuzzy combobox-with-create; components is a tags multiselect", async ({ page }) => {
    await page.goto("/");
    await page.locator("nav.tabs").getByRole("button", { name: "Bind" }).click();
    await page.locator(".entry-row").getByRole("button", { name: /Add row/i }).click();
    const row = page.locator(".queue-row--bind");
    await expect(row).toHaveCount(1, { timeout: 5_000 });
    await row.locator(".id-cell input").first().fill("2345-6789-ABCD-EF");

    // Vendor cell is a combobox: a typo'd query surfaces the canonical value
    // (fuzzy) AND a create-new affordance for a genuinely new vendor.
    const vendor = row.locator(".combobox").first();
    const vinput = vendor.locator(".combobox__input");
    await vinput.click();
    await vinput.fill("digi");
    await expect(vendor.locator('.combobox__opt', { hasText: "Digi-Key" })).toBeVisible();
    await expect(vendor.locator(".combobox__opt--create")).toBeVisible();
    await vendor.locator(".combobox__opt", { hasText: "Digi-Key" }).click();
    await expect(vinput).toHaveValue("Digi-Key");

    // Components cell is a tags-input: picking a known ID adds a chip.
    const tags = row.locator(".tags-input");
    await tags.locator(".tags-input__input").click();
    await tags.locator(".combobox__opt").first().click();
    await expect(tags.locator(".tags-input__chip")).toHaveCount(1);
  });

  test("PR3: edit-in-popup opens a row editor that persists back to the cell", async ({ page }) => {
    await page.goto("/");
    await page.locator("nav.tabs").getByRole("button", { name: "Bind" }).click();
    await page.locator(".entry-row").getByRole("button", { name: /Add row/i }).click();
    const row = page.locator(".queue-row--bind");
    await expect(row).toHaveCount(1, { timeout: 5_000 });
    await row.locator(".id-cell input").first().fill("2345-6789-ABCD-EF");

    await row.locator(".row-actions button[title='Edit in popup']").click();
    const editor = page.locator(".row-editor-card");
    await expect(editor).toBeVisible();
    // The description field is the first plain text input in the form.
    const desc = editor.locator(".row-editor__field input[type=text]").first();
    await desc.fill("3-wire RTD");
    await editor.getByRole("button", { name: "Save" }).click();
    await expect(editor).toBeHidden();

    // The change is persisted — reopening shows it.
    await row.locator(".row-actions button[title='Edit in popup']").click();
    await expect(
      page.locator(".row-editor-card .row-editor__field input[type=text]").first(),
    ).toHaveValue("3-wire RTD");
  });
});
