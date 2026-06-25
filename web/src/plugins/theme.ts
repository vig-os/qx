// Theme plugin — toolbar toggle for light / dark mode. The CSS in
// style.css has both light + dark variable sets; this plugin only
// flips the `data-theme` attribute on <html>.
//
// Default: follow `prefers-color-scheme`. Manual override stored in
// localStorage under "qx.theme".

import type { AppContext, Plugin, PluginHost } from "../core/types";
import { icon } from "../ui/icons";

const KEY = "qx.theme";
type Theme = "light" | "dark" | "auto";

function getStored(): Theme {
  const v = localStorage.getItem(KEY);
  if (v === "light" || v === "dark" || v === "auto") return v;
  return "auto";
}

function apply(theme: Theme): void {
  const html = document.documentElement;
  if (theme === "auto") html.removeAttribute("data-theme");
  else html.setAttribute("data-theme", theme);
}

function effectiveTheme(theme: Theme): "light" | "dark" {
  if (theme === "auto") {
    return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  }
  return theme;
}

export const themePlugin: Plugin = {
  id: "theme",
  install(host: PluginHost, _ctx: AppContext) {
    let theme = getStored();
    apply(theme);

    const updateButton = (btn: HTMLButtonElement) => {
      btn.innerHTML = "";
      // Show the icon for the *target* state (the one a click will produce).
      const eff = effectiveTheme(theme);
      btn.append(icon(eff === "dark" ? "sun" : "moon"));
      btn.title = `Theme: ${theme}${theme === "auto" ? ` (currently ${eff})` : ""}. Click to cycle.`;
    };

    let btn!: HTMLButtonElement;
    host.addToolbarButton({
      id: "theme",
      label: "",
      title: "Toggle light / dark / auto",
      onClick: () => {
        // light → dark → auto → light → ...
        theme = theme === "light" ? "dark" : theme === "dark" ? "auto" : "light";
        localStorage.setItem(KEY, theme);
        apply(theme);
        if (btn) updateButton(btn);
      },
    });

    // Capture the button we just appended so we can restyle it.
    const toolbar = document.querySelector(".shell__toolbar");
    btn = toolbar?.querySelector(".toolbar-btn:last-of-type") as HTMLButtonElement;
    if (btn) {
      btn.classList.add("icon-only");
      updateButton(btn);
    }

    // React to system preference changes when in auto mode.
    window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
      if (btn) updateButton(btn);
    });
  },
};
