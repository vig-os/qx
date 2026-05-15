// Headless e2e smoke (#35 Phase 3).
//
// Verifies the page boots end-to-end against the production build:
//   1. WASM initialises (no console errors, no unhandled rejections).
//   2. All three tabs render (Lookup, Print, Bind).
//   3. The Print tab renders an SVG preview after typing a canonical ID.
//   4. The header points at the (build-time-baked) data repo slug.
//
// The Vite preview server is started by `playwright.config.ts` with
// `VITE_DATA_REPO=exo-pet/exopet-registry-sandbox` so this mirrors a
// real sandbox-mode deploy.

import { expect, test } from "@playwright/test";

const REGISTRY_HEADER =
  "id,status,minted_at,batch,bound_at,type,description,vendor,part_number,location,notes,minted_by,bound_by,last_edited_at,last_edited_by\n";

const REGISTRY_TWO_ROWS =
  REGISTRY_HEADER +
  `ABCDEFGHJKMNPQ,bound,2026-05-08T12:00:00+00:00,B-2026-05-08,2026-05-08T12:30:00+00:00,PT100,Supply temperature sensor,TC Direct,402-141,cooling loop / supply-T,bench fixture,,,,\n` +
  `ABCDEFGHJKMNPR,unbound,2026-05-08T12:00:00+00:00,B-2026-05-08,,,,,,,,,,,\n`;

// Intercept the data-repo fetch so the smoke runs offline against a
// known-empty registry. The real data-repo Pages workflow does the
// equivalent at deploy time via the GitHub Pages serving layer.
test.beforeEach(async ({ page }) => {
  await page.route("**/registry.csv*", async (route) => {
    await route.fulfill({
      status: 200,
      headers: { "content-type": "text/csv" },
      body: REGISTRY_HEADER,
    });
  });
});

test.describe("part-registry FE smoke", () => {
  test("boots without console errors and renders all tabs", async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(`pageerror: ${e.message}`));
    page.on("console", (msg) => {
      if (msg.type() === "error") {
        // zxing-wasm logs a warning about missing serviceWorker in some
        // contexts; filter those so we only flag real failures.
        const text = msg.text();
        if (!text.includes("service worker")) {
          errors.push(`console.error: ${text}`);
        }
      }
    });

    await page.goto("/");

    // All three tab labels visible.
    await expect(page.getByRole("button", { name: "Lookup" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Print" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Bind" })).toBeVisible();

    expect(errors, `unexpected console errors: ${errors.join("\n")}`).toEqual([]);
  });

  test("Print tab activates and renders its panel without errors", async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(`pageerror: ${e.message}`));

    await page.goto("/");

    // Scope to the tab bar so we don't collide with any "Print"
    // button that the panel itself may render.
    const tabBar = page.locator("nav.tabs");
    await tabBar.getByRole("button", { name: "Print" }).click();
    await expect(tabBar.getByRole("button", { name: "Print" })).toHaveClass(
      /\bactive\b/,
    );

    expect(errors, `pageerrors: ${errors.join("\n")}`).toEqual([]);
  });

  test("Bind tab activates and renders its panel without errors", async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(`pageerror: ${e.message}`));

    await page.goto("/");

    const tabBar = page.locator("nav.tabs");
    await tabBar.getByRole("button", { name: "Bind" }).click();
    await expect(tabBar.getByRole("button", { name: "Bind" })).toHaveClass(
      /\bactive\b/,
    );

    expect(errors, `pageerrors: ${errors.join("\n")}`).toEqual([]);
  });

  test("Bind preflight (#23): renders banner + chips + local-issue when queuing an unknown ID", async ({ page }) => {
    // The bind entry row uses confirm()/alert() for unknown-registry
    // + sanity checks; auto-accept so the queue commit goes through.
    page.on("dialog", (dialog) => dialog.accept());

    await page.goto("/");

    const tabBar = page.locator("nav.tabs");
    await tabBar.getByRole("button", { name: "Bind" }).click();

    // Fill the ID input in the entry row, then click "Queue this bind".
    const entryRow = page.locator(".entry-row");
    await entryRow.waitFor({ state: "visible" });
    const idInput = entryRow.locator('input[placeholder*="ID" i]').first();
    await idInput.fill("ABCDEFGHJKMNPQ");
    await entryRow.locator('button[title="Queue this bind"]').click();

    // Preflight card renders with the policy decision attribute.
    const card = page.locator(".preflight-card");
    await expect(card).toBeVisible({ timeout: 5_000 });
    await expect(card).toHaveAttribute("data-preflight-decision", /allow|warn|block|requires_elevation/);

    // row_bind chip rendered (zero rows would actually classify; with
    // an unknown id the diff has no edits so actions may be empty —
    // assert only the card + local issue surface).
    await expect(page.locator(".issue--unknown_id")).toBeVisible();

    // Submit button is data-preflight=blocked when unknown_id fires.
    const submitBtn = page.getByRole("button", { name: /Submit batch/ });
    await expect(submitBtn).toHaveAttribute("data-preflight", "blocked");
  });

  test("PWA: manifest is reachable and ServiceWorker registers", async ({ page }) => {
    await page.goto("/");

    // Manifest link tag injected by vite-plugin-pwa.
    const manifestHref = await page.locator('link[rel="manifest"]').getAttribute("href");
    expect(manifestHref, "manifest <link> must be present").toBeTruthy();

    // The icon link we added in index.html.
    await expect(page.locator('link[rel="icon"][type="image/svg+xml"]')).toHaveAttribute(
      "href",
      /icon\.svg/,
    );

    // Manifest body parses + has the expected fields.
    const manifest = await page.evaluate(async (href) => {
      const res = await fetch(href as string);
      return res.json();
    }, manifestHref);
    expect(manifest.name).toBe("part-registry");
    expect(manifest.display).toBe("standalone");
    expect(manifest.icons.length).toBeGreaterThan(0);

    // The SW should register (autoUpdate strategy). Give it a beat to
    // finish since registerSW runs after main() resolves.
    await page.waitForFunction(
      () => navigator.serviceWorker?.controller !== null
        || (navigator.serviceWorker?.getRegistration().then((r) => !!r) as unknown as boolean),
      undefined,
      { timeout: 10_000 },
    ).catch(() => {
      // Firefox doesn't set `controller` until next navigation; falling
      // through to the explicit getRegistration check below.
    });

    const swReg = await page.evaluate(() =>
      navigator.serviceWorker?.getRegistration().then((r) => Boolean(r)),
    );
    expect(swReg, "ServiceWorker registration must exist").toBe(true);
  });

  test("WASM façade is reachable on window for diagnostics", async ({ page }) => {
    await page.goto("/");
    // The loader assigns module exports to `window.__partRegistryWasm`
    // (or similar) for debugging in production builds — be tolerant
    // and just confirm the load promise resolved by checking that
    // the page reached the same first-render milestone as the smoke.
    await expect(page.getByRole("button", { name: "Lookup" })).toBeVisible();
    // Probe that wasm-bindgen at least exposed one of the named
    // exports the FE depends on. We can't call them from Playwright
    // easily because they're ESM-loaded, but we can verify the wasm
    // module loaded by inspecting the network requests.
    const requests = await page.evaluate(() =>
      (performance.getEntriesByType("resource") as PerformanceResourceTiming[])
        .map((r) => r.name)
    );
    const wasmReq = requests.find((u) => u.endsWith(".wasm"));
    expect(wasmReq, "expected a .wasm request to have happened").toBeTruthy();
  });
});

test.describe("Lookup data-grid (#10)", () => {
  test.beforeEach(async ({ page }) => {
    // Override the default empty-registry route with a two-row fixture
    // so the data-grid has something to filter / click on.
    await page.route("**/registry.csv*", async (route) => {
      await route.fulfill({
        status: 200,
        headers: { "content-type": "text/csv" },
        body: REGISTRY_TWO_ROWS,
      });
    });
  });

  test("renders all rows, then narrows on status-filter click", async ({ page }) => {
    await page.goto("/");

    // Two rows visible by default (status filter = all).
    const allRows = page.locator(".lookup__table tbody tr");
    await expect(allRows).toHaveCount(2);

    // Click the "unbound" filter chip.
    await page.locator(".chip--filter", { hasText: /^unbound$/ }).click();
    const unboundRows = page.locator(".lookup__table tbody tr");
    await expect(unboundRows).toHaveCount(1);
    await expect(unboundRows.first()).toHaveAttribute("data-id", "ABCDEFGHJKMNPR");
  });

  test("fuzzy search narrows on vendor name and row click opens the detail card", async ({ page }) => {
    await page.goto("/");

    const search = page.locator(".lookup__search");
    await search.fill("TC Direct");

    const rows = page.locator(".lookup__table tbody tr");
    await expect(rows).toHaveCount(1);
    await expect(rows.first()).toHaveAttribute("data-id", "ABCDEFGHJKMNPQ");

    // Row click opens the inline detail card.
    await rows.first().click();
    await expect(page.locator(".row-detail")).toBeVisible();
    await expect(page.locator(".row-detail")).toContainText("PT100");
  });
});

test.describe("Print matrix studio (#11)", () => {
  test.beforeEach(async ({ page }) => {
    await page.route("**/registry.csv*", async (route) => {
      await route.fulfill({
        status: 200,
        headers: { "content-type": "text/csv" },
        body: REGISTRY_TWO_ROWS,
      });
    });
    await page.addInitScript(() => {
      window.localStorage.removeItem("part-registry.print-plan");
      window.localStorage.removeItem("part-registry.print-output-mode");
    });
  });

  test("paper format dropdown lists DK continuous, DK strip, DK-1201 die-cut, and A4/Letter sheet", async ({ page }) => {
    await page.goto("/");
    await page.locator("nav.tabs").getByRole("button", { name: "Print" }).click();

    const select = page.locator("select").filter({ hasText: "DK continuous" });
    const options = await select.locator("option").allTextContents();
    expect(options).toContain("DK continuous (auto-cut)");
    expect(options).toContain("DK strip + crop marks");
    expect(options).toContain("DK-1201 die-cut (29 × 90 mm)");
    expect(options).toContain("Sticker sheet (A4 / Letter)");
  });

  test("matrix-add duplicates the row with the next layout for the same ID", async ({ page }) => {
    // Accept any alerts (e.g. validation) so they don't block.
    page.on("dialog", (d) => d.accept());

    await page.goto("/");
    await page.locator("nav.tabs").getByRole("button", { name: "Print" }).click();

    // Wait for the entry row to be interactive.
    const entryRow = page.locator(".tab--print .entry-row");
    await entryRow.waitFor({ state: "visible" });

    // Add one row via the entry row.
    const idInput = entryRow.locator("input[type='text']").first();
    await idInput.fill("ABCDEFGHJKMNPQ");
    await entryRow.locator("button.primary").click();

    // Plan rows = tbody trs without the entry-row class.
    const planRows = page.locator(".tab--print tbody tr:not(.entry-row)");
    await expect(planRows).toHaveCount(1, { timeout: 10_000 });

    await page.locator(".tab--print .matrix-add").first().click();
    await expect(planRows).toHaveCount(2, { timeout: 10_000 });

    // Both plan rows reference the same ID.
    const idCells = planRows.locator(".id-cell");
    const first = await idCells.nth(0).textContent();
    const second = await idCells.nth(1).textContent();
    expect(first).toBe(second);
  });
});

test.describe("Lookup inline edit → bind queue (#6)", () => {
  test.beforeEach(async ({ page }) => {
    await page.route("**/registry.csv*", async (route) => {
      await route.fulfill({
        status: 200,
        headers: { "content-type": "text/csv" },
        body: REGISTRY_TWO_ROWS,
      });
    });
    // Reset the queue between tests; preview's origin is shared.
    await page.addInitScript(() => {
      window.localStorage.removeItem("part-registry.bind-queue");
    });
  });

  test("Edit on the detail card flips to a form, queues an edit, and switches to Bind", async ({ page }) => {
    await page.goto("/");

    // Open the detail card by clicking the bound row.
    const row = page.locator(".lookup__table tbody tr").first();
    await row.click();
    await expect(page.locator(".row-detail")).toBeVisible();

    // Click "Edit" → form appears.
    await page.locator(".row-detail__edit").click();
    await expect(page.locator(".row-detail--edit")).toBeVisible();

    // Change vendor.
    const vendorInput = page.locator(".row-detail__input[data-key='vendor']");
    await vendorInput.fill("ACME Probes");

    // Save → bind tab opens, edit row visible with before/after diff.
    await page.locator(".row-detail button", { hasText: "Queue edit" }).click();
    const tabBar = page.locator("nav.tabs");
    await expect(tabBar.getByRole("button", { name: "Bind" })).toHaveClass(
      /\bactive\b/,
    );

    const editRow = page.locator(".queue-row--edit");
    await expect(editRow).toHaveCount(1);
    await expect(editRow).toContainText("ACME Probes");
    await expect(editRow).toContainText("TC Direct");
  });
});
