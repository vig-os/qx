// Live-API submit e2e test — exercises the full PAT → branch → commit → PR
// pipeline against the real GitHub API using the sandbox data repo.
//
// Requires PARTREG_TEST_PAT to be set (GitHub Actions secret or env var).
// Skips gracefully when absent (local dev, forks without the secret).
//
// Creates a real PR on exo-pet/exopet-registry-sandbox, then closes and
// cleans up the branch after verification.

import { expect, test } from "@playwright/test";
import { readFileSync } from "fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const PAT = process.env.PARTREG_TEST_PAT ?? "";
const DATA_REPO = "exo-pet/exopet-registry-sandbox";

const FIXTURE_CSV = readFileSync(
  resolve(__dirname, "fixtures/registry.csv"),
  "utf-8",
);

// Skip all tests in this file when no PAT is available.
test.skip(!PAT, "PARTREG_TEST_PAT not set — skipping live API tests");

test.describe("Live submit pipeline", () => {
  test.beforeEach(async ({ page }) => {
    // Intercept registry.csv so the app loads with our fixture data
    await page.route("**/registry.csv*", async (route) => {
      await route.fulfill({
        status: 200,
        headers: { "content-type": "text/csv" },
        body: FIXTURE_CSV,
      });
    });
    // Pre-inject the test PAT + a known user into sessionStorage
    await page.addInitScript(
      ([token]) => {
        window.sessionStorage.setItem("part-registry.github-pat", token);
      },
      [PAT],
    );
    // Clear any leftover session data
    await page.addInitScript(() => {
      window.localStorage.clear();
    });
  });

  test("auth modal validates token and shows username", async ({ page }) => {
    // Don't pre-inject token for this test — we want to test the modal
    await page.addInitScript(() => {
      window.sessionStorage.removeItem("part-registry.github-pat");
      window.sessionStorage.removeItem("part-registry.github-user");
    });

    await page.goto("/");

    // Open auth modal via toolbar
    const connectBtn = page.locator(".auth-indicator button[title='Connect to GitHub']");
    await expect(connectBtn).toBeVisible({ timeout: 5_000 });
    await connectBtn.click();

    const modal = page.locator(".auth-modal-overlay");
    await expect(modal).toBeVisible({ timeout: 5_000 });

    // Paste the real PAT
    const tokenInput = modal.locator("input[type='password']");
    await tokenInput.fill(PAT);

    // Wait for real GitHub API validation — should show a username
    const validated = modal.locator(".auth-modal__validated");
    await expect(validated).toBeVisible({ timeout: 15_000 });
    await expect(validated).toContainText("@");

    // Connect
    const connectModalBtn = modal.getByRole("button", { name: /Connect/i });
    await connectModalBtn.click();

    // Modal should close and toolbar should show the username
    await expect(modal).not.toBeVisible();
    const indicator = page.locator(".auth-indicator");
    await expect(indicator).toContainText("@", { timeout: 5_000 });
  });

  test("full submit creates a PR on the sandbox repo", async ({ page }) => {
    // Resolve the operator username first
    const userRes = await fetch("https://api.github.com/user", {
      headers: {
        Authorization: `Bearer ${PAT}`,
        Accept: "application/vnd.github+json",
      },
    });
    const userData = (await userRes.json()) as { login: string };
    const username = userData.login;

    // Pre-inject username too
    await page.addInitScript(
      ([user]) => {
        window.sessionStorage.setItem("part-registry.github-user", user);
      },
      [username],
    );

    await page.goto("/");

    // Navigate to Bind tab
    await page.locator("nav.tabs").getByRole("button", { name: "Bind" }).click();

    // Add a bind row with an unbound ID from the fixture
    const addBtn = page.locator(".entry-row").getByRole("button", { name: /Add row/i });
    await addBtn.click();

    const queueRow = page.locator(".queue-row--bind");
    await expect(queueRow).toHaveCount(1, { timeout: 5_000 });

    const idInput = queueRow.locator(".id-cell input").first();
    await idInput.fill("2345-6789-ABCD-EF");
    await idInput.dispatchEvent("change");

    // Wait for preflight to allow submit
    const submitBtn = page.getByRole("button", { name: /Submit session/i });
    await expect(submitBtn).toBeEnabled({ timeout: 10_000 });

    // Accept the confirmation dialog
    page.on("dialog", (d) => d.accept());

    // Click Submit — this hits the real GitHub API
    await submitBtn.click();

    // Wait for the success card with a PR link
    const successCard = page.locator(".submit-error .error-card");
    await expect(successCard).toBeVisible({ timeout: 30_000 });
    await expect(successCard).toContainText("PR created", { timeout: 5_000 });
    await expect(successCard).toContainText("PR #");

    // Extract PR number from the success message
    const cardText = await successCard.textContent();
    const prMatch = cardText?.match(/PR #(\d+)/);
    expect(prMatch, "Should show PR number in success card").toBeTruthy();
    const prNumber = parseInt(prMatch![1], 10);

    // Verify PR exists via GitHub API
    const prRes = await fetch(
      `https://api.github.com/repos/${DATA_REPO}/pulls/${prNumber}`,
      {
        headers: {
          Authorization: `Bearer ${PAT}`,
          Accept: "application/vnd.github+json",
        },
      },
    );
    expect(prRes.ok, `PR #${prNumber} should exist`).toBe(true);
    const prData = (await prRes.json()) as {
      state: string;
      head: { ref: string };
      user: { login: string };
    };
    expect(prData.state).toBe("open");
    expect(prData.head.ref).toMatch(/^registry-proposal\//);

    // Cleanup: close the PR and delete the branch
    const branchName = prData.head.ref;
    await fetch(
      `https://api.github.com/repos/${DATA_REPO}/pulls/${prNumber}`,
      {
        method: "PATCH",
        headers: {
          Authorization: `Bearer ${PAT}`,
          Accept: "application/vnd.github+json",
        },
        body: JSON.stringify({ state: "closed" }),
      },
    );
    await fetch(
      `https://api.github.com/repos/${DATA_REPO}/git/refs/heads/${branchName}`,
      {
        method: "DELETE",
        headers: {
          Authorization: `Bearer ${PAT}`,
          Accept: "application/vnd.github+json",
        },
      },
    );
  });
});
