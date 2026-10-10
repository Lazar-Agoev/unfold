/**
 * Thumbnails for a list of boards (plan §9): `board_thumbnails` in chunks of at most 60 ids per
 * project, aligned to the list's order, each fetched only once one of its boards is rendered.
 */

import { useQueries } from "@tanstack/react-query";
import { useCallback, useMemo } from "react";
import { engineQueryOptions, type ProjectId, type Thumbnail } from "../../ipc";

const CHUNK = 60;

export interface ThumbnailRef {
  readonly project: ProjectId;
  readonly id: string;
}

interface Chunk {
  readonly project: ProjectId;
  readonly boards: string[];
  /** Positions of its boards in the list. */
  readonly indexes: number[];
}

/** Splits the list into per-project chunks of at most 60, in list order. */
export function thumbnailChunks(refs: readonly ThumbnailRef[]): Chunk[] {
  const chunks: Chunk[] = [];
  const open = new Map<ProjectId, Chunk>();
  refs.forEach((ref, index) => {
    let chunk = open.get(ref.project);
    if (!chunk || chunk.boards.length === CHUNK) {
      chunk = { project: ref.project, boards: [], indexes: [] };
      chunks.push(chunk);
      open.set(ref.project, chunk);
    }
    chunk.boards.push(ref.id);
    chunk.indexes.push(index);
  });
  return chunks;
}

const thumbKey = (project: ProjectId, id: string) => `${project}\u0000${id}`;

/**
 * The thumbnails of the boards in `refs` whose chunk has a board in `rendered` (list positions,
 * inclusive start, exclusive end), as a lookup by board.
 */
export function useThumbnails(refs: readonly ThumbnailRef[], rendered: readonly [number, number]): (ref: ThumbnailRef) => Thumbnail | undefined {
  const chunks = useMemo(() => thumbnailChunks(refs), [refs]);
  const [start, end] = rendered;
  const combine = useCallback(
    (results: readonly { readonly data?: { readonly thumbnails: readonly Thumbnail[] } | undefined }[]) => {
      const map = new Map<string, Thumbnail>();
      results.forEach((result, index) => {
        const chunk = chunks[index];
        if (result.data && chunk) {
          for (const thumbnail of result.data.thumbnails) {
            map.set(thumbKey(chunk.project, thumbnail.board), thumbnail);
          }
        }
      });
      return map;
    },
    [chunks],
  );
  const byBoard = useQueries({
    queries: chunks.map((chunk) => ({
      ...engineQueryOptions("board_thumbnails", { project: chunk.project, boards: chunk.boards }),
      enabled: chunk.indexes.some((index) => index >= start && index < end),
    })),
    combine,
  });
  return (ref) => byBoard.get(thumbKey(ref.project, ref.id));
}
