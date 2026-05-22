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

export const TABS: Tab[] = ALL_TABS
  .filter(({ featureKey }) => {
    const features = getFeatures();
    return (features as Record<string, boolean>)[featureKey] !== false;
  })
  .map(({ tab }) => tab);
