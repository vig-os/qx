// Auth toolbar plugin — shows connection status and provides
// connect/disconnect actions via the toolbar.
//
// When connected: shows "@username" with a disconnect button.
// When disconnected: shows a "Connect" button that opens the
// PAT onboarding modal.

import type { Plugin } from "../core/types";
import { el, button } from "../ui/dom";
import { icon } from "../ui/icons";
import {
  getStoredToken,
  getStoredUser,
  clearToken,
  fetchAndCacheUser,
} from "../registry/submit";
import {
  showAuthModal,
  emitAuthStateChanged,
  AUTH_STATE_CHANGED,
} from "../ui/auth-modal";

export const authPlugin: Plugin = {
  id: "auth",
  install(host) {
    // Create a container that we can update in-place.
    const container = el("div", { class: "auth-indicator", "data-plugin-button": "auth" });

    // Render current state.
    const render = () => {
      container.innerHTML = "";
      const token = getStoredToken();
      const user = getStoredUser();

      if (token && user) {
        // Connected state
        container.classList.add("auth-indicator--connected");
        container.classList.remove("auth-indicator--disconnected");
        container.append(
          icon("user", { size: 14 }),
          el("span", { class: "auth-indicator__user", title: user }, `@${user}`),
        );
        const disconnectBtn = button(
          { class: "icon-only", title: "Disconnect (clear token)" },
          icon("log-out", { size: 14 }),
        );
        disconnectBtn.addEventListener("click", () => {
          clearToken();
          emitAuthStateChanged();
          host.toast("Disconnected from GitHub", "info");
        });
        container.append(disconnectBtn);
      } else if (token && !user) {
        // Token stored but user not yet fetched — fetch it
        container.classList.add("auth-indicator--connected");
        container.append(
          icon("user", { size: 14 }),
          el("span", { class: "muted" }, "verifying\u2026"),
        );
        void fetchAndCacheUser(token).then((u) => {
          if (u) {
            emitAuthStateChanged();
          } else {
            // Token invalid — clear it
            clearToken();
            emitAuthStateChanged();
          }
        });
      } else {
        // Disconnected state
        container.classList.remove("auth-indicator--connected");
        container.classList.add("auth-indicator--disconnected");
        const connectBtn = button(
          { class: "icon-only", title: "Connect to GitHub" },
          icon("log-in", { size: 14 }),
        );
        connectBtn.addEventListener("click", async () => {
          const result = await showAuthModal();
          if (result) {
            host.toast(`Connected as @${result.username}`, "info");
          }
        });
        container.append(connectBtn);
      }
    };

    // Initial render.
    render();

    // Re-render on auth state changes.
    window.addEventListener(AUTH_STATE_CHANGED, render);

    // Insert into toolbar. We use a raw append instead of addToolbarButton
    // because we need a container element, not a single button.
    // Find the toolbar and insert before the first existing button.
    const insertIntoToolbar = () => {
      const toolbar = document.querySelector(".shell__toolbar");
      if (toolbar) {
        // Insert as the first toolbar item (leftmost, before theme/bug/settings)
        toolbar.insertBefore(container, toolbar.firstChild);
      }
    };

    // Toolbar might not exist yet at install time — defer.
    if (document.querySelector(".shell__toolbar")) {
      insertIntoToolbar();
    } else {
      requestAnimationFrame(insertIntoToolbar);
    }
  },
};
