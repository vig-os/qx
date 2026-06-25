// typeFields-driven bind form e2e (#171 P1) — selecting a type with
// contract typeFields reveals a Properties sub-row whose values persist
// into the bind item's metadata.

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
    window.localStorage.removeItem("qx.bind-queue");
  });
});

async function addBlankBindRow(page: import("@playwright/test").Page) {
  await page.locator("nav.tabs").getByRole("button", { name: "Bind" }).click();
  const addBtn = page.locator(".entry-row").getByRole("button", { name: /Add row/i });
  await addBtn.click();
  const row = page.locator(".queue-row--bind");
  await expect(row).toHaveCount(1, { timeout: 5_000 });
  return row;
}

test.describe("typeFields bind form (#171)", () => {
  test("setting a type with typeFields reveals the Properties sub-row", async ({ page }) => {
    await page.goto("/");
    const row = await addBlankBindRow(page);

    // Inputs in the bind row: [0]=id, [1]=type, ...
    const inputs = row.locator("input");
    await inputs.nth(1).fill("PT100");
    await inputs.nth(1).dispatchEvent("change");

    const props = page.locator(".queue-row--props");
    await expect(props).toBeVisible({ timeout: 5_000 });
    await expect(props.locator(".props-editor__label")).toContainText("PT100 properties");

    // PT100 has resistance_0c, accuracy_class, wiring
    await expect(props.locator(".props-editor__field")).toHaveCount(3);
    await expect(props).toContainText("R₀");
    await expect(props).toContainText("Class");
    await expect(props).toContainText("Wiring");
  });

  test("a type with no typeFields shows no Properties sub-row", async ({ page }) => {
    await page.goto("/");
    const row = await addBlankBindRow(page);

    const inputs = row.locator("input");
    await inputs.nth(1).fill("WidgetXYZ");
    await inputs.nth(1).dispatchEvent("change");

    await expect(page.locator(".queue-row--props")).toHaveCount(0);
  });

  test("changing type swaps the Properties fields", async ({ page }) => {
    await page.goto("/");
    const row = await addBlankBindRow(page);

    const inputs = row.locator("input");
    await inputs.nth(1).fill("Fitting");
    await inputs.nth(1).dispatchEvent("change");

    const props = page.locator(".queue-row--props");
    await expect(props).toBeVisible({ timeout: 5_000 });
    // Fitting has thread_size + material
    await expect(props).toContainText("Thread");
    await expect(props).toContainText("Material");
    await expect(props.locator(".props-editor__field")).toHaveCount(2);
  });

  test("property values persist into the bind item metadata", async ({ page }) => {
    await page.goto("/");
    const row = await addBlankBindRow(page);

    const inputs = row.locator("input");
    await inputs.nth(1).fill("Cable");
    await inputs.nth(1).dispatchEvent("change");

    const props = page.locator(".queue-row--props");
    await expect(props).toBeVisible({ timeout: 5_000 });

    // Fill the wire_gauge number field
    const gauge = props.locator("input[type='number']").first();
    await gauge.fill("18");
    await gauge.dispatchEvent("change");

    // Verify it persisted to the session store (IndexedDB-backed).
    const metaStr = await page.evaluate(async () => {
      const open = indexedDB.open("qx");
      return await new Promise<string>((resolve) => {
        open.onsuccess = () => {
          const db = open.result;
          const tx = db.transaction("session", "readonly");
          const get = tx.objectStore("session").get("current");
          get.onsuccess = () => {
            const sess = get.result;
            const bind = sess?.items?.find((i: { kind: string }) => i.kind === "bind");
            resolve(bind?.fields?.metadata ?? "");
          };
          get.onerror = () => resolve("");
        };
        open.onerror = () => resolve("");
      });
    });
    expect(metaStr).toContain("wire_gauge");
    expect(metaStr).toContain("18");
  });
});
