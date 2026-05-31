import { describe, it, expect, beforeEach, vi } from "vitest";
import { openModal } from "./modal";
import { el } from "../dom";

beforeEach(() => {
  document.body.innerHTML = "";
});

function press(key: string) {
  document.dispatchEvent(new KeyboardEvent("keydown", { key }));
}

describe("openModal", () => {
  it("mounts an overlay + card with the requested classes", () => {
    openModal({ body: el("div", {}, "hi"), overlayClass: "detail-modal-overlay", cardClass: "detail-modal" });
    const overlay = document.querySelector(".modal-overlay");
    expect(overlay).toBeTruthy();
    expect(overlay!.classList.contains("detail-modal-overlay")).toBe(true);
    expect(document.querySelector(".modal-card.detail-modal")).toBeTruthy();
  });

  it("sets dialog a11y attributes", () => {
    openModal({ body: el("div", {}), ariaLabel: "Thing" });
    const card = document.querySelector(".modal-card")!;
    expect(card.getAttribute("role")).toBe("dialog");
    expect(card.getAttribute("aria-modal")).toBe("true");
    expect(card.getAttribute("aria-label")).toBe("Thing");
  });

  it("closes on Escape (the bug the component fixes)", () => {
    const onClose = vi.fn();
    openModal({ body: el("div", {}), onClose });
    expect(document.querySelector(".modal-overlay")).toBeTruthy();
    press("Escape");
    expect(document.querySelector(".modal-overlay")).toBeNull();
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("closes on backdrop click but not on card click", () => {
    const { overlay, card } = openModal({ body: el("div", {}) });
    card.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(document.querySelector(".modal-overlay")).toBeTruthy();
    overlay.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(document.querySelector(".modal-overlay")).toBeNull();
  });

  it("closes via the X button", () => {
    openModal({ body: el("div", {}) });
    const closeBtn = document.querySelector<HTMLButtonElement>(".modal-card__close")!;
    expect(closeBtn).toBeTruthy();
    closeBtn.click();
    expect(document.querySelector(".modal-overlay")).toBeNull();
  });

  it("removes the keydown listener on close (no leak / double-close)", () => {
    const onClose = vi.fn();
    const { close } = openModal({ body: el("div", {}), onClose });
    close();
    press("Escape"); // listener gone — must not fire onClose again
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("dismissable:false blocks ESC, backdrop, and omits the X", () => {
    const { overlay } = openModal({ body: el("div", {}), dismissable: false });
    expect(document.querySelector(".modal-card__close")).toBeNull();
    press("Escape");
    overlay.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(document.querySelector(".modal-overlay")).toBeTruthy(); // still open
  });

  it("passes close to a function body", () => {
    let received: (() => void) | null = null;
    openModal({ body: (close) => { received = close; return el("div", {}); } });
    expect(typeof received).toBe("function");
    received!();
    expect(document.querySelector(".modal-overlay")).toBeNull();
  });
});
