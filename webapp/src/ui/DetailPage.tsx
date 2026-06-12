import type { CollectionDescriptor } from "../protocol";
import { useResolve } from "../data/hooks";
import { EntityDetail } from "./EntityDetail";

export function DetailPage({
  id,
  descriptor,
}: {
  id: string;
  descriptor: CollectionDescriptor;
}) {
  const entity = useResolve(id);
  return (
    <div className="space-y-4">
      <a href="#/" className="text-sm text-zinc-500 hover:text-zinc-900">
        ‹ {descriptor.name}
      </a>
      {entity.isPending && <p className="text-zinc-500">…</p>}
      {entity.isError && <p className="text-red-700">{(entity.error as Error).message}</p>}
      {entity.data && <EntityDetail entity={entity.data} descriptor={descriptor} />}
    </div>
  );
}
