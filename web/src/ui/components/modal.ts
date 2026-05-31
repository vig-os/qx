// Shared modal component (#93 review follow-up).
//
// Before this, every modal/overlay hand-rolled the same scaffold —
// overlay + card + close button + ESC + backdrop-click + focus — and
// they DIVERGED: auth/import modals closed on Escape, but the lookup
// detail/assembly modals and the recovery dialog did not. This owns the
// contract once so every modal behaves identically and stays accessible
// (role=dialog, focus on open, focus restored on close).
//
// Call sites keep their distinctive overlay/card class via opts so
// existing CSS + e2e selectors (e.g. `.detail-modal-overlay`) are
// preserved.

import { el, button } from "../dom";
import { icon } from "../icons";

export interface ModalOptions {
  /** The card's inner content. A function form receives `close` so a
   *  form's Cancel/Save buttons can dismiss the modal. */
  body: HTMLElement | ((close: () => void) => HTMLElement);
  /** Extra class on the overlay (feature styling + test hooks). The base
   *  `modal-overlay` is always applied. */
  overlayClass?: string;
  /** Extra class on the card. Base `modal-card` is always applied. */
  cardClass?: string;
  /** Accessible name when the body has no visible heading. */
  ariaLabel?: string;
  /** Render the built-in ✕ button (default true). */
  closeButton?: boolean;
  /** When false, the modal cannot be dismissed by ESC, backdrop click,
   *  or ✕ — forcing the body's own controls to call `close` (for
   *  forced-choice dialogs like crash recovery). Default true. */
  dismissable?: boolean;
  /** Called after the modal is removed, on any close path. */
  onClose?: () => void;
}

export interface ModalHandle {
  close: () => void;
  overlay: HTMLElement;
  card: HTMLElement;
}

/**
 * Open a modal dialog. Returns a handle with `close()`. Closes on:
 * the ✕ button, a backdrop click, or Escape — each path restores focus
 * to the element that was focused before opening and fires `onClose`.
 */
export function openModal(opts: ModalOptions): ModalHandle {
  const previouslyFocused = document.activeElement as HTMLElement | null;

  const overlay = el("div", {
    class: ["modal-overlay", opts.overlayClass].filter(Boolean).join(" "),
  });
  const card = el("div", {
    class: ["modal-card", opts.cardClass].filter(Boolean).join(" "),
    role: "dialog",
    "aria-modal": "true",
    tabindex: "-1",
    ...(opts.ariaLabel ? { "aria-label": opts.ariaLabel } : {}),
  });

  let closed = false;
  const close = () => {
    if (closed) return;
    closed = true;
    document.removeEventListener("keydown", onKey);
    overlay.remove();
    previouslyFocused?.focus?.();
    opts.onClose?.();
  };

  const dismissable = opts.dismissable !== false;
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") close();
  };
  if (dismissable) {
    document.addEventListener("keydown", onKey);
    overlay.addEventListener("click", (e) => {
      if (e.target === overlay) close();
    });
  }

  if (dismissable && opts.closeButton !== false) {
    const closeBtn = button(
      { class: "icon-only modal-card__close", title: "Close", "aria-label": "Close" },
      icon("x"),
    );
    closeBtn.addEventListener("click", close);
    card.append(closeBtn);
  }

  const body = typeof opts.body === "function" ? opts.body(close) : opts.body;
  card.append(body);
  overlay.append(card);
  document.body.append(overlay);
  card.focus();

  return { close, overlay, card };
}
