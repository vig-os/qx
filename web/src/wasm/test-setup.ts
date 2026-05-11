// Vitest setup: load the part-registry-wasm bundle synchronously
// from disk so layout tests can call the sync `renderLabelSync`
// surface. Per foundation issue #33: the FE layouts now talk to the
// WASM façade and tests must mirror the production init path.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { resolve, dirname } from "node:path";

import { initWasmFromBytes } from "./loader";

const here = dirname(fileURLToPath(import.meta.url));
const wasmPath = resolve(here, "part_registry_wasm_bg.wasm");
const bytes = readFileSync(wasmPath);
initWasmFromBytes(bytes);
