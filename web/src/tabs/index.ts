// Tab registry. Open/Closed: drop a new tab file in this folder and
// register it here.

import type { Tab } from "../core/types";
import { lookupTab } from "./lookup";
import { printTab } from "./print";
import { bindTab } from "./bind";
import { mintTab } from "./mint";

export const TABS: Tab[] = [lookupTab, printTab, bindTab, mintTab];
