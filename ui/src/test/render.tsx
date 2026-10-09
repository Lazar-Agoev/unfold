/** Renders a component with a fresh TanStack Query client configured like the app's. */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, type RenderResult } from "@testing-library/react";
import type { ReactNode } from "react";

export function testQueryClient(): QueryClient {
  return new QueryClient({ defaultOptions: { queries: { refetchOnWindowFocus: false, retry: false, staleTime: Infinity } } });
}

export function renderWithClient(ui: ReactNode, client: QueryClient = testQueryClient()): RenderResult & { client: QueryClient } {
  return { ...render(<QueryClientProvider client={client}>{ui}</QueryClientProvider>), client };
}
