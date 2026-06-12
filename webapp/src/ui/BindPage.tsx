// Bind queue page (#/bind): look up an id (protocol Resolve, so prefix
// and scheme forms work), fill the descriptor's editable fields, queue
// the bind locally (localStorage), and submit the queue as sequential
// Transition{to: "bound", fields} ops.
//
// Outcomes are honest: the mock applies binds immediately; the wasm
// transport answers Auth (no operator signed in) or Backend (proposal
// submission lands with the OAuth + PR wiring) — those errors render
// verbatim and failed items stay queued for a later retry.

import { useState } from "react";
import type {
  CollectionDescriptor,
  Entity,
  ProtocolError,
  Response,
  TransitionData,
} from "../protocol";
import { useTransport } from "../data/TransportContext";
import {
  dequeue,
  enqueue,
  loadQueue,
  saveQueue,
  settle,
  type BindQueueItem,
} from "../bind/queue";

const inputClass = "rounded border border-zinc-300 bg-white px-2 py-1 text-sm";

interface SubmitOutcome {
  id: string;
  ok: boolean;
  message: string;
}

export function BindPage({ descriptor }: { descriptor: CollectionDescriptor }) {
  const transport = useTransport();
  const editableFields = descriptor.fields.filter((f) => f.editable);

  const [queue, setQueue] = useState<BindQueueItem[]>(() => loadQueue());
  const [idInput, setIdInput] = useState("");
  const [resolved, setResolved] = useState<Entity | null>(null);
  const [lookupError, setLookupError] = useState<ProtocolError | null>(null);
  const [fieldValues, setFieldValues] = useState<Record<string, string>>({});
  const [submitting, setSubmitting] = useState(false);
  const [outcomes, setOutcomes] = useState<SubmitOutcome[]>([]);

  const updateQueue = (next: BindQueueItem[]) => {
    saveQueue(next);
    setQueue(next);
  };

  const lookup = async () => {
    setLookupError(null);
    setResolved(null);
    try {
      const res = await transport({ op: "Resolve", id: idInput });
      if (res.ok) {
        const entity = res.data as Entity;
        setResolved(entity);
        // Canonicalize the input and pre-fill editable fields from the
        // entity's current values.
        setIdInput(entity.id);
        const prefill: Record<string, string> = {};
        for (const f of editableFields) {
          const v = entity.fields[f.key];
          if (v !== undefined && v !== "") prefill[f.key] = v;
        }
        setFieldValues(prefill);
      } else {
        setLookupError(res.error);
      }
    } catch (e) {
      setLookupError({ kind: "Backend", message: String(e) });
    }
  };

  const queueBind = () => {
    const id = (resolved?.id ?? idInput).trim();
    if (id === "") return;
    const fields: Record<string, string> = {};
    for (const [k, v] of Object.entries(fieldValues)) {
      if (v.trim() !== "") fields[k] = v;
    }
    updateQueue(enqueue(queue, id, fields));
    setIdInput("");
    setResolved(null);
    setLookupError(null);
    setFieldValues({});
  };

  const submitQueue = async () => {
    setSubmitting(true);
    const results: SubmitOutcome[] = [];
    let current = queue;
    for (const item of [...current]) {
      let res: Response;
      try {
        res = await transport({
          op: "Transition",
          collection: descriptor.name,
          id: item.id,
          to: "bound",
          fields: item.fields,
        });
      } catch (e) {
        res = { ok: false, error: { kind: "Backend", message: String(e) } };
      }
      if (res.ok) {
        const data = res.data as TransitionData;
        results.push({ id: item.id, ok: true, message: `bound — proposal ${data.proposal.url}` });
      } else {
        results.push({ id: item.id, ok: false, message: `${res.error.kind}: ${res.error.message}` });
      }
      current = settle(current, item.id, res);
    }
    updateQueue(current);
    setOutcomes(results);
    setSubmitting(false);
  };

  return (
    <div className="space-y-6">
      <section className="space-y-3">
        <h2 className="font-semibold">Add to queue</h2>
        <div className="flex items-center gap-2">
          <input
            type="text"
            value={idInput}
            onChange={(e) => setIdInput(e.target.value)}
            placeholder="id or ≥8-char prefix"
            aria-label="bind id"
            className={`${inputClass} w-72 font-mono`}
          />
          <button
            type="button"
            onClick={() => void lookup()}
            disabled={idInput.trim() === ""}
            className="rounded border border-zinc-300 px-3 py-1.5 text-sm disabled:opacity-40"
          >
            Look up
          </button>
        </div>
        {lookupError && (
          <p className="text-sm text-red-700">
            {lookupError.kind}: {lookupError.message}
          </p>
        )}
        {resolved && (
          <p className="text-sm text-zinc-600">
            <span className="font-mono">{resolved.id}</span> — status {resolved.status}
          </p>
        )}
        <div className="grid max-w-xl gap-2">
          {editableFields.map((f) => (
            <label key={f.key} className="flex items-center gap-2 text-sm">
              <span className="w-32 text-zinc-600">{f.label}</span>
              <input
                type="text"
                value={fieldValues[f.key] ?? ""}
                onChange={(e) =>
                  setFieldValues((prev) => ({ ...prev, [f.key]: e.target.value }))
                }
                aria-label={f.label}
                className={`${inputClass} flex-1`}
              />
            </label>
          ))}
        </div>
        <button
          type="button"
          onClick={queueBind}
          disabled={idInput.trim() === ""}
          className="rounded bg-zinc-900 px-3 py-1.5 text-sm text-white disabled:opacity-40"
        >
          Queue bind
        </button>
      </section>

      <section className="space-y-3">
        <h2 className="font-semibold">Queue ({queue.length})</h2>
        {queue.length === 0 ? (
          <p className="text-sm text-zinc-500">Queue is empty.</p>
        ) : (
          <table className="w-full border-collapse text-sm">
            <thead>
              <tr className="border-b border-zinc-300 text-left">
                <th className="px-3 py-2 font-medium text-zinc-600">id</th>
                {editableFields.map((f) => (
                  <th key={f.key} className="px-3 py-2 font-medium text-zinc-600">
                    {f.label}
                  </th>
                ))}
                <th className="px-3 py-2 font-medium text-zinc-600">outcome</th>
                <th className="px-3 py-2" />
              </tr>
            </thead>
            <tbody>
              {queue.map((item) => (
                <tr key={item.id} className="border-b border-zinc-100 align-top">
                  <td className="px-3 py-2 font-mono text-xs">{item.id}</td>
                  {editableFields.map((f) => (
                    <td key={f.key} className="px-3 py-2">
                      {item.fields[f.key] ?? ""}
                    </td>
                  ))}
                  <td className="px-3 py-2">
                    {item.error && (
                      <span className="text-red-700">
                        {item.error.kind}: {item.error.message}
                      </span>
                    )}
                  </td>
                  <td className="px-3 py-2">
                    <button
                      type="button"
                      onClick={() => updateQueue(dequeue(queue, item.id))}
                      className="text-zinc-500 hover:text-zinc-900"
                    >
                      remove
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        <button
          type="button"
          onClick={() => void submitQueue()}
          disabled={submitting || queue.length === 0}
          className="rounded bg-zinc-900 px-3 py-1.5 text-sm text-white disabled:opacity-40"
        >
          Submit queue
        </button>
      </section>

      {outcomes.length > 0 && (
        <section className="space-y-2">
          <h2 className="font-semibold">Last submit</h2>
          <ul className="space-y-1 text-sm">
            {outcomes.map((o) => (
              <li key={o.id} className={o.ok ? "text-emerald-700" : "text-red-700"}>
                <span className="font-mono text-xs">{o.id}</span> — {o.message}
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}
