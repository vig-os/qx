// Core extension interfaces.
//
// SOLID — Open/Closed: new tabs, layouts, and plugins are added by
// implementing these interfaces and registering. The core never needs
// to know about them.
//
// SOLID — Interface Segregation: each interface is the smallest set of
// methods needed for that role. A Plugin doesn't know about layouts; a
// Layout doesn't know about tabs.

import type { Registry } from "../registry/registry";
import type { AppPath } from "../routing/route";

export interface AppContext {
  registry: Registry;
  /** Switch to a tab by id (tab navigation as a primitive). */
  showTab(id: string): void;
  /** Promote a canonical part id into the current route state / URL. */
  showPart(id: string): void;
  /** Current route state derived from the browser location. */
  getRoute(): AppPath;
  // Future: auth provider, settings store, plugin host, etc.
}

// ---------- Tab ----------
//
// A tab is a top-level navigation target. Implementations are
// registered in src/tabs/index.ts.

export interface Tab {
  readonly id: string;
  readonly label: string;
  /** Render into the given container. Called once when the tab is shown. */
  mount(container: HTMLElement, ctx: AppContext): void | Promise<void>;
  /** Optional cleanup — called when the tab is hidden. */
  unmount?(): void;
}

// ---------- Layout ----------
//
// A label layout is a recipe for arranging the QR + 4/4/4 text blocks
// at a given size. Implementations registered in src/layouts/index.ts.
//
// Adding a new layout (e.g. a circular tag) = new file, register, done.

export interface LayoutOptions {
  size: number; // mm of short side
  // Layout-specific options live in the variant tagged by `layout`
  // (e.g. cableOd for flag). Kept open via `extra` to avoid a closed
  // discriminated union the core has to know about.
  extra?: Record<string, unknown>;
}

export interface LayoutDimensions {
  widthMm: number;
  heightMm: number;
}

export interface Layout {
  readonly id: string;
  readonly label: string;
  readonly description: string;
  /** Compute the (w, h) without rendering, for print page sizing. */
  measure(opts: LayoutOptions): LayoutDimensions;
  /** Render an SVG string for the given canonical ID. mm-native. */
  renderSvg(canonical: string, opts: LayoutOptions): string;
  /** Optional: extra form fields the Print tab should expose for this layout. */
  optionFields?(): LayoutOptionField[];
}

export interface LayoutOptionField {
  key: string;
  label: string;
  type: "number" | "checkbox";
  default: number;
  min?: number;
  max?: number;
  step?: number;
}

// ---------- OutputMode ----------
//
// An output mode is a paper-aware page-layout strategy: it decides how
// the flat list of (id, layout, copies) items from the Print tab gets
// arranged into printable pages. The default is `dk-continuous` — one
// page per label on continuous DK tape, printer auto-cuts between.
// Other modes pack multiple labels onto a single die-cut sheet
// (DK-1201), tile a strip with crop marks (#7), or fill an A4 sticker
// sheet.
//
// Layout decides what *one label* looks like. OutputMode decides how
// *N labels* lay out on paper. Adding a new paper format (or sheet
// size, or cut-mark scheme) = new file in src/output/, register, done.
//
// The Print tab is kept dumb: it builds JobItem[] and delegates the
// planning + print-HTML emission to the selected mode.

export interface OutputModeField {
  key: string;
  label: string;
  type: "number" | "select";
  default: number | string;
  min?: number;
  max?: number;
  step?: number;
  options?: { value: string; label: string }[];
  /** Optional inline help shown next to the field. */
  hint?: string;
}

/**
 * One physical page emitted by an OutputMode.
 *
 * `widthMm`/`heightMm` are the physical media dimensions (e.g. 29×90
 * for a DK-1201 die-cut). `bodyHtml` is the HTML to drop inside the
 * page's content `<div>` — already sized/positioned by the mode.
 */
export interface PlannedPage {
  widthMm: number;
  heightMm: number;
  /** Inner content, mm-positioned. */
  bodyHtml: string;
  /** Optional: how many labels are on this page (for plan summaries). */
  labelCount?: number;
}

export interface OutputMode {
  readonly id: string;
  readonly label: string;
  readonly description: string;
  /** Form fields the Print tab should expose for this mode. */
  optionFields(): OutputModeField[];
  /**
   * Plan the job: take the JobItem-equivalent list of (id, layoutId,
   * size, copies, extras) and the user's option values; return the
   * physical pages.
   *
   * The mode is responsible for using the registered Layouts (via
   * `getLayout`) to render label SVGs at the correct size.
   */
  plan(items: PlanItem[], opts: Record<string, number | string>): PlannedPage[];
  /** Build the print-window HTML document for the planned pages. */
  renderPrintHtml(pages: PlannedPage[]): string;
}

/**
 * The OutputMode-facing view of a single Print tab row. Decoupled
 * from the Print tab's internal `JobItem` so modes can be unit-tested
 * without dragging in the tab.
 */
export interface PlanItem {
  id: string;
  layoutId: string;
  size: number;
  copies: number;
  extras: Record<string, number>;
}

// ---------- Plugin ----------
//
// A plugin attaches to the running app — toolbar buttons, observers,
// modal launchers, etc. — without being a tab. Error reporting,
// keyboard shortcut registries, future telemetry hooks all fit here.

export interface Plugin {
  readonly id: string;
  install(host: PluginHost, ctx: AppContext): void;
  uninstall?(): void;
}

export interface PluginHost {
  /** Add a button to the global toolbar (top-right). */
  addToolbarButton(spec: ToolbarButtonSpec): () => void;
  /** Display a transient toast message (info / error). */
  toast(message: string, kind?: "info" | "error"): void;
}

export interface ToolbarButtonSpec {
  id: string;
  label: string;
  title?: string;
  onClick: () => void | Promise<void>;
}
