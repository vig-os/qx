// Thin data layer: TanStack Query over the transport (ADR-030 §3).
// Ground truth and validation live in Rust; this layer only caches
// and unwraps the protocol envelope.

import { keepPreviousData, useQuery } from "@tanstack/react-query";
import type {
  CountData,
  DescribeData,
  Entity,
  ErrorKind,
  Filter,
  ListData,
  Page,
  Request,
  SortSpec,
} from "../protocol";
import type { Transport } from "../transport";
import { useTransport } from "./TransportContext";

/** A protocol-level failure ({ok: false}) surfaced as a typed throw. */
export class TransportError extends Error {
  readonly kind: ErrorKind;
  constructor(kind: ErrorKind, message: string) {
    super(message);
    this.name = "TransportError";
    this.kind = kind;
  }
}

async function dispatch<T>(transport: Transport, req: Request): Promise<T> {
  const res = await transport(req);
  if (!res.ok) throw new TransportError(res.error.kind, res.error.message);
  // The protocol contract fixes the data shape per op; the cast records
  // which shape this call site expects.
  return res.data as T;
}

export function useDescribe() {
  const transport = useTransport();
  return useQuery({
    queryKey: ["describe"],
    queryFn: () => dispatch<DescribeData>(transport, { op: "Describe" }),
  });
}

export interface ListOptions {
  filter?: Filter;
  sort?: SortSpec[];
  page?: Page;
}

export function useList(collection: string, options: ListOptions = {}) {
  const transport = useTransport();
  return useQuery({
    queryKey: ["list", collection, options],
    queryFn: () => dispatch<ListData>(transport, { op: "List", collection, ...options }),
    placeholderData: keepPreviousData,
  });
}

export function useCount(collection: string, by: string, filter?: Filter) {
  const transport = useTransport();
  return useQuery({
    queryKey: ["count", collection, by, filter],
    queryFn: () => dispatch<CountData>(transport, { op: "Count", collection, filter, by }),
    placeholderData: keepPreviousData,
  });
}

export function useResolve(id: string) {
  const transport = useTransport();
  return useQuery({
    queryKey: ["resolve", id],
    queryFn: () => dispatch<Entity>(transport, { op: "Resolve", id }),
  });
}
