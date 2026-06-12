// Deliberately tiny hash router: two routes (#/ grid, #/<id> detail) do
// not justify a routing dependency, and hash routing keeps the SPA
// serverless-deployable (GitHub Pages) with zero server config.

import { useEffect, useState } from "react";

function currentPath(): string {
  return window.location.hash.replace(/^#/, "");
}

/** Returns the current hash path, e.g. "" | "/" | "/PQ7G2MNVX4KH9T". */
export function useHashRoute(): string {
  const [path, setPath] = useState(currentPath);
  useEffect(() => {
    const onChange = () => setPath(currentPath());
    window.addEventListener("hashchange", onChange);
    return () => window.removeEventListener("hashchange", onChange);
  }, []);
  return path;
}

export function entityHref(id: string): string {
  return `#/${id}`;
}
