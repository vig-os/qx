// App shell: registry name + collection roster come from Describe —
// the shell renders what the registry declares (ADR-035 §0).

import { useDescribe } from "./data/hooks";
import { useHashRoute } from "./router";
import { BindPage } from "./ui/BindPage";
import { DetailPage } from "./ui/DetailPage";
import { GridPage } from "./ui/GridPage";
import { PrintPage } from "./ui/PrintPage";

export default function App() {
  const describe = useDescribe();
  const route = useHashRoute();

  if (describe.isPending) {
    return <p className="p-6 text-zinc-500">…</p>;
  }
  if (describe.isError) {
    return <p className="p-6 text-red-700">{(describe.error as Error).message}</p>;
  }

  const registry = describe.data;
  const collection = registry.collections[0];
  if (!collection) {
    return <p className="p-6 text-red-700">Describe returned no collections</p>;
  }

  const path = route.replace(/^\/+/, "");
  // Page routes are lowercase words; entity ids are nano14 (uppercase,
  // 14 chars), so the namespaces cannot collide.
  const page =
    path === "" ? (
      <GridPage descriptor={collection} />
    ) : path === "print" ? (
      <PrintPage descriptor={collection} />
    ) : path === "bind" ? (
      <BindPage descriptor={collection} />
    ) : (
      <DetailPage id={path} descriptor={collection} />
    );

  return (
    <div className="min-h-screen bg-zinc-50 text-zinc-900">
      <header className="border-b border-zinc-200 bg-white px-6 py-3">
        <div className="mx-auto flex max-w-6xl items-baseline gap-3">
          <a href="#/" className="font-semibold">
            {registry.name}
          </a>
          <span className="text-sm text-zinc-500">{collection.name}</span>
          <nav className="ml-auto flex gap-4 text-sm">
            <a href="#/print" className="text-zinc-600 hover:text-zinc-900">
              print
            </a>
            <a href="#/bind" className="text-zinc-600 hover:text-zinc-900">
              bind
            </a>
          </nav>
        </div>
      </header>
      <main className="mx-auto max-w-6xl p-6">{page}</main>
    </div>
  );
}
