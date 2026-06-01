// Screenshot harness (not a real test — a visual-diff tool). Captures
// each tab at desktop + mobile widths so a styling migration can be
// eyeballed before/after. Run: SHOT_DIR=before npx playwright test _shots
// then again with SHOT_DIR=after, and compare the two dirs.
import { test } from "@playwright/test";
import { readFileSync } from "fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// Opt-in only: this is a manual visual-diff tool, not a CI test. Run with
// `SHOTS=1 SHOT_DIR=before npx playwright test _shots`. Skipped otherwise so
// it doesn't burn CI time writing screenshots nobody reads.
test.skip(!process.env.SHOTS, "screenshot harness — set SHOTS=1 to run");

const __dirname = dirname(fileURLToPath(import.meta.url));
const FIXTURE_CSV = readFileSync(resolve(__dirname, "fixtures/registry.csv"), "utf-8");
const DIR = process.env.SHOT_DIR ?? "before";
const WIDTHS = { desktop: { width: 1280, height: 900 }, mobile: { width: 390, height: 844 } };

test.beforeEach(async ({ page }) => {
  await page.route("**/registry.csv*", (route) =>
    route.fulfill({ status: 200, headers: { "content-type": "text/csv" }, body: FIXTURE_CSV }),
  );
  await page.addInitScript(() => window.localStorage.clear());
});

async function shot(page: import("@playwright/test").Page, name: string) {
  for (const [w, size] of Object.entries(WIDTHS)) {
    await page.setViewportSize(size);
    await page.waitForTimeout(250);
    await page.screenshot({ path: `shots/${DIR}/${name}-${w}.png`, fullPage: true });
  }
}

const TABS = ["Lookup", "Bind", "Mint", "Print"];

for (const tab of TABS) {
  test(`shot ${tab}`, async ({ page }) => {
    await page.goto("/");
    await page.locator("nav.tabs").getByRole("button", { name: tab }).click();
    await page.waitForTimeout(400);
    await shot(page, tab.toLowerCase());
  });
}

test("shot bind-with-row", async ({ page }) => {
  await page.goto("/");
  await page.locator("nav.tabs").getByRole("button", { name: "Bind" }).click();
  await page.locator(".entry-row").getByRole("button", { name: /Add row/i }).click();
  await page.waitForTimeout(300);
  await shot(page, "bind-row");
});
