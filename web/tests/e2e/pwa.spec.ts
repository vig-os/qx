// PWA e2e tests — manifest link, ServiceWorker registration.
//
// Extracted from smoke.spec.ts and enhanced with contract-driven
// assertions.

import { expect, test } from "@playwright/test";
import { REGISTRY_HEADER } from "./helpers/contract";

test.beforeEach(async ({ page }) => {
  await page.route("**/registry.csv*", async (route) => {
    await route.fulfill({
      status: 200,
      headers: { "content-type": "text/csv" },
      body: REGISTRY_HEADER,
    });
  });
});

test.describe("PWA", () => {
  test("manifest link is present and reachable", async ({ page }) => {
    await page.goto("/");

    // Manifest link tag injected by vite-plugin-pwa.
    const manifestHref = await page
      .locator('link[rel="manifest"]')
      .getAttribute("href");
    expect(manifestHref, "manifest <link> must be present").toBeTruthy();

    // The icon link we added in index.html.
    await expect(
      page.locator('link[rel="icon"][type="image/svg+xml"]'),
    ).toHaveAttribute("href", /icon\.svg/);

    // Manifest body parses and has the expected fields.
    const manifest = await page.evaluate(async (href) => {
      const res = await fetch(href as string);
      return res.json();
    }, manifestHref);
    expect(manifest.name).toBe("qx");
    expect(manifest.display).toBe("standalone");
    expect(manifest.icons.length).toBeGreaterThan(0);
  });

  test("ServiceWorker registers", async ({ page }) => {
    await page.goto("/");

    // The SW should register (autoUpdate strategy). Give it a beat to
    // finish since registerSW runs after main() resolves.
    await page
      .waitForFunction(
        () =>
          navigator.serviceWorker?.controller !== null ||
          (navigator.serviceWorker
            ?.getRegistration()
            .then((r) => !!r) as unknown as boolean),
        undefined,
        { timeout: 10_000 },
      )
      .catch(() => {
        // Firefox doesn't set `controller` until next navigation;
        // falling through to the explicit getRegistration check below.
      });

    const swReg = await page.evaluate(() =>
      navigator.serviceWorker
        ?.getRegistration()
        .then((r) => Boolean(r)),
    );
    expect(swReg, "ServiceWorker registration must exist").toBe(true);
  });
});
