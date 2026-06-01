// Combobox with fuzzy match + create-new (PR3). The data-quality control for
// controlled-vocabulary fields (vendor, location): operators pick an existing
// value via fuzzy search — so "Digikey" surfaces the canonical "Digi-Key"
// instead of forking a new spelling — or deliberately create a new one. A
// created value is flagged `isNew` so the caller can persist it to the
// vocabulary store (and reveal the addition in the review PR).
//
// Accessible: role=combobox/listbox, aria-expanded/activedescendant, full
// keyboard nav (↑/↓ move, Enter picks/creates, Esc closes). Closing commits
// the current text so free-typing still works if the operator never opens the
// menu.

import Fuse from "fuse.js";
import { el } from "../dom";

export interface ComboboxOptions {
  value?: string;
  /** Known values to suggest (e.g. vendor vocabulary ∪ values in use). */
  getOptions: () => string[];
  /** Fired when the value is committed. `isNew` = not among getOptions(). */
  onChange: (value: string, isNew: boolean) => void;
  /** Offer a "Create …" row for unknown text (default true). */
  allowCreate?: boolean;
  placeholder?: string;
  ariaLabel?: string;
  /** Extra class on the text input (e.g. to match table-cell styling). */
  inputClass?: string;
}

export interface ComboboxHandle {
  el: HTMLElement;
  input: HTMLInputElement;
  getValue: () => string;
  setValue: (v: string) => void;
  /** True when the current value is not among the known options. */
  isNew: () => boolean;
}

const MAX_SHOWN = 8;

export function makeCombobox(opts: ComboboxOptions): ComboboxHandle {
  const allowCreate = opts.allowCreate !== false;
  const wrap = el("div", { class: "combobox" });
  const input = document.createElement("input");
  input.type = "text";
  input.className = ["combobox__input", opts.inputClass].filter(Boolean).join(" ");
  input.autocomplete = "off";
  input.setAttribute("role", "combobox");
  input.setAttribute("aria-expanded", "false");
  input.setAttribute("aria-autocomplete", "list");
  if (opts.placeholder) input.placeholder = opts.placeholder;
  if (opts.ariaLabel) input.setAttribute("aria-label", opts.ariaLabel);
  if (opts.value) input.value = opts.value;

  const menu = el("div", { class: "combobox__menu", role: "listbox" });
  menu.style.display = "none";
  wrap.append(input, menu);

  let activeIndex = -1; // -1 = none; index into the current rendered rows
  let rows: { value: string; create: boolean; node: HTMLElement }[] = [];

  const known = () => opts.getOptions();
  const isNew = () => {
    const v = input.value.trim();
    return v !== "" && !known().some((o) => o.toLowerCase() === v.toLowerCase());
  };

  const closeMenu = () => {
    menu.style.display = "none";
    input.setAttribute("aria-expanded", "false");
    input.removeAttribute("aria-activedescendant");
    activeIndex = -1;
    rows = [];
  };

  const commit = (value: string) => {
    input.value = value;
    closeMenu();
    opts.onChange(value, isNew());
  };

  const setActive = (i: number) => {
    rows.forEach((r, idx) => r.node.classList.toggle("combobox__opt--active", idx === i));
    activeIndex = i;
    if (rows[i]) input.setAttribute("aria-activedescendant", rows[i].node.id);
    else input.removeAttribute("aria-activedescendant");
  };

  const buildMenu = () => {
    const q = input.value.trim();
    const options = known();
    const matches = q === ""
      ? options.slice()
      : new Fuse(options, { threshold: 0.4 }).search(q).map((r) => r.item);
    const shown = matches.slice(0, MAX_SHOWN);

    menu.innerHTML = "";
    rows = [];
    shown.forEach((value, i) => {
      const node = el("div", { class: "combobox__opt", role: "option", id: `cb-opt-${i}` }, value);
      node.addEventListener("mousedown", (e) => { e.preventDefault(); commit(value); });
      menu.append(node);
      rows.push({ value, create: false, node });
    });

    // Offer create when the exact value isn't already an option.
    const exact = options.some((o) => o.toLowerCase() === q.toLowerCase());
    if (allowCreate && q !== "" && !exact) {
      const node = el(
        "div",
        { class: "combobox__opt combobox__opt--create", role: "option", id: `cb-opt-${rows.length}` },
        el("span", { class: "combobox__create-badge" }, "+ new"),
        ` ${q}`,
      );
      node.addEventListener("mousedown", (e) => { e.preventDefault(); commit(q); });
      menu.append(node);
      rows.push({ value: q, create: true, node });
    }

    if (rows.length === 0) {
      menu.append(el("div", { class: "combobox__opt combobox__opt--empty muted" }, "No matches"));
      menu.style.display = "block";
      input.setAttribute("aria-expanded", "true");
      return;
    }
    menu.style.display = "block";
    input.setAttribute("aria-expanded", "true");
    setActive(0);
  };

  input.addEventListener("focus", buildMenu);
  input.addEventListener("input", buildMenu);

  input.addEventListener("keydown", (e) => {
    const open = menu.style.display !== "none";
    if (e.key === "ArrowDown") {
      e.preventDefault();
      if (!open) return buildMenu();
      setActive(Math.min(activeIndex + 1, rows.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (open) setActive(Math.max(activeIndex - 1, 0));
    } else if (e.key === "Enter") {
      if (open && rows[activeIndex]) {
        e.preventDefault();
        commit(rows[activeIndex].value);
      }
    } else if (e.key === "Escape") {
      if (open) { e.preventDefault(); closeMenu(); }
    }
  });

  // Outside click / blur commits the typed text (free-typing still works).
  input.addEventListener("blur", () => {
    // Delay so an option's mousedown fires first.
    setTimeout(() => {
      if (menu.style.display !== "none") closeMenu();
      opts.onChange(input.value.trim(), isNew());
    }, 120);
  });

  return {
    el: wrap,
    input,
    getValue: () => input.value.trim(),
    setValue: (v: string) => { input.value = v; },
    isNew,
  };
}
