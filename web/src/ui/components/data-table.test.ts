import { describe, it, expect, beforeEach, vi } from "vitest";
import { makeFilterDropdown, tableScroll } from "./data-table";
import { el } from "../dom";

beforeEach(() => {
  document.body.innerHTML = "";
});

describe("makeFilterDropdown", () => {
  it("renders a labelled toggle and no count when nothing is selected", () => {
    const { wrap } = makeFilterDropdown("Vendor", () => ["A", "B"], new Set(), () => {});
    document.body.append(wrap);
    const btn = wrap.querySelector(".filter-dd-btn")!;
    expect(btn.textContent).toContain("Vendor");
    expect(wrap.querySelector(".filter-dd-count")).toBeNull();
  });

  it("opens on click and lists options with data-value hooks", () => {
    const { wrap } = makeFilterDropdown("Vendor", () => ["Acme", "Globex"], new Set(), () => {});
    document.body.append(wrap);
    wrap.querySelector<HTMLButtonElement>(".filter-dd-btn")!.click();
    const opts = wrap.querySelectorAll(".filter-dd-opt");
    expect(opts.length).toBe(2);
    expect(wrap.querySelector('.filter-dd-opt[data-value="Globex"]')).toBeTruthy();
  });

  it("toggling a checkbox mutates the set, fires onChange, and shows the count", () => {
    const selected = new Set<string>();
    const onChange = vi.fn();
    const { wrap } = makeFilterDropdown("Vendor", () => ["Acme", "Globex"], selected, onChange);
    document.body.append(wrap);
    wrap.querySelector<HTMLButtonElement>(".filter-dd-btn")!.click();
    const cb = wrap.querySelector<HTMLInputElement>('.filter-dd-opt[data-value="Acme"] input')!;
    cb.checked = true;
    cb.dispatchEvent(new Event("change"));
    expect(selected.has("Acme")).toBe(true);
    expect(onChange).toHaveBeenCalledOnce();
    expect(wrap.querySelector(".filter-dd-count")!.textContent).toContain("1");
  });

  it("refresh() re-syncs the count after the set is mutated externally", () => {
    const selected = new Set<string>();
    const { wrap, refresh } = makeFilterDropdown("Vendor", () => ["Acme"], selected, () => {});
    document.body.append(wrap);
    selected.add("Acme"); // e.g. a dashboard click-through mutates state directly
    expect(wrap.querySelector(".filter-dd-count")).toBeNull(); // not yet reflected
    refresh();
    expect(wrap.querySelector(".filter-dd-count")!.textContent).toContain("1");
  });

  it("shows a 'No values' hint when there are no options", () => {
    const { wrap } = makeFilterDropdown("Vendor", () => [], new Set(), () => {});
    document.body.append(wrap);
    wrap.querySelector<HTMLButtonElement>(".filter-dd-btn")!.click();
    expect(wrap.querySelector(".filter-dd-menu")!.textContent).toContain("No values");
  });
});

describe("tableScroll", () => {
  it("wraps a table in a horizontal-scroll container", () => {
    const table = el("table", {}, el("tbody", {}, el("tr", {}, el("td", {}, "x"))));
    const wrap = tableScroll(table);
    expect(wrap.classList.contains("data-table-scroll")).toBe(true);
    expect(wrap.firstElementChild).toBe(table);
  });

  it("maxHeight:false adds the --auto variant (horizontal-only scroll)", () => {
    const wrap = tableScroll(el("table", {}), { maxHeight: false });
    expect(wrap.classList.contains("data-table-scroll--auto")).toBe(true);
  });
});
