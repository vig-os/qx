// Tags-input multiselect (PR3). Replaces the ";"-separated free-text
// `components` field with chips picked from existing / staged-for-mint part
// IDs — so an assembly's BOM is built by selecting real IDs, not by hand-
// typing delimiters. Storage stays ";"-joined (the contract format); only the
// input changes. `formatTag` lets the chip show a grouped/pretty ID while the
// stored value stays canonical.
//
// Accessible: each chip's ✕ is a real button; the text input drives a fuzzy
// suggestion menu (↑/↓/Enter/Esc), Backspace on an empty input removes the
// last chip.

import Fuse from "fuse.js";
import { el, button } from "../dom";
import { icon } from "../icons";

export interface TagsInputOptions {
  value?: string[];
  /** Candidate values to suggest (already-selected ones are filtered out). */
  getOptions: () => string[];
  onChange: (values: string[]) => void;
  placeholder?: string;
  ariaLabel?: string;
  /** Display transform for a chip label (value stays canonical). */
  formatTag?: (v: string) => string;
  /** Allow adding a typed value not in getOptions() (default false). */
  allowCreate?: boolean;
}

export interface TagsInputHandle {
  el: HTMLElement;
  getValues: () => string[];
  setValues: (v: string[]) => void;
}

const MAX_SHOWN = 8;

export function makeTagsInput(opts: TagsInputOptions): TagsInputHandle {
  const allowCreate = opts.allowCreate === true;
  const fmt = opts.formatTag ?? ((v: string) => v);
  let values: string[] = [...(opts.value ?? [])];

  const wrap = el("div", { class: "tags-input" });
  const chips = el("div", { class: "tags-input__chips" });
  const input = document.createElement("input");
  input.type = "text";
  input.className = "tags-input__input";
  input.autocomplete = "off";
  input.setAttribute("role", "combobox");
  input.setAttribute("aria-expanded", "false");
  if (opts.placeholder) input.placeholder = opts.placeholder;
  if (opts.ariaLabel) input.setAttribute("aria-label", opts.ariaLabel);
  const menu = el("div", { class: "combobox__menu", role: "listbox" });
  menu.style.display = "none";
  wrap.append(chips, input, menu);

  let rows: { value: string; node: HTMLElement }[] = [];
  let activeIndex = -1;

  const emit = () => opts.onChange([...values]);

  const renderChips = () => {
    chips.innerHTML = "";
    values.forEach((v) => {
      const x = button({ class: "tags-input__remove", "aria-label": `Remove ${v}`, title: "Remove" }, icon("x"));
      x.addEventListener("mousedown", (e) => {
        e.preventDefault();
        values = values.filter((val) => val !== v);
        renderChips();
        emit();
      });
      chips.append(el("span", { class: "tags-input__chip" }, fmt(v), x));
    });
  };

  const closeMenu = () => {
    menu.style.display = "none";
    input.setAttribute("aria-expanded", "false");
    rows = [];
    activeIndex = -1;
  };

  const addValue = (v: string) => {
    const value = v.trim();
    if (!value || values.includes(value)) return;
    values.push(value);
    input.value = "";
    renderChips();
    emit();
    buildMenu();
  };

  const setActive = (i: number) => {
    rows.forEach((r, idx) => r.node.classList.toggle("combobox__opt--active", idx === i));
    activeIndex = i;
  };

  const buildMenu = () => {
    const q = input.value.trim();
    const available = opts.getOptions().filter((o) => !values.includes(o));
    const matches = q === ""
      ? available.slice()
      : new Fuse(available, { threshold: 0.4 }).search(q).map((r) => r.item);
    const shown = matches.slice(0, MAX_SHOWN);

    menu.innerHTML = "";
    rows = [];
    shown.forEach((value, i) => {
      const node = el("div", { class: "combobox__opt", role: "option", id: `tag-opt-${i}` }, fmt(value));
      node.addEventListener("mousedown", (e) => { e.preventDefault(); addValue(value); });
      menu.append(node);
      rows.push({ value, node });
    });
    const exact = available.some((o) => o.toLowerCase() === q.toLowerCase());
    if (allowCreate && q !== "" && !exact) {
      const node = el("div", { class: "combobox__opt combobox__opt--create", role: "option" },
        el("span", { class: "combobox__create-badge" }, "+ new"), ` ${q}`);
      node.addEventListener("mousedown", (e) => { e.preventDefault(); addValue(q); });
      menu.append(node);
      rows.push({ value: q, node });
    }

    if (rows.length === 0) { closeMenu(); return; }
    menu.style.display = "block";
    input.setAttribute("aria-expanded", "true");
    setActive(0);
  };

  input.addEventListener("focus", buildMenu);
  input.addEventListener("input", buildMenu);
  input.addEventListener("keydown", (e) => {
    const open = menu.style.display !== "none";
    if (e.key === "ArrowDown") { e.preventDefault(); if (!open) return buildMenu(); setActive(Math.min(activeIndex + 1, rows.length - 1)); }
    else if (e.key === "ArrowUp") { e.preventDefault(); if (open) setActive(Math.max(activeIndex - 1, 0)); }
    else if (e.key === "Enter") { if (open && rows[activeIndex]) { e.preventDefault(); addValue(rows[activeIndex].value); } }
    else if (e.key === "Escape") { if (open) { e.preventDefault(); closeMenu(); } }
    else if (e.key === "Backspace" && input.value === "" && values.length > 0) {
      values = values.slice(0, -1);
      renderChips();
      emit();
    }
  });
  input.addEventListener("blur", () => setTimeout(closeMenu, 120));

  renderChips();

  return {
    el: wrap,
    getValues: () => [...values],
    setValues: (v: string[]) => { values = [...v]; renderChips(); },
  };
}
