// PAT onboarding modal — replaces the raw prompt() for token entry.
//
// Provides step-by-step instructions for creating a fine-grained PAT,
// a password-masked input, immediate token validation via GET /user,
// and visual confirmation of the authenticated identity.
//
// The modal is shown when:
//   - The operator clicks "Submit session" without a stored token
//   - The operator clicks "Connect" in the toolbar
//   - An auth error (401/403) triggers "Re-enter token"

import { el, button, input } from "./dom";
import { icon } from "./icons";
import {
  storeToken,
  fetchAndCacheUser,
  getStoredToken,
} from "../registry/submit";
import { sendTokenToSW } from "../registry/sw-bridge";
import { DATA_REPO_SLUG } from "../config";

export interface AuthModalResult {
  token: string;
  username: string;
}

/** Event emitted when auth state changes (connect/disconnect). */
export const AUTH_STATE_CHANGED = "part-registry:auth-state-changed";

export function emitAuthStateChanged(): void {
  window.dispatchEvent(new CustomEvent(AUTH_STATE_CHANGED));
}

/**
 * Show the PAT onboarding modal. Returns the validated token + username,
 * or null if the user cancelled.
 */
export function showAuthModal(): Promise<AuthModalResult | null> {
  return new Promise((resolve) => {
    const existing = getStoredToken();

    // ---- Shared cleanup — removes overlay + Escape listener ----
    const onEsc = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    const close = () => {
      document.removeEventListener("keydown", onEsc);
      overlay.remove();
      resolve(null);
    };

    // ---- Build the modal ----
    const overlay = el("div", { class: "auth-modal-overlay" });

    const modal = el("div", {
      class: "auth-modal",
      role: "dialog",
      "aria-modal": "true",
      "aria-labelledby": "auth-modal-title",
    });

    // Close button
    const closeBtn = button(
      { class: "auth-modal__close icon-only", title: "Cancel" },
      icon("x"),
    );
    closeBtn.addEventListener("click", close);

    // Header
    const header = el(
      "div",
      { class: "auth-modal__header" },
      icon("shield", { size: 24 }),
      el("h3", { id: "auth-modal-title" }, existing ? "Update GitHub token" : "Connect to GitHub"),
    );

    // Instructions
    const repoName = DATA_REPO_SLUG.split("/").pop() ?? DATA_REPO_SLUG;
    const patUrl = `https://github.com/settings/personal-access-tokens/new`;

    const instructions = el(
      "div",
      { class: "auth-modal__instructions" },
      el(
        "p",
        {},
        "Create a ",
        el("strong", {}, "fine-grained Personal Access Token"),
        " scoped to the data repository:",
      ),
      el(
        "ol",
        {},
        el(
          "li",
          {},
          "Go to ",
          createLink(patUrl, "GitHub token settings"),
        ),
        el(
          "li",
          {},
          "Repository access: ",
          el("strong", {}, "Only select repositories"),
          ` \u2192 ${repoName}`,
        ),
        el(
          "li",
          {},
          "Permissions: ",
          el("code", {}, "Contents: Read & Write"),
          " + ",
          el("code", {}, "Pull requests: Read & Write"),
        ),
        el("li", {}, "Expiration: 90 days recommended"),
        el("li", {}, "Copy the token and paste it below"),
      ),
    );

    // Token input
    const tokenInput = input({
      type: "password",
      placeholder: "github_pat_...",
      autocomplete: "off",
      spellcheck: "false",
    }) as HTMLInputElement;
    tokenInput.style.width = "100%";
    tokenInput.style.fontFamily = "monospace";
    if (existing) {
      tokenInput.placeholder = "Leave blank to keep current token, or paste new one";
    }

    // Toggle visibility button
    const toggleBtn = button(
      { class: "auth-modal__toggle icon-only", title: "Show token" },
      icon("eye"),
    );
    toggleBtn.addEventListener("click", () => {
      const isPassword = tokenInput.type === "password";
      tokenInput.type = isPassword ? "text" : "password";
      toggleBtn.title = isPassword ? "Hide token" : "Show token";
      toggleBtn.innerHTML = "";
      toggleBtn.append(icon(isPassword ? "eye-off" : "eye"));
    });

    const inputRow = el(
      "div",
      { class: "auth-modal__input-row" },
      tokenInput,
      toggleBtn,
    );

    // Status area (shows validation result)
    const statusEl = el("div", { class: "auth-modal__status" });

    // Action buttons
    const connectBtn = button(
      { class: "primary", disabled: existing ? "" : "true" },
      icon("log-in"),
      existing ? " Update" : " Connect",
    );

    const cancelBtn = button({}, "Cancel");
    cancelBtn.addEventListener("click", close);

    const actions = el(
      "div",
      { class: "auth-modal__actions" },
      connectBtn,
      cancelBtn,
    );

    // Security note
    const secNote = el(
      "p",
      { class: "auth-modal__sec-note muted small" },
      icon("shield", { size: 12 }),
      " Token is stored in session memory only \u2014 cleared when you close this tab. ",
      "Never stored on disk or sent to any server other than api.github.com.",
    );

    // ---- Wire up validation ----

    let validatedToken: string | null = null;
    let validatedUser: string | null = null;

    const validateToken = async (token: string) => {
      statusEl.innerHTML = "";
      statusEl.append(
        el("span", { class: "auth-modal__validating muted" }, "Validating token\u2026"),
      );
      connectBtn.disabled = true;

      const username = await fetchAndCacheUser(token);
      statusEl.innerHTML = "";

      if (username) {
        validatedToken = token;
        validatedUser = username;
        statusEl.append(
          el(
            "div",
            { class: "auth-modal__validated" },
            icon("check", { size: 16 }),
            el("span", {}, ` Authenticated as `),
            el("strong", {}, `@${username}`),
          ),
        );
        connectBtn.disabled = false;
      } else {
        validatedToken = null;
        validatedUser = null;
        statusEl.append(
          el(
            "div",
            { class: "auth-modal__error" },
            "Token validation failed. Check the token and try again.",
          ),
        );
        connectBtn.disabled = true;
      }
    };

    // Validate on input change (debounced)
    let debounceTimer: ReturnType<typeof setTimeout>;
    tokenInput.addEventListener("input", () => {
      const val = tokenInput.value.trim();
      clearTimeout(debounceTimer);
      if (val.length > 10) {
        debounceTimer = setTimeout(() => void validateToken(val), 500);
      } else if (val === "" && existing) {
        // Keep existing token
        statusEl.innerHTML = "";
        statusEl.append(
          el("span", { class: "muted" }, "Current token will be kept."),
        );
        connectBtn.disabled = false;
        validatedToken = existing;
        validatedUser = null; // will be fetched on connect
      } else {
        statusEl.innerHTML = "";
        connectBtn.disabled = true;
        validatedToken = null;
        validatedUser = null;
      }
    });

    // Connect button
    connectBtn.addEventListener("click", async () => {
      const token = validatedToken ?? (tokenInput.value.trim() || existing);
      if (!token) return;

      storeToken(token);

      // If we don't have the username yet, fetch it
      if (!validatedUser) {
        const user = await fetchAndCacheUser(token);
        if (user) validatedUser = user;
      }

      // Push to SW enclave for secure storage
      await sendTokenToSW(token, validatedUser ?? "");

      emitAuthStateChanged();
      document.removeEventListener("keydown", onEsc);
      overlay.remove();
      resolve({
        token,
        username: validatedUser ?? "unknown",
      });
    });

    // Enter key submits
    tokenInput.addEventListener("keydown", (e) => {
      if (e.key === "Enter" && !connectBtn.disabled) {
        connectBtn.click();
      }
    });

    // Close on backdrop click
    overlay.addEventListener("click", (e) => {
      if (e.target === overlay) close();
    });

    // Close on Escape (listener cleaned up by close())
    document.addEventListener("keydown", onEsc);

    // ---- Assemble & show ----
    modal.append(closeBtn, header, instructions, inputRow, statusEl, actions, secNote);
    overlay.append(modal);
    document.body.append(overlay);
    tokenInput.focus();
  });
}

function createLink(href: string, text: string): HTMLAnchorElement {
  const a = el("a", {
    href,
    target: "_blank",
    rel: "noopener",
  }, text, " ", icon("external-link", { size: 12 })) as unknown as HTMLAnchorElement;
  return a;
}
