import { describe, it, expect, beforeEach, vi } from "vitest";
import { openRowEditor } from "./row-editor";
import type { FieldDef } from "../../registry/schema";
import type { AppContext } from "../../core/types";

beforeEach(() => {
  document.body.innerHTML = "";
});

const ctx = { registry: { all: () => [] } } as unknown as AppContext;

const FIELDS: FieldDef[] = [
  { key: "description", label: "Description", type: "string", editable: true } as FieldDef,
  { key: "vendor", label: "Vendor", type: "dropdown", editable: true } as FieldDef,
  { key: "components", label: "Components", type: "string", editable: true } as FieldDef,
];

describe("openRowEditor", () => {
  it("renders a modal with a field per editable field", () => {
    openRowEditor({ title: "Edit X", fields: FIELDS, values: {}, ctx, onSave: () => {} });
    const card = document.querySelector(".row-editor-card")!;
    expect(card).toBeTruthy();
    expect(card.textContent).toContain("Edit X");
    expect(document.querySelectorAll(".row-editor__field").length).toBe(3);
  });

  it("uses a combobox for vendor and a tags-input for components", () => {
    openRowEditor({ title: "Edit", fields: FIELDS, values: {}, ctx, onSave: () => {} });
    expect(document.querySelector(".row-editor__field .combobox")).toBeTruthy();
    expect(document.querySelector(".row-editor__field .tags-input")).toBeTruthy();
  });

  it("Save reports the edited values and closes", () => {
    const onSave = vi.fn();
    openRowEditor({ title: "Edit", fields: FIELDS, values: { description: "old" }, ctx, onSave });
    const descInput = document.querySelector<HTMLInputElement>(".row-editor__field input[type=text]")!;
    descInput.value = "new";
    descInput.dispatchEvent(new Event("input"));
    document.querySelector<HTMLButtonElement>(".row-editor__actions .primary")!.click();
    expect(onSave).toHaveBeenCalledOnce();
    expect(onSave.mock.calls[0][0].description).toBe("new");
    expect(document.querySelector(".row-editor-card")).toBeNull(); // closed
  });

  it("Cancel closes without saving", () => {
    const onSave = vi.fn();
    openRowEditor({ title: "Edit", fields: FIELDS, values: {}, ctx, onSave });
    document.querySelector<HTMLButtonElement>(".row-editor__actions .secondary")!.click();
    expect(onSave).not.toHaveBeenCalled();
    expect(document.querySelector(".row-editor-card")).toBeNull();
  });

  it("seeds initial values into the form", () => {
    openRowEditor({ title: "Edit", fields: FIELDS, values: { description: "seeded" }, ctx, onSave: () => {} });
    const descInput = document.querySelector<HTMLInputElement>(".row-editor__field input[type=text]")!;
    expect(descInput.value).toBe("seeded");
  });
});
