import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import App from "./App";
import { TransportProvider } from "./data/TransportContext";
import { transportFromEnv } from "./transport";
import "./index.css";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { retry: 1, refetchOnWindowFocus: false },
  },
});

const root = document.getElementById("root");
if (!root) throw new Error("missing #root element");

createRoot(root).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <TransportProvider transport={transportFromEnv()}>
        <App />
      </TransportProvider>
    </QueryClientProvider>
  </StrictMode>,
);
