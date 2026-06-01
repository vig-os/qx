// Shared data-table primitives — extracted so the Lookup and Bind tables
// stop diverging (the "elevate as a common tool" ask). Two pieces:
//
//   - makeFilterDropdown: a multi-select checkbox menu behind a labelled
//     button. Lookup uses it for Status + column filters; Bind reuses it
//     for the queue filter bar. Was private to lookup.ts.
//   - tableScroll: wraps a <table> in a horizontal-scroll container so wide
//     tables scroll instead of crushing columns into unreadability (the
//     Bind-table complaint). Lookup uses it too — fixes mobile overflow.
//
// Styling lives in style.css under the generic .filter-dd* / .data-table*
// classes (formerly .lookup__*), sized via DaisyUI tokens so controls align.

import { el, button } from "../dom";

/**
 * A multi-select filter dropdown (checkbox list behind a labelled button).
 * Mutates `selected` directly; calls `onChange` after any toggle. Returns
 * the wrapper plus a `refresh()` that re-syncs the button label + checkboxes
 * to `selected` (e.g. after a dashboard click or Clear).
 */
export function makeFilterDropdown(
  label: string,
  getOptions: () => string[],
  selected: Set<string>,
  onChange: () => void,
): { wrap: HTMLElement; refresh: () => void } {
  const wrap = el("div", { class: "filter-dd" });
  const toggle = button({ class: "outline small filter-dd-btn", type: "button" });
  const menu = el("div", { class: "filter-dd-menu" });
  menu.style.display = "none";

  const syncLabel = () => {
    toggle.textContent = "";
    toggle.append(
      `${label}`,
      selected.size > 0
        ? el("span", { class: "filter-dd-count" }, ` ${selected.size}`)
        : "",
      el("span", { class: "filter-dd-caret" }, " ▾"),
    );
    toggle.classList.toggle("filter-dd-btn--active", selected.size > 0);
  };

  const buildMenu = () => {
    menu.innerHTML = "";
    const opts = getOptions();
    if (opts.length === 0) {
      menu.append(el("p", { class: "muted small", style: "margin:4px 8px;" }, "No values"));
      return;
    }
    for (const opt of opts) {
      const row = el("label", { class: "filter-dd-opt", "data-value": opt });
      const cb = document.createElement("input");
      cb.type = "checkbox";
      cb.checked = selected.has(opt);
      cb.addEventListener("change", () => {
        if (cb.checked) selected.add(opt);
        else selected.delete(opt);
        syncLabel();
        onChange();
      });
      row.append(cb, ` ${opt}`);
      menu.append(row);
    }
  };

  toggle.addEventListener("click", (e) => {
    e.stopPropagation();
    const showing = menu.style.display !== "none";
    if (!showing) buildMenu();
    menu.style.display = showing ? "none" : "block";
  });
  document.addEventListener("click", () => { menu.style.display = "none"; });
  menu.addEventListener("click", (e) => e.stopPropagation());

  const refresh = () => {
    syncLabel();
    if (menu.style.display !== "none") buildMenu();
  };

  syncLabel();
  wrap.append(toggle, menu);
  return { wrap, refresh };
}

/**
 * Wrap a <table> in a horizontal-scroll container. Wide tables (e.g. the
 * Bind queue's 11 columns) scroll instead of squeezing every column to
 * illegibility. `maxHeight` (default true) also caps vertical height with a
 * sticky header for long tables.
 */
export function tableScroll(table: HTMLElement, opts: { maxHeight?: boolean } = {}): HTMLElement {
  const wrap = el("div", {
    class: opts.maxHeight === false ? "data-table-scroll data-table-scroll--auto" : "data-table-scroll",
  });
  wrap.append(table);
  return wrap;
}
