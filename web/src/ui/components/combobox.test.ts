import { describe, it, expect, beforeEach, vi } from "vitest";
import { makeCombobox } from "./combobox";

beforeEach(() => {
  document.body.innerHTML = "";
});

function mount(opts: Parameters<typeof makeCombobox>[0]) {
  const cb = makeCombobox(opts);
  document.body.append(cb.el);
  return cb;
}

function pick(node: Element) {
  node.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
}

describe("makeCombobox", () => {
  it("renders an accessible combobox input", () => {
    const cb = mount({ getOptions: () => ["Acme"], onChange: () => {} });
    expect(cb.input.getAttribute("role")).toBe("combobox");
    expect(cb.input.getAttribute("aria-expanded")).toBe("false");
  });

  it("opens the menu on focus and lists options", () => {
    const cb = mount({ getOptions: () => ["Acme", "Globex"], onChange: () => {} });
    cb.input.dispatchEvent(new FocusEvent("focus"));
    const opts = cb.el.querySelectorAll(".combobox__opt:not(.combobox__opt--create)");
    expect(opts.length).toBe(2);
    expect(cb.input.getAttribute("aria-expanded")).toBe("true");
  });

  it("fuzzy-filters as you type and surfaces a near-miss", () => {
    const cb = mount({ getOptions: () => ["Digi-Key", "Mouser", "Adafruit"], onChange: () => {} });
    cb.input.value = "digikey";
    cb.input.dispatchEvent(new Event("input"));
    const opts = [...cb.el.querySelectorAll(".combobox__opt:not(.combobox__opt--create)")].map((n) => n.textContent);
    expect(opts).toContain("Digi-Key"); // typo'd query still finds the canonical value
  });

  it("picking an existing option commits it as not-new", () => {
    const onChange = vi.fn();
    const cb = mount({ getOptions: () => ["Acme", "Globex"], onChange });
    cb.input.dispatchEvent(new FocusEvent("focus"));
    pick([...cb.el.querySelectorAll(".combobox__opt")].find((n) => n.textContent === "Globex")!);
    expect(cb.getValue()).toBe("Globex");
    expect(onChange).toHaveBeenCalledWith("Globex", false);
  });

  it("offers a create row for unknown text and flags it new", () => {
    const onChange = vi.fn();
    const cb = mount({ getOptions: () => ["Acme"], onChange });
    cb.input.value = "Keysight";
    cb.input.dispatchEvent(new Event("input"));
    const create = cb.el.querySelector(".combobox__opt--create")!;
    expect(create).toBeTruthy();
    expect(create.textContent).toContain("Keysight");
    pick(create);
    expect(cb.getValue()).toBe("Keysight");
    expect(cb.isNew()).toBe(true);
    expect(onChange).toHaveBeenCalledWith("Keysight", true);
  });

  it("does not offer create when the text exactly matches an option", () => {
    const cb = mount({ getOptions: () => ["Acme"], onChange: () => {} });
    cb.input.value = "Acme";
    cb.input.dispatchEvent(new Event("input"));
    expect(cb.el.querySelector(".combobox__opt--create")).toBeNull();
  });

  it("respects allowCreate:false", () => {
    const cb = mount({ getOptions: () => ["Acme"], onChange: () => {}, allowCreate: false });
    cb.input.value = "Zzz";
    cb.input.dispatchEvent(new Event("input"));
    expect(cb.el.querySelector(".combobox__opt--create")).toBeNull();
  });

  it("ArrowDown + Enter commits the highlighted option", () => {
    const onChange = vi.fn();
    const cb = mount({ getOptions: () => ["Acme", "Globex"], onChange });
    cb.input.dispatchEvent(new FocusEvent("focus")); // active = 0 (Acme)
    cb.input.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown" })); // -> Globex
    cb.input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
    expect(onChange).toHaveBeenCalledWith("Globex", false);
  });

  it("Escape closes the menu", () => {
    const cb = mount({ getOptions: () => ["Acme"], onChange: () => {} });
    cb.input.dispatchEvent(new FocusEvent("focus"));
    cb.input.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    expect(cb.input.getAttribute("aria-expanded")).toBe("false");
  });
});
