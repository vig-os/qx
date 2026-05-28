// Service Worker bridge — proxies GitHub API requests through the SW
// token enclave so the PAT never re-enters page JS after the initial
// handoff.
//
// Falls back to direct fetch with sessionStorage token when:
//   - SW is not registered (dev mode, first visit before SW activates)
//   - SW doesn't respond within 10s (timeouts)
//
// The fallback preserves the existing behavior from before the SW
// enclave was added — the auth modal still works either way.

/** Send the validated token to the SW for secure storage. */
export async function sendTokenToSW(token: string, user: string): Promise<boolean> {
  const sw = navigator.serviceWorker?.controller;
  if (!sw) {
    // SW not active yet — try waiting for it
    const reg = await navigator.serviceWorker?.ready;
    const active = reg?.active;
    if (!active) return false;
    active.postMessage({ type: "SET_TOKEN", token, user });
    return true;
  }
  sw.postMessage({ type: "SET_TOKEN", token, user });
  return true;
}

/** Tell the SW to clear the token. */
export function clearTokenInSW(): void {
  navigator.serviceWorker?.controller?.postMessage({ type: "CLEAR_TOKEN" });
}

/** Check if the SW has a token stored. */
export function getAuthStateFromSW(): Promise<{ hasToken: boolean; user: string | null }> {
  return new Promise((resolve) => {
    const sw = navigator.serviceWorker?.controller;
    if (!sw) {
      resolve({ hasToken: false, user: null });
      return;
    }

    const handler = (event: MessageEvent) => {
      if (event.data?.type === "AUTH_STATE") {
        navigator.serviceWorker.removeEventListener("message", handler);
        resolve({ hasToken: event.data.hasToken, user: event.data.user });
      }
    };
    navigator.serviceWorker.addEventListener("message", handler);
    sw.postMessage({ type: "GET_AUTH_STATE" });

    // Timeout after 2s
    setTimeout(() => {
      navigator.serviceWorker.removeEventListener("message", handler);
      resolve({ hasToken: false, user: null });
    }, 2000);
  });
}

/**
 * Make a GitHub API request through the SW token enclave.
 * Falls back to direct fetch if SW is unavailable.
 */
export async function ghFetchViaSW(
  url: string,
  fallbackToken: string,
  init?: RequestInit,
): Promise<Response> {
  const sw = navigator.serviceWorker?.controller;

  // If SW is available, proxy through it
  if (sw) {
    const requestId = crypto.randomUUID();

    const result = await new Promise<{
      ok: boolean;
      status: number;
      body: string;
    }>((resolve) => {
      const handler = (event: MessageEvent) => {
        if (
          event.data?.type === "GH_FETCH_RESULT" &&
          event.data.requestId === requestId
        ) {
          navigator.serviceWorker.removeEventListener("message", handler);
          resolve(event.data);
        }
      };
      navigator.serviceWorker.addEventListener("message", handler);

      sw.postMessage({
        type: "GH_FETCH",
        url,
        init: init
          ? {
              method: init.method,
              body: init.body,
              headers: init.headers,
            }
          : undefined,
        requestId,
      });

      // Timeout after 30s
      setTimeout(() => {
        navigator.serviceWorker.removeEventListener("message", handler);
        resolve({ ok: false, status: 0, body: "SW proxy timeout" });
      }, 30_000);
    });

    // Convert SW response back to a Response-like object
    return new Response(result.body, {
      status: result.status || 500,
      headers: { "Content-Type": "application/json" },
    });
  }

  // Fallback: direct fetch with token in JS (pre-SW behavior)
  return fetch(url, {
    ...init,
    headers: {
      Accept: "application/vnd.github+json",
      Authorization: `Bearer ${fallbackToken}`,
      "X-GitHub-Api-Version": "2022-11-28",
      ...(init?.headers ?? {}),
    },
  });
}
