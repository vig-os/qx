import { describe, it, expect, beforeEach, vi } from "vitest";
import { makeTagsInput } from "./tags-input";

beforeEach(() => {
  document.body.innerHTML = "";
});

function mount(opts: Parameters<typeof makeTagsInput>[0]) {
  const t = makeTagsInput(opts);
  document.body.append(t.el);
  return t;
}
const pick = (n: Element) => n.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));

describe("makeTagsInput", () => {
  it("renders initial values as chips", () => {
    const t = mount({ value: ["A", "B"], getOptions: () => [], onChange: () => {} });
    expect(t.el.querySelectorAll(".tags-input__chip").length).toBe(2);
    expect(t.getValues()).toEqual(["A", "B"]);
  });

  it("suggests only options not already selected", () => {
    const t = mount({ value: ["A"], getOptions: () => ["A", "B", "C"], onChange: () => {} });
    t.el.querySelector<HTMLInputElement>(".tags-input__input")!.dispatchEvent(new FocusEvent("focus"));
    const shown = [...t.el.querySelectorAll(".combobox__opt")].map((n) => n.textContent);
    expect(shown).toEqual(["B", "C"]);
  });

  it("picking an option adds a chip and fires onChange", () => {
    const onChange = vi.fn();
    const t = mount({ getOptions: () => ["A", "B"], onChange });
    const input = t.el.querySelector<HTMLInputElement>(".tags-input__input")!;
    input.dispatchEvent(new FocusEvent("focus"));
    pick([...t.el.querySelectorAll(".combobox__opt")].find((n) => n.textContent === "B")!);
    expect(t.getValues()).toEqual(["B"]);
    expect(onChange).toHaveBeenCalledWith(["B"]);
  });

  it("removing a chip via its ✕ updates values", () => {
    const onChange = vi.fn();
    const t = mount({ value: ["A", "B"], getOptions: () => [], onChange });
    pick(t.el.querySelector(".tags-input__chip .tags-input__remove")!);
    expect(t.getValues()).toEqual(["B"]);
    expect(onChange).toHaveBeenCalledWith(["B"]);
  });

  it("Backspace on empty input removes the last chip", () => {
    const t = mount({ value: ["A", "B"], getOptions: () => [], onChange: () => {} });
    const input = t.el.querySelector<HTMLInputElement>(".tags-input__input")!;
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Backspace" }));
    expect(t.getValues()).toEqual(["A"]);
  });

  it("does not add a duplicate value", () => {
    const t = mount({ value: ["A"], getOptions: () => ["A"], onChange: () => {} });
    const input = t.el.querySelector<HTMLInputElement>(".tags-input__input")!;
    input.dispatchEvent(new FocusEvent("focus"));
    // "A" is already selected, so it's filtered out of suggestions — nothing to pick.
    expect(t.el.querySelectorAll(".combobox__opt").length).toBe(0);
    expect(t.getValues()).toEqual(["A"]);
  });

  it("formatTag controls the chip label while the value stays canonical", () => {
    const t = mount({ value: ["ABCDEFGHJKMNPQ"], getOptions: () => [], onChange: () => {}, formatTag: (v) => v.slice(0, 4) });
    expect(t.el.querySelector(".tags-input__chip")!.textContent).toContain("ABCD");
    expect(t.getValues()).toEqual(["ABCDEFGHJKMNPQ"]);
  });

  it("allowCreate adds a typed value not in options", () => {
    const onChange = vi.fn();
    const t = mount({ getOptions: () => ["A"], onChange, allowCreate: true });
    const input = t.el.querySelector<HTMLInputElement>(".tags-input__input")!;
    input.value = "Z";
    input.dispatchEvent(new Event("input"));
    pick(t.el.querySelector(".combobox__opt--create")!);
    expect(t.getValues()).toEqual(["Z"]);
  });
});
