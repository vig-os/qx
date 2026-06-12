// Persistent error card component — replaces alert() for submit and
// validation failures. Shows structured error information with
// recovery actions (Retry, Edit Token, Dismiss).
//
// Inspired by hyrr's ComputeErrorCard pattern: persistent cards with
// actionable data instead of dismissible toasts or blocking alerts.

import { el, button } from "./dom";
import { icon } from "./icons";

export interface ErrorCardAction {
  label: string;
  onClick: () => void;
  /** "primary" renders as the blue primary button; "destructive" in red. */
  style?: "primary" | "destructive" | "outline";
}

export interface ErrorCardOptions {
  /** Short headline, e.g. "Submit failed" */
  title: string;
  /** Human-readable error message. */
  message: string;
  /** Optional structured details (step, status code, field name). */
  details?: Array<{ label: string; value: string }>;
  /** Recovery actions. At least one should dismiss the card. */
  actions: ErrorCardAction[];
  /** "error" (red border) or "warning" (amber border). Default "error". */
  kind?: "error" | "warning";
}

/**
 * Render a persistent error card. Caller is responsible for inserting
 * it into the DOM (and removing any previous card first).
 */
export function renderErrorCard(opts: ErrorCardOptions): HTMLElement {
  const kind = opts.kind ?? "error";
  const card = el("div", { class: `error-card error-card--${kind}` });

  const header = el("div", { class: "error-card__header" });
  header.append(icon("bug"), el("strong", {}, ` ${opts.title}`));
  card.append(header);

  card.append(el("p", { class: "error-card__message" }, opts.message));

  if (opts.details && opts.details.length > 0) {
    const dl = el("div", { class: "error-card__details" });
    for (const d of opts.details) {
      dl.append(
        el("span", { class: "error-card__detail-label muted small" }, d.label + ": "),
        el("span", { class: "error-card__detail-value small" }, d.value),
        el("br", {}),
      );
    }
    card.append(dl);
  }

  if (opts.actions.length > 0) {
    const actions = el("div", { class: "error-card__actions" });
    for (const a of opts.actions) {
      const btn = button({ class: a.style ?? "outline" }, a.label);
      btn.addEventListener("click", a.onClick);
      actions.append(btn);
    }
    card.append(actions);
  }

  return card;
}

/**
 * Build a validation error card from field-level validation errors.
 * Shows which fields failed and why, with a "Fix" action that focuses
 * the first errored field.
 */
export function renderValidationErrors(
  errors: Array<{ field: string; message: string }>,
  onDismiss: () => void,
): HTMLElement {
  return renderErrorCard({
    title: "Validation errors",
    message: `${errors.length} field${errors.length > 1 ? "s" : ""} failed validation.`,
    kind: "warning",
    details: errors.map((e) => ({ label: e.field, value: e.message })),
    actions: [
      { label: "Dismiss", onClick: onDismiss },
    ],
  });
}
