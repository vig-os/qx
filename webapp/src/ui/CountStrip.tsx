// Count-by-status strip. Status tokens come from lifecycle.statuses,
// counts from the Count op — both data, nothing hardcoded. The chips
// double as the status filter.

export function CountStrip({
  statuses,
  counts,
  active,
  onSelect,
}: {
  statuses: string[];
  counts: Record<string, number>;
  active: string | null;
  onSelect: (status: string | null) => void;
}) {
  return (
    <div className="flex flex-wrap gap-2">
      {statuses.map((status) => (
        <button
          key={status}
          type="button"
          onClick={() => onSelect(active === status ? null : status)}
          aria-pressed={active === status}
          className={`rounded-full border px-3 py-1 text-sm ${
            active === status
              ? "border-zinc-800 bg-zinc-800 text-white"
              : "border-zinc-300 bg-white text-zinc-700 hover:border-zinc-500"
          }`}
        >
          {status} <span className="font-semibold">{counts[status] ?? 0}</span>
        </button>
      ))}
    </div>
  );
}
