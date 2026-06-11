// Small parts fixture set for the mock transport (dev + tests).
//
// Field vocabulary follows the parts preset (ADR-035 / ADR-012,
// cf. schema/registry-contract.json); ids are nano14 (alphabet
// 23456789ABCDEFGHJKMNPQRSTUVWXYZ, length 14). All parts of one mint
// event share one created_at stamp (ADR-035 §1b).

import type { DescribeData, Entity } from "../protocol";

export interface Fixtures {
  describe: DescribeData;
  entities: Entity[];
}

export const partsDescribe: DescribeData = {
  name: "mock-registry",
  collections: [
    {
      name: "parts",
      id: { scheme: "nano14", default: true },
      lifecycle: { statuses: ["unbound", "bound", "void"] },
      fields: [
        { key: "type", type: "string", label: "Type", editable: true, meaningful_from: "bound" },
        { key: "vendor", type: "string", label: "Vendor", editable: true, meaningful_from: "bound" },
        { key: "location", type: "string", label: "Location", editable: true, meaningful_from: "bound" },
        { key: "part_number", type: "string", label: "Part number", editable: true, meaningful_from: "bound" },
        { key: "notes", type: "string", label: "Notes", editable: true },
      ],
      render: { label_fields: ["id", "type"] },
    },
  ],
};

const MINT_1 = "2026-05-14T09:12:00.000Z";
const MINT_2 = "2026-06-02T14:30:00.000Z";

function part(
  id: string,
  status: string,
  created_at: string,
  fields: Record<string, string>,
  transitioned_at: Record<string, string> = {},
): Entity {
  return {
    id,
    collection: "parts",
    label: null,
    created_at,
    status,
    kind: null,
    transitioned_at,
    fields,
    properties: {},
  };
}

export const partsEntities: Entity[] = [
  part(
    "PQ7G2MNVX4KH9T",
    "bound",
    MINT_1,
    {
      type: "t-sensor",
      vendor: "Omega",
      location: "lab-1/shelf-A",
      part_number: "TJ36-CASS",
      notes: "calibrated 2026-05",
    },
    { bound: "2026-05-20T10:01:00.000Z" },
  ),
  part(
    "W3JD8RST2UVKXM",
    "bound",
    MINT_1,
    {
      type: "cable",
      vendor: "Lapp",
      location: "lab-1/drawer-3",
      part_number: "UNITRONIC-300",
      notes: "",
    },
    { bound: "2026-05-20T10:04:00.000Z" },
  ),
  part(
    "H9N4PQRS7TUVW2",
    "bound",
    MINT_1,
    {
      type: "sensor",
      vendor: "Omega",
      location: "rig-2",
      part_number: "PX309",
      notes: "spare",
    },
    { bound: "2026-05-22T08:45:00.000Z" },
  ),
  part(
    "M2B7CDEF3GHJK4",
    "bound",
    MINT_1,
    {
      type: "pump",
      vendor: "KNF",
      location: "rig-2",
      part_number: "N86KN.18",
      notes: "",
    },
    { bound: "2026-05-28T16:20:00.000Z" },
  ),
  part("T8U3VWXY9Z2ABC", "unbound", MINT_2, {}),
  part("K4M9NPQR2STUV7", "unbound", MINT_2, {}),
  part("C7D2EFGH8JKMN3", "unbound", MINT_2, { notes: "label printed, on desk" }),
  part(
    "V5W8XYZ2B3CDEF",
    "void",
    MINT_1,
    {
      type: "cable",
      vendor: "Lapp",
      location: "scrapped",
      part_number: "UNITRONIC-300",
      notes: "damaged shield",
    },
    { bound: "2026-05-20T10:06:00.000Z", void: "2026-06-03T11:00:00.000Z" },
  ),
];

export const partsFixtures: Fixtures = {
  describe: partsDescribe,
  entities: partsEntities,
};
