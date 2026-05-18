// Tiny event bus for cross-tab / cross-plugin communication.
//
// SOLID — Dependency Inversion: tabs publish/subscribe to abstract
// event names rather than knowing about each other. The Lookup tab
// emits "reprint:request"; the Print tab listens. Neither imports
// the other.

type Handler<T> = (payload: T) => void;

class EventBus {
  private handlers = new Map<string, Set<Handler<unknown>>>();

  on<T>(event: string, handler: Handler<T>): () => void {
    let set = this.handlers.get(event);
    if (!set) {
      set = new Set();
      this.handlers.set(event, set);
    }
    set.add(handler as Handler<unknown>);
    return () => set!.delete(handler as Handler<unknown>);
  }

  emit<T>(event: string, payload: T): void {
    const set = this.handlers.get(event);
    if (!set) return;
    for (const h of set) (h as Handler<T>)(payload);
  }
}

export const events = new EventBus();

// Event names — SSOT for cross-module event types.
export const EVENT_REPRINT_REQUEST = "reprint:request" as const;
export const EVENT_TAB_SHOW = "tab:show" as const;
export const EVENT_PLAN_CHANGED = "plan:changed" as const;
export const EVENT_QUEUE_CHANGED = "queue:changed" as const;

export interface ReprintRequest {
  ids: string[];
}
