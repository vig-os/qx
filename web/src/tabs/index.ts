// Tab registry. Open/Closed: drop a new tab file in this folder and
// register it here. Feature flags from deploy-config control visibility.

import type { Tab } from "../core/types";
import { getConfig } from "../config/deploy-config";
import { lookupTab } from "./lookup";
import { printTab } from "./print";
import { bindTab } from "./bind";
import { mintTab } from "./mint";

const ALL_TABS: Array<{ tab: Tab; featureKey: keyof typeof _features }> = [
  { tab: lookupTab, featureKey: "enableLookupTab" },
  { tab: printTab, featureKey: "enablePrintTab" },
  { tab: bindTab, featureKey: "enableBindTab" },
  { tab: mintTab, featureKey: "enableMintTab" },
];

// Feature defaults — lookup is always shown (no flag).
const _features = {
  enableLookupTab: true,
  enablePrintTab: true,
  enableBindTab: true,
  enableMintTab: true,
};

function getFeatures(): typeof _features {
  try {
    const cfg = getConfig();
    return { ..._features, ...cfg.features } as typeof _features;
  } catch {
    return _features;
  }
}

// Lazy evaluation — config is read at first call, not module load time.
// This allows window.__PART_REGISTRY_CONFIG__ (e.g. via addInitScript
// in Playwright tests) to be set before tabs are computed.
export function TABS(): Tab[] {
  const features = getFeatures();
  return ALL_TABS
    .filter(({ featureKey }) => {
      return (features as Record<string, boolean>)[featureKey] !== false;
    })
    .map(({ tab }) => tab);
}
