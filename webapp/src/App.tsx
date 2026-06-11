// App shell: registry name + collection roster come from Describe —
// the shell renders what the registry declares (ADR-035 §0).

import { useDescribe } from "./data/hooks";
import { useHashRoute } from "./router";
import { DetailPage } from "./ui/DetailPage";
import { GridPage } from "./ui/GridPage";

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

  const id = route.replace(/^\/+/, "");

  return (
    <div className="min-h-screen bg-zinc-50 text-zinc-900">
      <header className="border-b border-zinc-200 bg-white px-6 py-3">
        <div className="mx-auto flex max-w-6xl items-baseline gap-3">
          <a href="#/" className="font-semibold">
            {registry.name}
          </a>
          <span className="text-sm text-zinc-500">{collection.name}</span>
        </div>
      </header>
      <main className="mx-auto max-w-6xl p-6">
        {id === "" ? (
          <GridPage descriptor={collection} />
        ) : (
          <DetailPage id={id} descriptor={collection} />
        )}
      </main>
    </div>
  );
}
