// Lookup tab — search by ID/prefix or scan QR, show registry row,
// reprint via cross-tab event.
//
// (A future issue, #10, expands this to a full filterable data-grid
// with column filters and fuzzy search. For the spike: detail view
// of a single match.)

import { ID_REGEX } from "../config";
import { FIELDS, type RegistryRow } from "../registry/schema";
import type { AppContext, Tab } from "../core/types";
import { events, EVENT_REPRINT_REQUEST, type ReprintRequest } from "../core/events";
import { el, button, input, formRow } from "../ui/dom";
import { icon } from "../ui/icons";
import { openScanner, type ScanStatus } from "../ui/scanner";

export const lookupTab: Tab = {
  id: "lookup",
  label: "Lookup",
  mount(container, ctx) {
    container.innerHTML = "";
    container.append(buildUI(ctx));
  },
};

function buildUI(ctx: AppContext): HTMLElement {
  const root = el("div", { class: "tab tab--lookup" });
  const header = el("h2", {}, "Lookup");

  const queryInput = input({
    type: "text",
    placeholder: "Scan QR or paste 12-char ID",
    autocomplete: "off",
    autocapitalize: "characters",
  });

  const scanBtn = button({ class: "icon-only", title: "Scan QR with camera" }, icon("camera"));
  const goBtn = button({ class: "primary" }, icon("search"), " Find");

  const result = el("div", { class: "lookup__result" });

  const onSubmit = () => {
    result.innerHTML = "";
    const raw = queryInput.value.trim().toUpperCase().replace(/-/g, "");
    if (!raw) {
      result.append(el("p", { class: "muted" }, "Enter or scan an ID."));
      return;
    }
    if (raw.length === 12 && !ID_REGEX.test(raw)) {
      result.append(el("p", { class: "error" }, "ID contains characters outside the canonical alphabet."));
      return;
    }
    const matches = raw.length === 12
      ? ([ctx.registry.findById(raw)].filter(Boolean) as RegistryRow[])
      : ctx.registry.find({ prefix: raw });
    if (matches.length === 0) {
      result.append(el("p", { class: "warn" }, `No match for "${raw}".`));
      return;
    }
    if (matches.length > 1) {
      result.append(
        el("p", { class: "warn" }, `${matches.length} matches for prefix "${raw}":`),
      );
      result.append(renderMatchList(matches, ctx));
      return;
    }
    result.append(renderRowDetail(matches[0], ctx));
  };

  goBtn.addEventListener("click", onSubmit);
  queryInput.addEventListener("keydown", (e) => {
    if ((e as KeyboardEvent).key === "Enter") onSubmit();
  });
  scanBtn.addEventListener("click", async () => {
    try {
      const text = await openScanner({
        multi: true,
        // Lookup status is just "what does the registry say?" — no
        // queue concept here. unbound/bound/void map to the visible
        // palette; void shows as bound-grey since it's not actionable.
        resolveStatus: (canonical): ScanStatus => {
          const row = ctx.registry.findById(canonical);
          if (!row) return "unknown";
          if (row.status === "unbound") return "unbound";
          // bound or void — both are "already-known" greyed-out.
          return "bound";
        },
      });
      queryInput.value = text;
      onSubmit();
    } catch {
      // User cancelled or camera failed — silent for cancel.
    }
  });

  root.append(header, formRow([queryInput, scanBtn, goBtn]), result);
  return root;
}

function renderRowDetail(row: RegistryRow, ctx: AppContext): HTMLElement {
  const wrap = el("div", { class: "row-detail" });
  const dl = el("dl");
  for (const f of FIELDS) {
    const value = row[f.key] || "";
    dl.append(el("dt", {}, f.label));
    dl.append(el("dd", {}, value || el("span", { class: "muted" }, "—")));
  }
  wrap.append(dl);

  const reprintBtn = button({ class: "primary" }, icon("reprint"), " Reprint label");
  reprintBtn.addEventListener("click", () => {
    const payload: ReprintRequest = { ids: [row.id] };
    events.emit(EVENT_REPRINT_REQUEST, payload);
    ctx.showTab("print");
  });
  wrap.append(formRow([reprintBtn]));
  return wrap;
}

function renderMatchList(rows: RegistryRow[], ctx: AppContext): HTMLElement {
  const ul = el("ul", { class: "match-list" });
  for (const row of rows) {
    const li = el("li", {});
    const id = el("strong", {}, row.id);
    const meta = el(
      "span",
      { class: "muted" },
      `  ${row.status}  ${row.type || "(unbound)"}  @ ${row.location || "—"}`,
    );
    const reprint = button(
      { class: "icon-only", title: "Reprint" },
      icon("reprint"),
    );
    reprint.addEventListener("click", () => {
      events.emit<ReprintRequest>(EVENT_REPRINT_REQUEST, { ids: [row.id] });
      ctx.showTab("print");
    });
    li.append(id, meta, " ", reprint);
    ul.append(li);
  }
  return ul;
}
