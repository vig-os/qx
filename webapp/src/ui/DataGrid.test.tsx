import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { CollectionDescriptor, Entity } from "../protocol";
import { DataGrid } from "./DataGrid";

// A descriptor that shares nothing with the parts preset — proves the
// grid is generated from Describe metadata, not hardcoded field names.
const gliders: CollectionDescriptor = {
  name: "gliders",
  id: { scheme: "nano14", default: true },
  lifecycle: { statuses: ["stored", "rigged"] },
  fields: [
    { key: "wingspan", type: "string", label: "Wingspan", editable: true },
    { key: "sail_maker", type: "string", label: "Sail maker", editable: true },
  ],
  render: { label_fields: ["id"] },
};

const items: Entity[] = [
  {
    id: "GLDR2345678ABC",
    collection: "gliders",
    label: null,
    created_at: "2026-01-01T00:00:00.000Z",
    status: "rigged",
    kind: null,
    transitioned_at: { rigged: "2026-01-02T00:00:00.000Z" },
    fields: { wingspan: "15m", sail_maker: "North" },
    properties: {},
  },
];

describe("DataGrid", () => {
  it("generates columns from the descriptor's field metadata", () => {
    render(<DataGrid descriptor={gliders} items={items} />);
    // Micro-core columns render their protocol keys…
    expect(screen.getByRole("columnheader", { name: "id" })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "status" })).toBeInTheDocument();
    // …declared fields render their descriptor labels…
    expect(screen.getByRole("columnheader", { name: "Wingspan" })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "Sail maker" })).toBeInTheDocument();
    // …and cells render the entity's values for those keys.
    expect(screen.getByText("GLDR2345678ABC")).toBeInTheDocument();
    expect(screen.getByText("rigged")).toBeInTheDocument();
    expect(screen.getByText("15m")).toBeInTheDocument();
    expect(screen.getByText("North")).toBeInTheDocument();
  });
});
