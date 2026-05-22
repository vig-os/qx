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
});
