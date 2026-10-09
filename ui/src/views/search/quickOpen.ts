/**
 * Quick open (⌘P files, ⌘T symbols) over the `search` command, registered while a folder is open.
 * The palette shows the results as ranked; picking one opens a source tab or a card.
 */

import { useQueryClient } from "@tanstack/react-query";
import { useMemo } from "react";
import { engineQueryOptions } from "../../ipc/queries";
import { useQuickOpenProvider, type QuickOpenItem, type QuickOpenProvider } from "../../palette";
import { openBlockCard } from "../../state/boards/api";
import { openSource } from "../../state/tabs";

const LIMIT = 50;

export function useSearchQuickOpen(): void {
  const client = useQueryClient();

  const files = useMemo<QuickOpenProvider>(
    () => ({
      placeholder: "Go to file",
      emptyMessage: "No files match.",
      search: async (query, signal) => {
        if (query.trim() === "") {
          return [];
        }
        const results = await client.query(engineQueryOptions("search", { query, kinds: ["files"], limit: LIMIT }));
        if (signal.aborted) {
          return [];
        }
        return results.files.map(
          (hit): QuickOpenItem => ({
            id: hit.path,
            label: hit.name,
            detail: hit.path,
            run: () => {
              openSource(hit.path);
            },
          }),
        );
      },
    }),
    [client],
  );

  const symbols = useMemo<QuickOpenProvider>(
    () => ({
      placeholder: "Go to symbol",
      emptyMessage: "No definitions match.",
      search: async (query, signal) => {
        if (query.trim() === "") {
          return [];
        }
        const results = await client.query(engineQueryOptions("search", { query, kinds: ["definitions"], limit: LIMIT }));
        if (signal.aborted) {
          return [];
        }
        return results.definitions.map(
          (hit): QuickOpenItem => ({
            id: hit.key,
            label: hit.qualifiedName,
            detail: hit.line === null ? hit.path : `${hit.path}:${hit.line}`,
            run: () => {
              openBlockCard(hit.key);
            },
          }),
        );
      },
    }),
    [client],
  );

  useQuickOpenProvider("files", files);
  useQuickOpenProvider("symbols", symbols);
}
