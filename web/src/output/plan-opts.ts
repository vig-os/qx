// Shared helper: convert a PlanItem to LayoutOptions, injecting
// global label settings (code type, format, show text) so all
// output modes produce labels consistent with the user's choices.

import type { LayoutOptions, PlanItem } from "../core/types";
import { loadLabelSettings } from "../layouts/label-settings";

export function planItemToOpts(item: PlanItem): LayoutOptions {
  const s = loadLabelSettings();
  return {
    size: item.size,
    extra: {
      ...item.extras,
      micro: s.codeType === "micro",
      format: s.format === "auto" ? undefined : s.format,
      showText: s.showText,
    },
  };
}
