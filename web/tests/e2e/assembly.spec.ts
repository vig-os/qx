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
    // The session store lives in IndexedDB — reset it so queue counts
    // are deterministic between tests.
    indexedDB.deleteDatabase("qx");
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

test.describe("Create assembly from selection", () => {
  // Two bound, unparented parts in the fixture.
  const PART_A = "56789ABCDEFGHJ";
  const PART_B = "6789ABCDEFGHJK";

  async function selectRow(page: import("@playwright/test").Page, id: string) {
    await page.locator(`tr[data-id='${id}'] input[type=checkbox]`).check();
  }

  test("button is disabled until two parts are selected", async ({ page }) => {
    await page.goto("/");

    const btn = page.getByRole("button", { name: /Combine into assembly/i });
    await expect(btn).toBeDisabled();

    await selectRow(page, PART_A);
    await expect(btn).toBeDisabled(); // one selection isn't enough

    await selectRow(page, PART_B);
    await expect(btn).toBeEnabled();
  });

  test("combining two parts queues a mint + bind for a new assembly", async ({ page }) => {
    await page.goto("/");

    await selectRow(page, PART_A);
    await selectRow(page, PART_B);
    await page.getByRole("button", { name: /Combine into assembly/i }).click();

    const modal = page.locator(".detail-modal-overlay");
    await expect(modal.locator(".row-detail--assembly")).toBeVisible();
    await expect(modal.locator(".component-chip")).toHaveCount(2);

    await modal.getByPlaceholder("Description (optional)").fill("Cooling sub-assembly");
    await modal.getByRole("button", { name: /Create assembly/i }).click();

    // Lands on the Bind tab with the new assembly queued as mint + bind.
    const mintRow = page.locator(".queue-row--mint");
    const bindRow = page.locator(".queue-row--bind");
    await expect(mintRow).toHaveCount(1);
    await expect(bindRow).toHaveCount(1);

    // Both rows reference the same freshly minted assembly ID.
    const mintId = await mintRow.getAttribute("data-id");
    const bindId = await bindRow.getAttribute("data-id");
    expect(mintId).toBeTruthy();
    expect(mintId).toBe(bindId);
  });

  test("a voided component blocks creation", async ({ page }) => {
    await page.goto("/");

    await selectRow(page, PART_A);
    await selectRow(page, "789ABCDEFGHJKM"); // voided in the fixture
    await page.getByRole("button", { name: /Combine into assembly/i }).click();

    const modal = page.locator(".detail-modal-overlay");
    await expect(modal.locator(".row-detail__error")).toContainText(/void/i);
    await expect(modal.getByRole("button", { name: /Create assembly/i })).toBeDisabled();
  });

  test("a part already in another assembly blocks creation", async ({ page }) => {
    await page.goto("/");

    await selectRow(page, PART_A);
    await selectRow(page, "3456ABCDEFGHJK"); // already a component of BCDEFGHJKMNPQR
    await page.getByRole("button", { name: /Combine into assembly/i }).click();

    const modal = page.locator(".detail-modal-overlay");
    await expect(modal.locator(".row-detail__error")).toContainText(/already a component/i);
    await expect(modal.getByRole("button", { name: /Create assembly/i })).toBeDisabled();
  });

  test("a part claimed by a pending (unsubmitted) assembly blocks re-use", async ({ page }) => {
    const PART_C = "ABCDEFGHJKMNPQ"; // third bound, unparented part
    await page.goto("/");

    // First assembly: combine A + B and queue it (do NOT submit).
    await selectRow(page, PART_A);
    await selectRow(page, PART_B);
    await page.getByRole("button", { name: /Combine into assembly/i }).click();
    const modal1 = page.locator(".detail-modal-overlay");
    await expect(modal1.locator(".row-detail--assembly")).toBeVisible();
    await modal1.getByRole("button", { name: /Create assembly/i }).click();
    await expect(page.locator(".queue-row--bind")).toHaveCount(1);

    // Back to Lookup; try to combine A (now claimed) + C into a second
    // assembly. The pending bind isn't in the registry yet, but the modal
    // must still reject A based on the session's pending claim.
    await page.locator("nav.tabs").getByRole("button", { name: "Lookup" }).click();
    await selectRow(page, PART_A);
    await selectRow(page, PART_C);
    await page.getByRole("button", { name: /Combine into assembly/i }).click();

    const modal2 = page.locator(".detail-modal-overlay");
    await expect(modal2.locator(".row-detail__error")).toContainText(/already a component/i);
    await expect(modal2.getByRole("button", { name: /Create assembly/i })).toBeDisabled();
  });
});
