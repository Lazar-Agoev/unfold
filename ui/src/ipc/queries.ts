/**
 * Engine data in TanStack Query: one key shape for every command (`["engine", command, args]`),
 * a hook to read it, and the precise invalidation engine events drive (architecture §11.8).
 *
 * Invalidation is per event, never "refetch everything" unless the engine says so
 * (`graph_updated.everything`):
 * - `graph_updated`: the changed files' outlines, views, tree levels, cards, and peeks, plus every
 *   entrance card (its applications can change with any file); the entrance tree when entrances or
 *   inputs changed; name search, relations, and the Index log.
 * - `files_changed`: the changed files' views, tree levels (pending marks), and cards (their source
 *   is now "changed"), and file search.
 * - `boards_changed`: the board list. `config_changed`: tree levels (skip marks), the summary, and
 *   the settings. `problems_changed`: Problems. `graph_status`: written straight into the cache.
 */

import { useQuery, type Query, type QueryClient, type UseQueryResult } from "@tanstack/react-query";
import { call, onEngineEvent } from "./commands";
import type { CommandArgs, CommandName, CommandResult, Events } from "./generated";
import { onConnection, type UnfoldError } from "./unfold";

/** The query key of one command call. */
export type EngineKey<C extends CommandName = CommandName> = readonly ["engine", C, CommandArgs<C>];

export function engineKey<C extends CommandName>(command: C, args: CommandArgs<C>): EngineKey<C> {
  return ["engine", command, args];
}

/** The key prefix of every call of one command (for invalidation). */
export function commandKey(command: CommandName): readonly ["engine", CommandName] {
  return ["engine", command];
}

/** Query options for one command call, for `useQueries` and `fetchQuery`. */
export function engineQueryOptions<C extends CommandName>(command: C, args: CommandArgs<C>) {
  return {
    queryKey: engineKey(command, args),
    queryFn: () => call(command, args),
  };
}

export interface EngineQueryOptions {
  readonly enabled?: boolean;
}

/** Reads one command's result, cached under its key and refreshed by engine events. */
export function useEngineQuery<C extends CommandName>(
  command: C,
  args: CommandArgs<C>,
  options: EngineQueryOptions = {},
): UseQueryResult<CommandResult<C>, UnfoldError> {
  return useQuery<CommandResult<C>, UnfoldError, CommandResult<C>, EngineKey<C>>({
    queryKey: engineKey(command, args),
    queryFn: () => call(command, args),
    enabled: options.enabled ?? true,
  });
}

/** Commands whose results don't depend on the open folder; they survive a folder change. */
const APP_COMMANDS: ReadonlySet<CommandName> = new Set<CommandName>(["recent_folders", "get_preferences", "app_info"]);

function isFolderQuery(query: Query): boolean {
  return query.queryKey[0] === "engine" && !APP_COMMANDS.has(query.queryKey[1] as CommandName);
}

/** Drops every folder-dependent result (the window now shows another folder, or none). */
export function clearFolderQueries(client: QueryClient): void {
  client.removeQueries({ predicate: isFolderQuery });
}

function argsOf(query: Query): Readonly<Record<string, unknown>> {
  const args = query.queryKey[2];
  return typeof args === "object" && args !== null ? (args as Readonly<Record<string, unknown>>) : {};
}

function stringArg(query: Query, name: string): string | null {
  const value = argsOf(query)[name];
  return typeof value === "string" ? value : null;
}

/** Invalidates calls of `command` whose arguments pass `test` (all calls without one). */
function invalidate(
  client: QueryClient,
  command: CommandName,
  test?: (query: Query) => boolean,
): void {
  void client.invalidateQueries({
    queryKey: commandKey(command),
    ...(test ? { predicate: test } : {}),
  });
}

/** Every folder from the root down to `path`'s parent: the tree levels whose marks it affects. */
export function ancestorDirs(path: string): string[] {
  const dirs = [""];
  let index = path.indexOf("/");
  while (index >= 0) {
    dirs.push(path.slice(0, index));
    index = path.indexOf("/", index + 1);
  }
  return dirs;
}

const ENTRANCE_PREFIX = "entrance:";

/** The file a block key belongs to (`src/a.ts#function:f` → `src/a.ts`). */
export function keyFile(key: string): string {
  const hash = key.indexOf("#");
  return hash < 0 ? key : key.slice(0, hash);
}

function invalidateFiles(client: QueryClient, paths: readonly string[]): void {
  const files = new Set(paths);
  const dirs = new Set(paths.flatMap(ancestorDirs));
  const byPath = (query: Query) => files.has(stringArg(query, "path") ?? "\u0000");
  invalidate(client, "file_view", byPath);
  invalidate(client, "list_dir", (query) => dirs.has(stringArg(query, "path") ?? "\u0000"));
}

/** Applies one `graph_updated` event to the cache. */
export function applyGraphUpdated(client: QueryClient, event: Events["graph_updated"]): void {
  if (event.everything) {
    void client.invalidateQueries({ predicate: isFolderQuery });
    return;
  }
  const paths = [...event.changedFiles, ...event.removedFiles];
  const files = new Set(paths);
  const keys = new Set([...event.changedKeys, ...event.changedEntrances]);
  invalidateFiles(client, paths);
  invalidate(client, "file_outline", (query) => files.has(stringArg(query, "path") ?? "\u0000"));
  const byKey = (query: Query) => {
    const key = stringArg(query, "key");
    return key !== null && (keys.has(key) || files.has(keyFile(key)));
  };
  // An entrance card also lists the applications it belongs to, which any file can change.
  invalidate(client, "card", (query) => byKey(query) || (stringArg(query, "key")?.startsWith(ENTRANCE_PREFIX) ?? false));
  invalidate(client, "peek", byKey);
  if (event.changedEntrances.length > 0 || event.inputsChanged) {
    invalidate(client, "entrance_tree");
  }
  // Callers and reached-from can change with any file; names can appear anywhere.
  invalidate(client, "relations");
  invalidate(client, "search");
  invalidate(client, "index_log");
}

/** Applies one `files_changed` event to the cache. */
export function applyFilesChanged(client: QueryClient, event: Events["files_changed"]): void {
  if (event.truncated) {
    invalidate(client, "file_view");
    invalidate(client, "list_dir");
    invalidate(client, "card");
  } else {
    invalidateFiles(client, event.paths);
    // A card checks its file's bytes before showing them: an edited file shows as changed at once.
    const files = new Set(event.paths);
    invalidate(client, "card", (query) => files.has(keyFile(stringArg(query, "key") ?? "")));
  }
  // The file list behind file search may have gained or lost files.
  invalidate(client, "search", (query) => {
    const kinds = argsOf(query)["kinds"];
    return !Array.isArray(kinds) || kinds.includes("files");
  });
}

/**
 * After an engine-host restart the engine reopened the folder from its cache: whatever it answered
 * before may be out of date (no events arrived while it was down), and requests cut off by the
 * restart failed with `disconnected`. Every folder query is asked again.
 */
function refetchAfterRestart(client: QueryClient): () => void {
  let lost = false;
  return onConnection((state) => {
    if (state === "reconnecting" || state === "unavailable") {
      lost = true;
    } else if (state === "connected" && lost) {
      lost = false;
      void client.invalidateQueries({ predicate: isFolderQuery });
    }
  });
}

/** Subscribes the cache to engine events (and to engine-host restarts). Returns the unsubscribe function. */
export function installEngineInvalidation(client: QueryClient): () => void {
  const unsubscribers = [
    onEngineEvent("graph_updated", (event) => {
      applyGraphUpdated(client, event);
    }),
    onEngineEvent("files_changed", (event) => {
      applyFilesChanged(client, event);
    }),
    onEngineEvent("boards_changed", () => {
      invalidate(client, "list_boards");
    }),
    onEngineEvent("config_changed", () => {
      invalidate(client, "list_dir");
      invalidate(client, "folder_summary");
      invalidate(client, "get_folder_config");
    }),
    onEngineEvent("problems_changed", () => {
      invalidate(client, "problems");
    }),
    onEngineEvent("graph_status", (event) => {
      client.setQueryData(engineKey("graph_status", {}), event.status);
    }),
    // Another window (or this one) saved the preferences. The event carries them, so the cache is
    // replaced without a refetch; the shell's preference hooks then reapply the theme and bindings.
    onEngineEvent("preferences_changed", (event) => {
      client.setQueryData(engineKey("get_preferences", {}), event.preferences);
    }),
    refetchAfterRestart(client),
  ];
  return () => {
    for (const unsubscribe of unsubscribers) {
      unsubscribe();
    }
  };
}
