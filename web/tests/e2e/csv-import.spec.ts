// Bulk CSV/TSV import e2e (#176 P0) — paste tabular data, map columns,
// commit, and verify mint+bind rows land in the session queue. The
// parse/map/classify logic is exhaustively unit-tested in
// src/registry/csv-import.test.ts; this covers the modal + queue wiring.

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

async function openImport(page: import("@playwright/test").Page) {
  await page.locator("nav.tabs").getByRole("button", { name: "Bind" }).click();
  await page.getByRole("button", { name: /Import list/i }).click();
  const modal = page.locator(".import-modal-overlay");
  await expect(modal).toBeVisible({ timeout: 5_000 });
  return modal;
}

test.describe("Bulk CSV import (#176 P0)", () => {
  test("Import list button opens the modal", async ({ page }) => {
    await page.goto("/");
    const modal = await openImport(page);
    await expect(modal.locator("h3")).toContainText("Import parts");
    await expect(modal.locator(".import-modal__textarea")).toBeVisible();
  });

  test("paste TSV → auto-mapped columns → commit mints+binds into queue", async ({ page }) => {
    await page.goto("/");
    const modal = await openImport(page);

    // Paste a 2-row TSV with recognizable headers.
    const tsv = "vendor\tpart_number\tlocation\nOmega\t402-141\tLab-1\nSwagelok\tSS-400-1-4\tLab-2";
    await modal.locator(".import-modal__textarea").fill(tsv);
    await modal.getByRole("button", { name: /^Parse$/ }).click();

    // Mapping table appears with auto-detected targets.
    const mapTable = modal.locator(".import-map");
    await expect(mapTable).toBeVisible();
    const selects = mapTable.locator("select");
    await expect(selects).toHaveCount(3);
    await expect(selects.nth(0)).toHaveValue("field:vendor");
    await expect(selects.nth(1)).toHaveValue("field:part_number");
    await expect(selects.nth(2)).toHaveValue("field:location");

    // Summary shows 2 new (mint+bind).
    await expect(modal.locator(".import-modal__summary")).toContainText("2 new");

    // Commit → modal closes, queue shows 2 mint rows + 2 bind rows.
    await modal.getByRole("button", { name: /Add to queue/i }).click();
    await expect(modal).toHaveCount(0);

    await expect(page.locator(".queue-row--mint")).toHaveCount(2, { timeout: 5_000 });
    await expect(page.locator(".queue-row--bind")).toHaveCount(2);

    // Regression (#176): a bind queued in the same session as its mint
    // must NOT be flagged unknown_id / block submit. The freshly-minted
    // IDs aren't in the loaded registry, but the preflight treats pending
    // mints as known, so Submit stays enabled.
    await expect(page.getByRole("button", { name: /Submit session/i })).toBeEnabled();
    await expect(page.locator(".preflight-card")).not.toContainText("unknown_id");
  });

  test("a valid canonical ID column produces bind-only rows", async ({ page }) => {
    await page.goto("/");
    const modal = await openImport(page);

    // 89ABCDEFGHJKMN is a valid 14-char ID; vendor is metadata.
    const tsv = "id\tvendor\n89ABCDEFGHJKMN\tOmega";
    await modal.locator(".import-modal__textarea").fill(tsv);
    await modal.getByRole("button", { name: /^Parse$/ }).click();

    await expect(modal.locator(".import-map select").nth(0)).toHaveValue("field:id");
    await expect(modal.locator(".import-modal__summary")).toContainText("bind-only");

    await modal.getByRole("button", { name: /Add to queue/i }).click();
    await expect(modal).toHaveCount(0);

    // Bind-only: 1 bind row, 0 mint rows.
    await expect(page.locator(".queue-row--bind")).toHaveCount(1, { timeout: 5_000 });
    await expect(page.locator(".queue-row--mint")).toHaveCount(0);
  });

  test("unmapped-only data disables commit", async ({ page }) => {
    await page.goto("/");
    const modal = await openImport(page);

    await modal.locator(".import-modal__textarea").fill("foo\tbar\n1\t2");
    await modal.getByRole("button", { name: /^Parse$/ }).click();

    // Both headers unrecognized → all "ignore" → warn + disabled commit.
    await expect(modal.locator(".import-modal__warn")).toBeVisible();
    await expect(modal.getByRole("button", { name: /Add to queue/i })).toBeDisabled();
  });

  test("a mixed batch (one existing ID + one new) commits 1 bind + 1 mint", async ({ page }) => {
    await page.goto("/");
    const modal = await openImport(page);

    // Row 1: existing canonical ID → bind-only. Row 2: no ID → mint+bind.
    const tsv = "id\tvendor\n89ABCDEFGHJKMN\tOmega\n\tSwagelok";
    await modal.locator(".import-modal__textarea").fill(tsv);
    await modal.getByRole("button", { name: /^Parse$/ }).click();
    await expect(modal.locator(".import-modal__summary")).toContainText("1 new");
    await expect(modal.locator(".import-modal__summary")).toContainText("bind-only");

    await modal.getByRole("button", { name: /Add to queue/i }).click();
    await expect(modal).toHaveCount(0);
    await expect(page.locator(".queue-row--mint")).toHaveCount(1, { timeout: 5_000 });
    await expect(page.locator(".queue-row--bind")).toHaveCount(2); // bind-only + minted bind
  });

  test("changing a mapping dropdown re-classifies the summary", async ({ page }) => {
    await page.goto("/");
    const modal = await openImport(page);

    // Header "ref" is unrecognized → ignore; map it to id and watch the
    // summary flip from mint to bind-only.
    await modal.locator(".import-modal__textarea").fill("ref\tvendor\n89ABCDEFGHJKMN\tOmega");
    await modal.getByRole("button", { name: /^Parse$/ }).click();
    await expect(modal.locator(".import-modal__summary")).toContainText("1 new");

    await modal.locator(".import-map select").nth(0).selectOption("field:id");
    await expect(modal.locator(".import-modal__summary")).toContainText("bind-only");
  });

  test("ragged rows surface a warning", async ({ page }) => {
    await page.goto("/");
    const modal = await openImport(page);

    // Row has an extra column vs the 2-col header.
    await modal.locator(".import-modal__textarea").fill("vendor\tloc\nOmega\tLab-1\tEXTRA");
    await modal.getByRole("button", { name: /^Parse$/ }).click();
    await expect(modal.locator(".import-modal__warn")).toContainText("different column count");
  });

  test("Back returns to the paste step", async ({ page }) => {
    await page.goto("/");
    const modal = await openImport(page);

    await modal.locator(".import-modal__textarea").fill("vendor\nOmega");
    await modal.getByRole("button", { name: /^Parse$/ }).click();
    await expect(modal.locator(".import-map")).toBeVisible();

    await modal.getByRole("button", { name: /^Back$/ }).click();
    await expect(modal.locator(".import-modal__textarea")).toBeVisible();
    // Back toggles the mapping step hidden (display:none), not removed.
    await expect(modal.locator(".import-map")).not.toBeVisible();
  });

  test("file upload path parses and maps", async ({ page }) => {
    await page.goto("/");
    const modal = await openImport(page);

    // Drive the hidden file input directly with an in-memory CSV.
    await modal.locator("input[type='file']").setInputFiles({
      name: "parts.csv",
      mimeType: "text/csv",
      buffer: Buffer.from("vendor,part_number\nOmega,402-141\n"),
    });

    await expect(modal.locator(".import-map")).toBeVisible({ timeout: 5_000 });
    await expect(modal.locator(".import-map select").nth(0)).toHaveValue("field:vendor");
    await expect(modal.locator(".import-map select").nth(1)).toHaveValue("field:part_number");
  });
});
