import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { Entity } from "../protocol";
import { partsDescribe } from "../transport/fixtures";
import { EntityDetail } from "./EntityDetail";

const descriptor = partsDescribe.collections[0];
if (!descriptor) throw new Error("parts fixture descriptor missing");

const entity: Entity = {
  id: "PQ7G2MNVX4KH9T",
  collection: "parts",
  label: null,
  created_at: "2026-05-14T09:12:00.000Z",
  status: "bound",
  kind: null,
  transitioned_at: { bound: "2026-05-20T10:01:00.000Z" },
  fields: {
    type: "t-sensor",
    vendor: "Omega",
    location: "lab-1/shelf-A",
    part_number: "TJ36-CASS",
    notes: "calibrated 2026-05",
  },
  properties: { torque_spec: "0.6 Nm" },
};

describe("EntityDetail", () => {
  it("renders a definition list generated from the descriptor", () => {
    render(<EntityDetail entity={entity} descriptor={descriptor} />);
    // Title from render.label_fields (["id", "type"]).
    expect(
      screen.getByRole("heading", { name: "PQ7G2MNVX4KH9T — t-sensor" }),
    ).toBeInTheDocument();
    // Declared-field labels come from the descriptor.
    expect(screen.getByText("Vendor")).toBeInTheDocument();
    expect(screen.getByText("Omega")).toBeInTheDocument();
    expect(screen.getByText("Part number")).toBeInTheDocument();
    expect(screen.getByText("TJ36-CASS")).toBeInTheDocument();
    // Status + lifecycle timestamps.
    expect(screen.getByText("bound")).toBeInTheDocument();
    expect(screen.getByText("2026-05-14T09:12:00.000Z")).toBeInTheDocument();
    expect(screen.getByText("transitioned_at · bound")).toBeInTheDocument();
    expect(screen.getByText("2026-05-20T10:01:00.000Z")).toBeInTheDocument();
    // Tier-3 open properties render too.
    expect(screen.getByText("torque_spec")).toBeInTheDocument();
  });
});
