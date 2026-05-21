// Bootstrap — wire registry, tabs, plugins. Everything else is plug-in.

import "@picocss/pico/css/pico.min.css";
import "./style.css";
import { registerSW } from "virtual:pwa-register";

import { REPO_SLUG } from "./config";
import { createRegistry } from "./registry/registry";
import type {
  AppContext,
  Plugin,
  PluginHost,
  ToolbarButtonSpec,
} from "./core/types";
import { TABS } from "./tabs";
import { PLUGINS } from "./plugins";
import { buildPartPath, parseAppPath, type AppPath } from "./routing/route";
import { el, button } from "./ui/dom";
import { icon } from "./ui/icons";
import { loadWasm } from "./wasm/loader";
import {
  loadSession,
  getSessionSync,
  migrateOldQueue,
  clearSession,
  summarizeSession,
} from "./registry/session";
import { loadPlan } from "./tabs/print";
import {
  events,
  EVENT_PLAN_CHANGED,
  EVENT_QUEUE_CHANGED,
} from "./core/events";

async function main(): Promise<void> {
  const root = document.getElementById("app");
  if (!root) throw new Error("missing #app");

  // Initialise the Rust WASM facade up front so layouts can stay
  // synchronous in their `renderSvg` interface (ADR-017 strangler-fig
  // step 8; foundation issue #33). The bundle is ~330 KB raw / ~128 KB
  // gzipped — within the 1.5 MB ceiling so blocking on boot is fine.
  await loadWasm();

  // ---- Session store init (#115, #117) ----
  // Load (or create) the session and migrate any old localStorage queue.
  const migrated = await migrateOldQueue();
  const session = await loadSession();
  if (migrated > 0) {
    console.info(`[session] Migrated ${migrated} item(s) from old bind queue.`);
  }

  // ---- Crash recovery (#117) ----
  // If the session has items from a previous page load, offer recovery.
  if (session.items.length > 0) {
    const stats = summarizeSession(session);
    const startedAt = new Date(session.createdAt).toLocaleString();
    const recovered = await showRecoveryDialog(stats, startedAt);
    if (!recovered) {
      await clearSession();
    }
  }

  const registry = createRegistry();

  // showTab is set after the tab system is wired below — but tabs
  // and plugins receive `ctx` early, so we reference a mutable holder.
  const ctxHolder: { showTab: (id: string) => void } = {
    showTab: () => {
      throw new Error("showTab called before tabs were wired");
    },
  };
  let route: AppPath = parseAppPath(window.location.pathname);
  const ctx: AppContext = {
    registry,
    showTab: (id) => ctxHolder.showTab(id),
    showPart: (id) => {
      route = { kind: "part", id };
      const nextUrl = new URL(window.location.href);
      nextUrl.pathname = buildPartPath(id, import.meta.env.BASE_URL);
      window.history.pushState({}, "", nextUrl);
    },
    getRoute: () => route,
  };

  syncCanonicalPath(route);

  const layout = renderLayout();
  root.append(layout.shell);

  installPlugins(layout.toolbar, ctx, PLUGINS);

  layout.statusBar.textContent = "Loading registry\u2026";
  try {
    await registry.load();
    const versionTag = `${__APP_VERSION__} (${__GIT_HASH__})`;
    layout.statusBar.textContent = `${registry.all().length} parts loaded.`;
    layout.statusBar.append(
      el("span", { class: "shell__version muted", title: `Built ${__BUILD_TIME__}` }, ` \u00b7 ${versionTag}`),
    );
  } catch (e) {
    layout.statusBar.textContent = `Registry load failed: ${(e as Error).message}`;
    layout.statusBar.classList.add("error");
    return;
  }

  const tabBar = el("nav", { class: "tabs" });
  const tabList = el("ul", {});
  tabBar.append(tabList);
  const panel = el("section", { class: "tab-panel" });
  layout.main.append(tabBar, panel);

  const tabEntries = new Map<string, { li: HTMLElement; btn: HTMLButtonElement }>();
  let activeTabId = route.kind === "home" ? TABS[0]?.id : "lookup";

  const showTab = async (id: string) => {
    const tab = TABS.find((t) => t.id === id);
    if (!tab) return;
    activeTabId = id;
    for (const [k, entry] of tabEntries) {
      const isActive = k === id;
      entry.li.classList.toggle("active", isActive);
      entry.btn.classList.toggle("active", isActive);
    }
    await tab.mount(panel, ctx);
  };
  ctxHolder.showTab = (id) => void showTab(id);

  for (const tab of TABS) {
    const btn = button({ class: "tab-btn" }, tab.label);
    btn.addEventListener("click", () => void showTab(tab.id));
    const li = el("li", { class: "tab-item" }, btn);
    tabEntries.set(tab.id, { li, btn });
    tabList.append(li);
  }

  // ---- Session indicator (#115) ----
  const sessionIndicator = el("div", { class: "session-indicator" });
  layout.main.insertBefore(sessionIndicator, tabBar.nextSibling);

  // Issue #97: badge counts on Bind and Print tab buttons + queue warning banner.
  const queueWarning = el("div", { class: "queue-warning" });
  layout.main.insertBefore(queueWarning, sessionIndicator.nextSibling);

  const updateBadges = () => {
    // Bind tab badge — uses session item count (bind + edit + void)
    const bindEntry = tabEntries.get("bind");
    if (bindEntry) {
      void loadSession().then((sess) => {
        const nonMintCount = sess.items.filter((i) => i.kind !== "mint").length;
        let badge = bindEntry.btn.querySelector(".tab-badge");
        if (nonMintCount > 0) {
          if (!badge) {
            badge = el("span", { class: "tab-badge" });
            bindEntry.btn.append(badge);
          }
          badge.textContent = String(nonMintCount);
        } else {
          badge?.remove();
        }
      });
    }

    // Print tab badge
    const printEntry = tabEntries.get("print");
    if (printEntry) {
      const plan = loadPlan();
      let badge = printEntry.btn.querySelector(".tab-badge");
      if (plan.length > 0) {
        if (!badge) {
          badge = el("span", { class: "tab-badge" });
          printEntry.btn.append(badge);
        }
        badge.textContent = String(plan.length);
      } else {
        badge?.remove();
      }
    }

    // Session indicator
    void loadSession().then((sess) => {
      sessionIndicator.innerHTML = "";
      if (sess.items.length === 0) {
        sessionIndicator.style.display = "none";
        return;
      }
      sessionIndicator.style.display = "";

      const stats = summarizeSession(sess);
      const indicatorText = el(
        "span",
        { class: "session-indicator__text" },
        `${stats.total} uncommitted change${stats.total > 1 ? "s" : ""}`,
      );
      const detailText = el(
        "span",
        { class: "session-indicator__detail muted small" },
        ` (${stats.label})`,
      );
      indicatorText.append(detailText);

      const submitHint = el(
        "span",
        { class: "session-indicator__hint muted small" },
        " — go to Bind tab to submit or clear",
      );

      sessionIndicator.append(indicatorText, submitHint);
      sessionIndicator.style.cursor = "pointer";
      sessionIndicator.onclick = () => void showTabWithBadges("bind");
    });

    // Queue staleness warning
    queueWarning.innerHTML = "";
    void loadSession().then((sess) => {
      if (sess.items.length === 0) return;
      const oneHourAgo = Date.now() - 60 * 60 * 1000;
      const stale = sess.items.filter((i) => {
        const ts = new Date(i.createdAt).getTime();
        return ts > 0 && ts < oneHourAgo;
      });
      if (stale.length > 0) {
        const oldest = new Date(
          Math.min(...stale.map((i) => new Date(i.createdAt).getTime())),
        );
        queueWarning.append(
          el(
            "div",
            { class: "queue-warning__banner" },
            `${stale.length} unsubmitted item(s) from ${oldest.toLocaleString()}`,
          ),
        );
      }
    });
  };

  // Update on tab switch
  const origShowTab = showTab;
  const showTabWithBadges = async (id: string) => {
    await origShowTab(id);
    updateBadges();
  };
  ctxHolder.showTab = (id) => void showTabWithBadges(id);
  for (const [tabId, entry] of tabEntries) {
    // Re-wire click handlers to use badge-aware showTab
    entry.btn.onclick = () => void showTabWithBadges(tabId);
  }

  // Update on plan/queue mutations
  events.on(EVENT_PLAN_CHANGED, updateBadges);
  events.on(EVENT_QUEUE_CHANGED, updateBadges);

  // Initial badge render
  updateBadges();

  // ---- beforeunload guard (#117) ----
  window.addEventListener("beforeunload", (e) => {
    // Synchronous check from the in-memory cache — beforeunload
    // must be synchronous to trigger the browser's leave-page dialog.
    const sess = getSessionSync();
    if (sess && sess.items.length > 0) {
      e.preventDefault();
      e.returnValue = "You have unsubmitted changes.";
    }
  });

  window.addEventListener("popstate", () => {
    route = parseAppPath(window.location.pathname);
    syncCanonicalPath(route);
    const nextTabId = route.kind === "home" ? TABS[0]?.id : "lookup";
    if (nextTabId) void showTabWithBadges(nextTabId);
  });

  if (activeTabId) await showTabWithBadges(activeTabId);
}

/**
 * Show crash recovery dialog (#117). Returns true if user chose to
 * resume, false if they chose to discard.
 */
function showRecoveryDialog(
  stats: { total: number; label: string },
  startedAt: string,
): Promise<boolean> {
  return new Promise((resolve) => {
    const overlay = el("div", { class: "recovery-overlay" });
    const dialog = el("div", { class: "recovery-dialog" });

    dialog.append(
      el("h2", {}, "Session recovered"),
      el(
        "p",
        {},
        `Recovered ${stats.total} item${stats.total > 1 ? "s" : ""} from a previous session (started ${startedAt}).`,
      ),
      el("p", { class: "muted" }, `Contents: ${stats.label}`),
    );

    const resumeBtn = button({ class: "primary" }, "Resume session");
    const discardBtn = button({ class: "destructive" }, "Discard");

    resumeBtn.addEventListener("click", () => {
      overlay.remove();
      resolve(true);
    });
    discardBtn.addEventListener("click", () => {
      overlay.remove();
      resolve(false);
    });

    dialog.append(el("div", { class: "recovery-dialog__actions" }, resumeBtn, discardBtn));
    overlay.append(dialog);
    document.body.append(overlay);
  });
}

function syncCanonicalPath(route: AppPath): void {
  if (route.kind !== "part") return;

  const canonicalPath = buildPartPath(route.id, import.meta.env.BASE_URL);
  if (window.location.pathname === canonicalPath) return;

  const nextUrl = new URL(window.location.href);
  nextUrl.pathname = canonicalPath;
  window.history.replaceState({}, "", nextUrl);
}

function renderLayout() {
  const shell = el("main", { class: "container shell" });
  const header = el("header", { class: "shell__header" });

  // Inline QR icon SVG — stylised 3x3 grid suggesting a QR code
  const qrIcon = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  qrIcon.setAttribute("viewBox", "0 0 20 20");
  qrIcon.setAttribute("width", "20");
  qrIcon.setAttribute("height", "20");
  qrIcon.setAttribute("fill", "currentColor");
  qrIcon.setAttribute("aria-hidden", "true");
  qrIcon.innerHTML = [
    '<rect x="1" y="1" width="6" height="6" rx="1"/>',
    '<rect x="13" y="1" width="6" height="6" rx="1"/>',
    '<rect x="1" y="13" width="6" height="6" rx="1"/>',
    '<rect x="9" y="9" width="2" height="2"/>',
    '<rect x="13" y="13" width="2" height="2"/>',
    '<rect x="17" y="13" width="2" height="2"/>',
    '<rect x="13" y="17" width="6" height="2"/>',
    '<rect x="9" y="13" width="2" height="6"/>',
  ].join("");

  const title = el("h1", { class: "shell__title" });
  title.append(qrIcon, "part-registry");
  const repoLink = el("a", {
    class: "shell__repo",
    href: `https://github.com/${REPO_SLUG}`,
    target: "_blank",
    rel: "noopener",
  }, REPO_SLUG);
  const toolbar = el("div", { class: "shell__toolbar" });

  // Settings gear (placeholder for future settings panel)
  const settingsBtn = button({ class: "toolbar-btn icon-only", title: "Settings" });
  settingsBtn.append(icon("settings"));
  settingsBtn.addEventListener("click", () => {
    alert("Settings panel coming soon.");
  });
  toolbar.append(settingsBtn);

  header.append(title, repoLink, toolbar);

  const main = el("section", { class: "shell__main" });
  const statusBar = el("div", { class: "shell__status muted" });

  shell.append(header, main, statusBar);
  return { shell, toolbar, main, statusBar };
}

function installPlugins(toolbar: HTMLElement, ctx: AppContext, plugins: Plugin[]): void {
  let pendingId = "";
  const host: PluginHost = {
    addToolbarButton(spec: ToolbarButtonSpec) {
      const btn = button(
        {
          class: "toolbar-btn",
          title: spec.title ?? spec.label,
          "data-plugin-button": pendingId,
        },
        spec.label,
      );
      btn.addEventListener("click", () => void spec.onClick());
      toolbar.append(btn);
      return () => btn.remove();
    },
    toast(message: string, kind: "info" | "error" = "info") {
      const t = el("div", { class: `toast toast--${kind}` }, message);
      document.body.append(t);
      setTimeout(() => t.remove(), 4000);
    },
  };
  for (const p of plugins) {
    pendingId = p.id;
    p.install(host, ctx);
  }
}

void main();

// Register the service worker (ADR-013 §"PWA installability is
// mandatory for the lab-floor UX"). `registerType: 'autoUpdate'` in
// vite.config.ts means the SW will silently fetch new bundles and
// swap on the next reload — no operator action required.
registerSW({
  immediate: true,
  onRegisteredSW(_swUrl, registration) {
    // Best-effort: re-check for updates every hour while the tab is
    // open. Workbox handles cache versioning; this just keeps long-
    // running tabs (operator on the lab floor) from getting stuck
    // on an old build.
    if (registration) {
      setInterval(() => void registration.update().catch(() => {}), 60 * 60 * 1000);
    }
  },
});
