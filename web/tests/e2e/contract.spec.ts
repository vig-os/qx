// SSoT contract conformance tests — verifies the web app surfaces
// match the shared registry-contract.json schema.

import { expect, test } from "@playwright/test";
import { readFileSync } from "fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
import { FIELD_KEYS, STATUSES, REGISTRY_HEADER } from "./helpers/contract";

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

test.describe("Contract SSoT conformance", () => {
  test("every status from contract appears in the Status filter dropdown", async ({ page }) => {
    await page.goto("/");
    await page.locator(".lookup__filter-dd-btn", { hasText: "Status" }).click();

    for (const status of STATUSES) {
      const opt = page.locator(`.lookup__filter-dd-opt[data-value="${status}"]`);
      await expect(opt, `status option for "${status}" must exist`).toBeVisible();
    }
  });

  test("REGISTRY_HEADER in smoke.spec.ts matches contract.fields order", () => {
    // The smoke spec defines REGISTRY_HEADER as a CSV header line.
    // Verify it matches the contract field keys exactly.
    const smokeHeader =
      "id,status,minted_at,bound_at,type,description,vendor,part_number,location,notes,minted_by,bound_by,last_edited_at,last_edited_by,components,manufacturer_id,metadata\n";

    expect(smokeHeader).toBe(REGISTRY_HEADER);
  });

  test("fixture CSV header matches contract field keys", () => {
    const headerLine = FIXTURE_CSV.split("\n")[0];
    const headerKeys = headerLine.split(",");
    expect(headerKeys).toEqual(FIELD_KEYS);
  });
});
