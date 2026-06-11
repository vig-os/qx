// Print page (#/print): select entities (pasted ids, or the shared
// filter grammar), choose render options, dispatch the protocol Print
// op, preview the returned SVGs, and hand them to a child window as a
// continuous-roll print document (one page per label; die-cut sheets
// are deliberately out of scope — see src/print/render.ts).
//
// Option values come from the protocol vocabularies (PRINT_LAYOUTS,
// PRINT_CHARS) and the descriptor's lifecycle statuses — nothing here
// invents its own option list. The request sets log: false because the
// read-only deploys have no signed-in operator; print-event audit
// logging is the write-capable shells' path.

import { useState, type ReactNode } from "react";
import type {
  CollectionDescriptor,
  PrintData,
  PrintOptions,
  ProtocolError,
  Request,
  Selection,
} from "../protocol";
import { PRINT_CHARS, PRINT_LAYOUTS } from "../protocol";
import { useTransport } from "../data/TransportContext";
import { planPages, renderPrintDocument } from "../print/render";

const inputClass = "rounded border border-zinc-300 bg-white px-2 py-1 text-sm";

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="flex items-center gap-2 text-sm">
      <span className="w-24 text-zinc-600">{label}</span>
      {children}
    </label>
  );
}

export function PrintPage({ descriptor }: { descriptor: CollectionDescriptor }) {
  const transport = useTransport();

  const [idsText, setIdsText] = useState("");
  const [status, setStatus] = useState("");
  const [text, setText] = useState("");

  const [layout, setLayout] = useState("horz");
  const [sizeMm, setSizeMm] = useState("8");
  const [chars, setChars] = useState("auto");
  const [micro, setMicro] = useState(false);
  const [copies, setCopies] = useState("1");
  const [cableOd, setCableOd] = useState("4");

  const [pending, setPending] = useState(false);
  const [result, setResult] = useState<PrintData | null>(null);
  const [error, setError] = useState<ProtocolError | null>(null);

  const pastedIds = idsText
    .split(/[\s,]+/)
    .map((s) => s.trim())
    .filter((s) => s !== "");

  const buildRequest = (): Request => {
    const selection: Selection =
      pastedIds.length > 0
        ? { ids: pastedIds }
        : {
            filter: {
              status: status === "" ? null : status,
              text: text === "" ? null : text,
            },
          };
    const options: PrintOptions = {
      layout,
      size_mm: Number(sizeMm),
      chars,
      micro,
      copies: Number(copies),
      log: false,
      ...(layout === "flag" ? { cable_od_mm: Number(cableOd) } : {}),
    };
    return { op: "Print", collection: descriptor.name, selection, options };
  };

  const preview = async () => {
    setPending(true);
    setError(null);
    try {
      const res = await transport(buildRequest());
      if (res.ok) {
        setResult(res.data as PrintData);
      } else {
        setResult(null);
        setError(res.error);
      }
    } catch (e) {
      setResult(null);
      setError({ kind: "Backend", message: String(e) });
    } finally {
      setPending(false);
    }
  };

  const printNow = () => {
    if (!result) return;
    const pages = planPages(result, Math.max(1, Number(copies) || 1));
    const html = renderPrintDocument(pages);
    const child = window.open("", "_blank");
    if (!child) {
      setError({ kind: "Backend", message: "popup blocked — allow popups to print" });
      return;
    }
    child.document.write(html);
    child.document.close();
  };

  return (
    <div className="space-y-6">
      <section className="space-y-3">
        <h2 className="font-semibold">Selection</h2>
        <Field label="ids">
          <textarea
            value={idsText}
            onChange={(e) => setIdsText(e.target.value)}
            placeholder="paste ids (whitespace/comma separated) — overrides the filter"
            aria-label="ids"
            rows={2}
            className={`${inputClass} w-full max-w-xl font-mono`}
          />
        </Field>
        <Field label="status">
          <select
            value={status}
            onChange={(e) => setStatus(e.target.value)}
            aria-label="status filter"
            className={inputClass}
          >
            <option value="">*</option>
            {descriptor.lifecycle.statuses.map((s) => (
              <option key={s} value={s}>
                {s}
              </option>
            ))}
          </select>
        </Field>
        <Field label="text">
          <input
            type="search"
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder="free-text filter…"
            aria-label="text filter"
            className={`${inputClass} w-64`}
          />
        </Field>
      </section>

      <section className="space-y-3">
        <h2 className="font-semibold">Options</h2>
        <Field label="layout">
          <select
            value={layout}
            onChange={(e) => setLayout(e.target.value)}
            aria-label="layout"
            className={inputClass}
          >
            {PRINT_LAYOUTS.map((l) => (
              <option key={l} value={l}>
                {l}
              </option>
            ))}
          </select>
        </Field>
        {layout === "flag" && (
          <Field label="cable Ø mm">
            <input
              type="number"
              min={0.5}
              step={0.5}
              value={cableOd}
              onChange={(e) => setCableOd(e.target.value)}
              aria-label="cable od mm"
              className={`${inputClass} w-24`}
            />
          </Field>
        )}
        <Field label="size mm">
          <input
            type="number"
            min={1}
            step={1}
            value={sizeMm}
            onChange={(e) => setSizeMm(e.target.value)}
            aria-label="size mm"
            className={`${inputClass} w-24`}
          />
        </Field>
        <Field label="chars">
          <select
            value={chars}
            onChange={(e) => setChars(e.target.value)}
            aria-label="chars"
            className={inputClass}
          >
            {PRINT_CHARS.map((c) => (
              <option key={c} value={c}>
                {c}
              </option>
            ))}
          </select>
        </Field>
        <Field label="micro">
          <input
            type="checkbox"
            checked={micro}
            onChange={(e) => setMicro(e.target.checked)}
            aria-label="micro"
          />
        </Field>
        <Field label="copies">
          <input
            type="number"
            min={1}
            step={1}
            value={copies}
            onChange={(e) => setCopies(e.target.value)}
            aria-label="copies"
            className={`${inputClass} w-24`}
          />
        </Field>
      </section>

      <div className="flex items-center gap-3">
        <button
          type="button"
          onClick={() => void preview()}
          disabled={pending}
          className="rounded bg-zinc-900 px-3 py-1.5 text-sm text-white disabled:opacity-40"
        >
          Preview
        </button>
        <button
          type="button"
          onClick={printNow}
          disabled={!result}
          className="rounded border border-zinc-300 px-3 py-1.5 text-sm disabled:opacity-40"
        >
          Print
        </button>
        {pending && <span className="text-sm text-zinc-500">…</span>}
      </div>

      {error && (
        <p className="text-sm text-red-700">
          {error.kind}: {error.message}
        </p>
      )}

      {result && (
        <section className="space-y-3">
          <h2 className="font-semibold">
            Preview — {result.labels.length} label{result.labels.length === 1 ? "" : "s"} ·{" "}
            {result.size_mm}mm · chars {result.chars}
          </h2>
          {result.warning != null && <p className="text-sm text-amber-700">{result.warning}</p>}
          <ul className="flex flex-wrap gap-4">
            {result.labels.map((label) => (
              <li key={label.id} className="space-y-1">
                <div
                  className="border border-zinc-200 bg-white p-1"
                  // SVG markup comes from the engine/codec (or the
                  // mock's placeholder renderer) — trusted output.
                  dangerouslySetInnerHTML={{ __html: label.svg }}
                />
                <p className="font-mono text-xs text-zinc-500">{label.id}</p>
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}
