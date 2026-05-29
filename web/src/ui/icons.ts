// Centralized icon access. Tree-shaken Lucide imports — only the icons
// we actually use are bundled. Adding a new icon = one import + one
// entry in `ICONS`.
//
// SSOT for which icons the app uses, and a single place to swap icon
// libraries (the consumer API is `icon("camera")`).
//
// Lucide exports each icon as a data array of [tag, attrs] tuples.
// The SVG wrapper is added here at render time — that's why we don't
// just inline the strings.

import {
  Camera,
  Bug,
  Trash2,
  Pencil,
  Plus,
  X,
  Search,
  Printer,
  Sun,
  Moon,
  ScanLine,
  ScanText,
  Copy,
  RotateCw,
  Download,
  Upload,
  ListChecks,
  Check,
  Settings,
  LogIn,
  LogOut,
  User,
  ExternalLink,
  Shield,
  Eye,
  EyeOff,
  type IconNode,
} from "lucide";

const ICONS: Record<string, IconNode> = {
  camera: Camera,
  bug: Bug,
  trash: Trash2,
  edit: Pencil,
  plus: Plus,
  x: X,
  search: Search,
  printer: Printer,
  sun: Sun,
  moon: Moon,
  scan: ScanLine,
  "scan-text": ScanText,
  copy: Copy,
  reprint: RotateCw,
  download: Download,
  upload: Upload,
  "list-checks": ListChecks,
  check: Check,
  settings: Settings,
  "log-in": LogIn,
  "log-out": LogOut,
  user: User,
  "external-link": ExternalLink,
  shield: Shield,
  eye: Eye,
  "eye-off": EyeOff,
};

export type IconName = keyof typeof ICONS;

const SVG_NS = "http://www.w3.org/2000/svg";

export function icon(name: IconName, opts: { size?: number; class?: string } = {}): SVGElement {
  const node = ICONS[name];
  if (!node) throw new Error(`Unknown icon: ${name}`);
  const size = opts.size ?? 16;
  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("xmlns", SVG_NS);
  svg.setAttribute("width", String(size));
  svg.setAttribute("height", String(size));
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "2");
  svg.setAttribute("stroke-linecap", "round");
  svg.setAttribute("stroke-linejoin", "round");
  svg.setAttribute("class", `icon ${opts.class ?? ""}`.trim());
  for (const [tag, attrs] of node) {
    const child = document.createElementNS(SVG_NS, tag);
    for (const [k, v] of Object.entries(attrs)) {
      if (v === undefined) continue;
      child.setAttribute(k, String(v));
    }
    svg.append(child);
  }
  return svg;
}
