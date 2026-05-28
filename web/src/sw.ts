/// <reference lib="webworker" />
// Custom service worker — extends Workbox precaching with a secure
// token enclave for GitHub API requests (#133).
//
// The token is held in SW-scoped memory — never in sessionStorage,
// localStorage, or IndexedDB. The page posts the token once via
// postMessage; subsequent fetches to api.github.com are intercepted
// and the Authorization header is injected by the SW. The token never
// returns to the page JS context after the initial handoff.
//
// Uses injectManifest strategy: vite-plugin-pwa injects the precache
// manifest at build time via the `self.__WB_MANIFEST` placeholder.

import { cleanupOutdatedCaches, precacheAndRoute, createHandlerBoundToURL } from "workbox-precaching";
import { registerRoute, NavigationRoute } from "workbox-routing";
import { NetworkFirst } from "workbox-strategies";
import { ExpirationPlugin } from "workbox-expiration";
import { CacheableResponsePlugin } from "workbox-cacheable-response";

declare const self: ServiceWorkerGlobalScope;

// ---- Precaching (replaces generateSW's auto-precache) ----
cleanupOutdatedCaches();
precacheAndRoute(self.__WB_MANIFEST);

// ---- SPA navigation fallback ----
// Serve index.html for all navigation requests (SPA routing).
const handler = createHandlerBoundToURL("index.html");
const navigationRoute = new NavigationRoute(handler);
registerRoute(navigationRoute);

// ---- Runtime caching for registry CSV ----
registerRoute(
  /^https:\/\/raw\.githubusercontent\.com\/.+\.csv$/,
  new NetworkFirst({
    cacheName: "registry-data",
    networkTimeoutSeconds: 5,
    plugins: [
      new ExpirationPlugin({
        maxEntries: 32,
        maxAgeSeconds: 60 * 60 * 24,
      }),
      new CacheableResponsePlugin({ statuses: [0, 200] }),
    ],
  }),
);

// ---- Token enclave ----
//
// The token lives in a closure-scoped variable. It's set via postMessage
// from the page and used to inject Authorization headers into GitHub API
// requests. The token is never exposed back to the page.

let _ghToken: string | null = null;
let _ghUser: string | null = null;

// Message protocol:
//   { type: "SET_TOKEN", token: string }
//   { type: "CLEAR_TOKEN" }
//   { type: "GET_AUTH_STATE" } → responds with { type: "AUTH_STATE", hasToken, user }
//   { type: "GH_FETCH", url, init?, requestId } → responds with { type: "GH_FETCH_RESULT", requestId, ok, status, body }

self.addEventListener("message", (event) => {
  const data = event.data;
  if (!data || typeof data !== "object") return;

  switch (data.type) {
    case "SET_TOKEN": {
      _ghToken = data.token ?? null;
      _ghUser = data.user ?? null;
      // Notify all clients of the state change
      void self.clients.matchAll().then((clients) => {
        for (const client of clients) {
          client.postMessage({
            type: "AUTH_STATE",
            hasToken: !!_ghToken,
            user: _ghUser,
          });
        }
      });
      break;
    }
    case "CLEAR_TOKEN": {
      _ghToken = null;
      _ghUser = null;
      void self.clients.matchAll().then((clients) => {
        for (const client of clients) {
          client.postMessage({
            type: "AUTH_STATE",
            hasToken: false,
            user: null,
          });
        }
      });
      break;
    }
    case "GET_AUTH_STATE": {
      event.source?.postMessage({
        type: "AUTH_STATE",
        hasToken: !!_ghToken,
        user: _ghUser,
      });
      break;
    }
    case "GH_FETCH": {
      // Proxy a GitHub API request with the stored token.
      if (!_ghToken) {
        event.source?.postMessage({
          type: "GH_FETCH_RESULT",
          requestId: data.requestId,
          ok: false,
          status: 0,
          body: "No token stored in service worker",
        });
        break;
      }
      void handleGhFetch(data, event.source as Client);
      break;
    }
  }
});

async function handleGhFetch(
  data: { url: string; init?: RequestInit; requestId: string },
  client: Client,
): Promise<void> {
  try {
    // Validate URL — only allow api.github.com
    const url = new URL(data.url);
    if (url.hostname !== "api.github.com") {
      client.postMessage({
        type: "GH_FETCH_RESULT",
        requestId: data.requestId,
        ok: false,
        status: 0,
        body: "SW token proxy only allows api.github.com requests",
      });
      return;
    }

    const res = await fetch(data.url, {
      ...data.init,
      headers: {
        Accept: "application/vnd.github+json",
        Authorization: `Bearer ${_ghToken}`,
        "X-GitHub-Api-Version": "2022-11-28",
        ...(data.init?.headers ?? {}),
      },
    });

    const body = await res.text();
    client.postMessage({
      type: "GH_FETCH_RESULT",
      requestId: data.requestId,
      ok: res.ok,
      status: res.status,
      body,
    });
  } catch (err) {
    client.postMessage({
      type: "GH_FETCH_RESULT",
      requestId: data.requestId,
      ok: false,
      status: 0,
      body: `SW fetch error: ${(err as Error).message}`,
    });
  }
}

// ---- Activate immediately (skip waiting) for autoUpdate strategy ----
self.addEventListener("install", () => void self.skipWaiting());
self.addEventListener("activate", (event) => {
  event.waitUntil(self.clients.claim());
});
