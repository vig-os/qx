// Error report plugin — adds a "Report issue" toolbar button. On
// click: take a page screenshot via html2canvas-pro, copy it to the
// clipboard, and open a prefilled GitHub issue URL in a new tab. The
// user pastes the screenshot into the issue body.
//
// Why prefilled URL instead of full GitHub API: no token / OAuth
// needed, works for any signed-in GitHub user, and the screenshot is
// too large to fit in a URL query string. Clipboard + paste is the
// pragmatic spike path; a token-based path with auto-attach is a
// later upgrade.

import { ISSUE_NEW_URL } from "../config";
import type { AppContext, Plugin, PluginHost } from "../core/types";
import { icon } from "../ui/icons";
import { el } from "../ui/dom";

export const errorReportPlugin: Plugin = {
  id: "error-report",
  install(host: PluginHost, _ctx: AppContext) {
    host.addToolbarButton({
      id: "error-report",
      label: "Report",
      title: "Capture a screenshot and open a prefilled GitHub issue.",
      onClick: async () => {
        try {
          await captureAndOpenIssue(host);
        } catch (e) {
          host.toast(`Report failed: ${(e as Error).message}`, "error");
        }
      },
    });
    // Replace the rendered toolbar button content with an icon + label.
    const btn = document.querySelector<HTMLButtonElement>(
      '.shell__toolbar [data-plugin-button="error-report"]',
    );
    if (btn) {
      btn.innerHTML = "";
      btn.classList.add("icon-only");
      btn.append(icon("bug"));
    }
  },
};

function collectAppState(): string {
  const lines: string[] = [];
  try {
    const activeTab = document.querySelector(".tabs button.active")?.textContent?.trim() ?? "unknown";
    lines.push(`- Active tab: ${activeTab}`);
    const partCount = document.querySelector(".shell__status")?.textContent?.trim() ?? "unknown";
    lines.push(`- Registry: ${partCount}`);
    // Session state
    const sessionIndicator = document.querySelector(".session-indicator")?.textContent?.trim();
    if (sessionIndicator) lines.push(`- Session: ${sessionIndicator}`);
    // Queue badge
    const bindBadge = document.querySelector('.tabs button:nth-child(3) .tab-badge')?.textContent;
    if (bindBadge) lines.push(`- Bind queue: ${bindBadge} items`);
    // Contract version
    lines.push(`- Contract schema_version: ${(window as any).__contractVersion ?? "unknown"}`);
  } catch {
    lines.push("- (state collection failed)");
  }
  return lines.join("\n");
}

async function captureAndOpenIssue(host: PluginHost): Promise<void> {
  const html2canvas = (await import("html2canvas-pro")).default;
  const canvas = await html2canvas(document.body, {
    useCORS: true,
    backgroundColor: "#fff",
    logging: false,
  });
  const blob: Blob = await new Promise((resolve, reject) =>
    canvas.toBlob((b) => (b ? resolve(b) : reject(new Error("blob failed"))), "image/png"),
  );
  let copied = false;
  try {
    if (navigator.clipboard && "write" in navigator.clipboard) {
      await navigator.clipboard.write([
        new ClipboardItem({ "image/png": blob }),
      ]);
      copied = true;
    }
  } catch (e) {
    console.warn("Clipboard write failed:", e);
  }

  const ua = navigator.userAgent;
  const url = location.href;
  const body = [
    "## What happened",
    "(describe the issue)",
    "",
    "## Steps to reproduce",
    "1. ",
    "",
    "## Screenshot",
    copied
      ? "*Paste from clipboard (image already copied).*"
      : "*Attach manually — clipboard copy was not supported on this browser.*",
    "",
    "## App state",
    collectAppState(),
    "",
    "## Environment",
    `- URL: ${url}`,
    `- User agent: \`${ua}\``,
    `- Time: ${new Date().toISOString()}`,
  ].join("\n");
  const params = new URLSearchParams({
    title: "Bug: ",
    body,
    labels: "bug",
  });
  const issueUrl = `${ISSUE_NEW_URL}?${params.toString()}`;
  window.open(issueUrl, "_blank", "noopener");

  host.toast(
    copied
      ? "Screenshot copied to clipboard. Paste into the GitHub issue."
      : "Issue opened — please attach the screenshot manually.",
    "info",
  );
}
