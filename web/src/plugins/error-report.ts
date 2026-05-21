// Error report plugin — adds a "Report issue" toolbar button.
//
// On click: capture screenshot via html2canvas-pro, encode app state
// (session + print plan) as a reproducible base64 hash, copy screenshot
// to clipboard, and open a prefilled GitHub issue URL with full context.
//
// Inspired by hyrr's BugReportModal pattern: structured context capture
// with reproducible config URL and screenshot.

import { ISSUE_NEW_URL } from "../config";
import type { AppContext, Plugin, PluginHost } from "../core/types";
import { icon } from "../ui/icons";
import { loadPlan } from "../tabs/print";

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

/** Collect app state as structured text for the bug report. */
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
    // Print plan
    const plan = loadPlan();
    if (plan.length > 0) lines.push(`- Print plan: ${plan.length} item(s)`);
    // Contract version
    lines.push(`- Contract schema_version: ${(window as any).__contractVersion ?? "unknown"}`);
    // Label settings
    const codeType = localStorage.getItem("part-registry.label.codeType") ??
      sessionStorage.getItem("part-registry.label.codeType") ?? "standard";
    const fmt = localStorage.getItem("part-registry.label.format") ??
      sessionStorage.getItem("part-registry.label.format") ?? "auto";
    lines.push(`- Label settings: code=${codeType}, format=${fmt}`);
  } catch {
    lines.push("- (state collection failed)");
  }
  return lines.join("\n");
}

/**
 * Encode session + print plan as a compact base64 state string for
 * reproducible bug reports. The state can be used to restore the exact
 * app configuration that triggered the issue.
 */
function encodeReproState(): string {
  try {
    const state: Record<string, unknown> = {};

    // Session items (from IndexedDB are async — use localStorage fallback snapshot)
    const sessionRaw = localStorage.getItem("part-registry.session");
    if (sessionRaw) {
      try {
        const sess = JSON.parse(sessionRaw);
        if (sess?.items?.length > 0) {
          state.session = sess.items.slice(0, 20); // cap at 20 for URL length
        }
      } catch { /* ignore parse errors */ }
    }

    // Print plan
    const plan = loadPlan();
    if (plan.length > 0) {
      state.plan = plan.slice(0, 10); // cap at 10
    }

    // Label settings
    state.labelSettings = {
      codeType: localStorage.getItem("part-registry.label.codeType") ?? "standard",
      format: localStorage.getItem("part-registry.label.format") ?? "auto",
      showText: localStorage.getItem("part-registry.label.showText") ?? "true",
    };

    if (Object.keys(state).length === 0) return "";
    return btoa(JSON.stringify(state));
  } catch {
    return "";
  }
}

async function captureAndOpenIssue(host: PluginHost): Promise<void> {
  host.toast("Capturing screenshot...", "info");

  const html2canvas = (await import("html2canvas-pro")).default;
  const canvas = await html2canvas(document.body, {
    useCORS: true,
    backgroundColor: "#fff",
    logging: false,
    scale: Math.min(window.devicePixelRatio, 2), // cap at 2x
    width: Math.min(document.body.scrollWidth, 1280),
  });

  // Downscale to JPEG for smaller payload
  const screenshotDataUrl = canvas.toDataURL("image/jpeg", 0.8);

  // Also copy as PNG to clipboard for paste
  const blob: Blob | null = await new Promise((resolve) =>
    canvas.toBlob((b) => resolve(b), "image/png"),
  );
  let copied = false;
  if (blob) {
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
  }

  const reproState = encodeReproState();
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
    "",
    // Embed screenshot as base64 image in the issue body.
    // GitHub renders this inline. Falls back to clipboard paste if too large.
    screenshotDataUrl.length < 65000
      ? `![screenshot](${screenshotDataUrl})`
      : (copied
        ? "*Screenshot too large for inline embed. Paste from clipboard (image already copied).*"
        : "*Screenshot too large for inline embed. Please attach manually.*"),
    "",
    "## App state",
    collectAppState(),
    "",
    "## Reproducible state",
    reproState
      ? `<details><summary>Expand state (base64)</summary>\n\n\`\`\`\n${reproState}\n\`\`\`\n</details>`
      : "- (no state captured)",
    "",
    "## Environment",
    `- URL: \`${url}\``,
    `- User agent: \`${ua}\``,
    `- Time: ${new Date().toISOString()}`,
  ].join("\n");

  // GitHub issue URL has a ~8000 char limit on query strings.
  // If the body is too long (due to screenshot), truncate the screenshot.
  let finalBody = body;
  const maxUrlLen = 7500;
  if (finalBody.length > maxUrlLen) {
    finalBody = finalBody.replace(
      /!\[screenshot\]\(data:image\/jpeg;base64,[^)]+\)/,
      copied
        ? "*Screenshot too large for URL. Paste from clipboard (image already copied).*"
        : "*Screenshot too large for URL. Please attach manually.*",
    );
  }

  const params = new URLSearchParams({
    title: "Bug: ",
    body: finalBody,
    labels: "bug",
  });
  const issueUrl = `${ISSUE_NEW_URL}?${params.toString()}`;
  window.open(issueUrl, "_blank", "noopener");

  host.toast(
    copied
      ? "Screenshot copied to clipboard. Paste into the GitHub issue if not embedded."
      : "Issue opened — please attach the screenshot manually.",
    "info",
  );
}
