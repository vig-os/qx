// Auth onboarding e2e tests — verify the PAT modal, toolbar indicator,
// and token lifecycle work correctly.
//
// Uses route interception to mock GitHub API responses (no real token needed).

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
  // Clear session storage between tests
  await page.addInitScript(() => {
    window.sessionStorage.clear();
    window.localStorage.clear();
  });
});

test.describe("Auth onboarding modal", () => {
  test("Submit session button opens auth modal when no token is stored", async ({ page }) => {
    // Mock the GitHub user endpoint for token validation
    await page.route("https://api.github.com/user", async (route) => {
      await route.fulfill({
        status: 200,
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ login: "test-operator" }),
      });
    });

    await page.goto("/");

    // Navigate to Bind tab
    const tabBar = page.locator("nav.tabs");
    await tabBar.getByRole("button", { name: "Bind" }).click();

    // Add a row so we have something to submit
    const addBtn = page.locator(".entry-row").getByRole("button", { name: /Add row/i });
    await addBtn.click();

    // Fill the ID in the new bind row
    const queueRow = page.locator(".queue-row--bind");
    await expect(queueRow).toHaveCount(1, { timeout: 5_000 });
    const idInput = queueRow.locator(".id-cell input").first();
    await idInput.fill("ABCD-EFGH-JKMN-PQ");
    await idInput.dispatchEvent("change");

    // Click Submit — should open the auth modal (not a prompt())
    await page.getByRole("button", { name: /Submit session/i }).click();

    // Auth modal should be visible
    const modal = page.locator(".auth-modal-overlay");
    await expect(modal).toBeVisible({ timeout: 5_000 });

    // Verify modal content
    await expect(modal.locator("h3")).toContainText("Connect to GitHub");
    await expect(modal.locator("ol")).toBeVisible(); // instructions list
    await expect(modal.locator("input[type='password']")).toBeVisible(); // masked input

    // Verify security note is present
    await expect(modal.locator(".auth-modal__sec-note")).toContainText("session memory");
  });

  test("token validation shows username on valid token", async ({ page }) => {
    await page.route("https://api.github.com/user", async (route) => {
      await route.fulfill({
        status: 200,
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ login: "lars-gerchow" }),
      });
    });

    await page.goto("/");

    // Open auth modal via toolbar connect button
    const connectBtn = page.locator(".auth-indicator .icon-only").first();
    await connectBtn.click();

    const modal = page.locator(".auth-modal-overlay");
    await expect(modal).toBeVisible({ timeout: 5_000 });

    // Type a token
    const tokenInput = modal.locator("input[type='password']");
    await tokenInput.fill("github_pat_fake_test_token_1234567890");

    // Wait for validation (debounced 500ms)
    const validated = modal.locator(".auth-modal__validated");
    await expect(validated).toBeVisible({ timeout: 5_000 });
    await expect(validated).toContainText("@lars-gerchow");

    // Connect button should be enabled
    const connectModalBtn = modal.getByRole("button", { name: /Connect/i });
    await expect(connectModalBtn).toBeEnabled();
  });

  test("invalid token shows error message", async ({ page }) => {
    await page.route("https://api.github.com/user", async (route) => {
      await route.fulfill({
        status: 401,
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ message: "Bad credentials" }),
      });
    });

    await page.goto("/");

    const connectBtn = page.locator(".auth-indicator .icon-only").first();
    await connectBtn.click();

    const modal = page.locator(".auth-modal-overlay");
    await expect(modal).toBeVisible({ timeout: 5_000 });

    const tokenInput = modal.locator("input[type='password']");
    await tokenInput.fill("github_pat_invalid_token");

    // Wait for validation error
    const error = modal.locator(".auth-modal__error");
    await expect(error).toBeVisible({ timeout: 5_000 });
    await expect(error).toContainText("validation failed");
  });

  test("Escape closes the modal without connecting", async ({ page }) => {
    await page.goto("/");

    const connectBtn = page.locator(".auth-indicator .icon-only").first();
    await connectBtn.click();

    const modal = page.locator(".auth-modal-overlay");
    await expect(modal).toBeVisible({ timeout: 5_000 });

    // Press Escape
    await page.keyboard.press("Escape");
    await expect(modal).not.toBeVisible();
  });

  test("backdrop click closes the modal", async ({ page }) => {
    await page.goto("/");

    const connectBtn = page.locator(".auth-indicator .icon-only").first();
    await connectBtn.click();

    const modal = page.locator(".auth-modal-overlay");
    await expect(modal).toBeVisible({ timeout: 5_000 });

    // Click the overlay backdrop (outside the modal)
    await modal.click({ position: { x: 10, y: 10 } });
    await expect(modal).not.toBeVisible();
  });
});

test.describe("Toolbar auth indicator", () => {
  test("shows connect button when not authenticated", async ({ page }) => {
    await page.goto("/");

    const indicator = page.locator(".auth-indicator");
    await expect(indicator).toBeVisible();

    // Should have a log-in icon button
    const connectBtn = indicator.locator("button[title='Connect to GitHub']");
    await expect(connectBtn).toBeVisible();
  });

  test("shows username and disconnect after connecting", async ({ page }) => {
    await page.route("https://api.github.com/user", async (route) => {
      await route.fulfill({
        status: 200,
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ login: "test-user" }),
      });
    });

    // Pre-set token in sessionStorage
    await page.addInitScript(() => {
      window.sessionStorage.setItem("part-registry.github-pat", "github_pat_fake");
      window.sessionStorage.setItem("part-registry.github-user", "test-user");
    });

    await page.goto("/");

    const indicator = page.locator(".auth-indicator");
    await expect(indicator).toBeVisible();
    await expect(indicator).toContainText("@test-user");

    // Should have a disconnect button
    const disconnectBtn = indicator.locator("button[title*='Disconnect']");
    await expect(disconnectBtn).toBeVisible();
  });

  test("disconnect clears token and shows connect button", async ({ page }) => {
    await page.route("https://api.github.com/user", async (route) => {
      await route.fulfill({
        status: 200,
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ login: "test-user" }),
      });
    });

    await page.addInitScript(() => {
      window.sessionStorage.setItem("part-registry.github-pat", "github_pat_fake");
      window.sessionStorage.setItem("part-registry.github-user", "test-user");
    });

    await page.goto("/");

    const indicator = page.locator(".auth-indicator");
    await expect(indicator).toContainText("@test-user");

    // Click disconnect
    const disconnectBtn = indicator.locator("button[title*='Disconnect']");
    await disconnectBtn.click();

    // Should revert to connect button
    const connectBtn = indicator.locator("button[title='Connect to GitHub']");
    await expect(connectBtn).toBeVisible({ timeout: 5_000 });
    await expect(indicator).not.toContainText("@test-user");
  });
});
