import { createContext, useContext, type ReactNode } from "react";
import type { Transport } from "../transport";

const TransportContext = createContext<Transport | null>(null);

export function TransportProvider({
  transport,
  children,
}: {
  transport: Transport;
  children: ReactNode;
}) {
  return <TransportContext.Provider value={transport}>{children}</TransportContext.Provider>;
}

export function useTransport(): Transport {
  const transport = useContext(TransportContext);
  if (!transport) throw new Error("useTransport: no TransportProvider in the tree");
  return transport;
}
