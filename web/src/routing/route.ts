import { ID_REGEX } from "../config";

const DEFAULT_BASE_PATH = (() => {
  const base = import.meta.env.BASE_URL || "/part-registry/";
  return base === "/" ? "/part-registry/" : base;
})();

function normalizeBasePath(basePath: string): string {
  if (!basePath.startsWith("/")) return `/${basePath.replace(/^\/+/, "")}`;
  return basePath.endsWith("/") ? basePath : `${basePath}/`;
}

export function normalizeCanonicalId(raw: string): string {
  return raw.trim().toUpperCase().replace(/-/g, "");
}

export type AppPath =
  | { kind: "home" }
  | { kind: "part"; id: string }
  | { kind: "invalid-part-id"; rawSegment: string; normalized: string };

export function parseAppPath(
  pathname: string,
  basePath = DEFAULT_BASE_PATH,
): AppPath {
  const normalizedBase = normalizeBasePath(basePath);
  if (!pathname.startsWith(normalizedBase)) return { kind: "home" };

  const remainder = pathname.slice(normalizedBase.length);
  const segments = remainder.split("/").filter(Boolean);
  if (segments.length !== 1) return { kind: "home" };

  const rawSegment = decodeURIComponent(segments[0]);
  const normalized = normalizeCanonicalId(rawSegment);
  if (!normalized) return { kind: "home" };
  if (!ID_REGEX.test(normalized)) {
    return { kind: "invalid-part-id", rawSegment, normalized };
  }
  return { kind: "part", id: normalized };
}
