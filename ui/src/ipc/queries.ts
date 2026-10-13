/**
 * Engine data in TanStack Query: one key shape for every command (`["engine", command, args]`), a hook
 * to read it, and the invalidation engine events drive (docs/ipc.md).
 *
 * Every project command's arguments carry `project`, so each event touches only the queries of its
 * project (and the cross-project `search` that can include it). Nothing is
 * refetched wholesale except `graph_updated.everything` (that project only) and the engine-restart
 * reset (`resetEngineQueries`), which the window's session code orders.
 */

import { useQuery, type Query, type QueryClient, type UseQueryResult } from "@tanstack/react-query";
import { call, onEngineEvent } from "./commands";
import type { CommandArgs, CommandName, CommandResult, EventType, ProjectId, WorkspaceEvent, WorkspaceView } from "./generated";
import type { UnfoldError } from "./unfold";

/** The query key of one command call. */
export type EngineKey<C extends CommandName = CommandName> = readonly ["engine", C, CommandArgs<C>];

export function engineKey<C extends CommandName>(command: C, args: CommandArgs<C>): EngineKey<C> {
  return ["engine", command, args];
}

/** Query options for one command call, for `useQueries`, `fetchQuery`, and `ensureQueryData`. */
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
    ...engineQueryOptions(command, args),
    enabled: options.enabled ?? true,
  });
}

type Args = Readonly<Record<string, unknown>>;

function commandOf(query: Query): string | null {
  const [scope, command] = query.queryKey;
  return scope === "engine" && typeof command === "string" ? command : null;
}

function argsOf(query: Query): Args {
  const args = query.queryKey[2];
  return typeof args === "object" && args !== null ? (args as Args) : {};
}

function text(args: Args, name: string): string {
  const value = args[name];
  return typeof value === "string" ? value : "";
}

/** Whether a cross-project query (`search`) covered `project`: all projects, or a list naming it. */
function includes(args: Args, project: ProjectId): boolean {
  const projects = args["projects"];
  return !Array.isArray(projects) || projects.includes(project);
}

/** Marks stale (and refetches if observed) the engine queries `test` picks, in one pass over the cache. */
function invalidate(client: QueryClient, test: (command: string, args: Args) => boolean): void {
  void client.invalidateQueries({
    predicate: (query) => {
      const command = commandOf(query);
      return command !== null && test(command, argsOf(query));
    },
  });
}

const ENTRANCE_PREFIX = "entrance:";

/**
 * The file a block key belongs to (`src/a.ts#function:f` → `src/a.ts`): up to its last `#`, since a
 * path may hold `#` and a step never does. An entrance's key is its own.
 */
export function keyFile(key: string): string {
  const hash = key.startsWith(ENTRANCE_PREFIX) ? -1 : key.lastIndexOf("#");
  return hash < 0 ? key : key.slice(0, hash);
}

/** Commands that don't depend on any project; the engine-restart reset leaves them alone. */
const APP_COMMANDS: ReadonlySet<string> = new Set<CommandName>(["get_settings", "app_info"]);

const WORKSPACE_KEY = engineKey("workspace", {});

/** Applies one engine event to the cache (events that aren't about the cache are ignored). */
export function applyEngineEvent(client: QueryClient, event: WorkspaceEvent): void {
  switch (event.type) {
    case "workspace_changed":
      client.setQueryData(WORKSPACE_KEY, event.workspace);
      rereadWorkspaceInFlight(client);
      return;
    case "project_state": {
      const { project, state } = event;
      client.setQueryData<WorkspaceView>(WORKSPACE_KEY, (workspace) =>
        workspace ? { ...workspace, projects: workspace.projects.map((row) => (row.id === project ? { ...row, state } : row)) } : workspace,
      );
      rereadWorkspaceInFlight(client);
      invalidate(client, (command, args) => command === "project_summary" && args["project"] === project);
      return;
    }
    case "project_reset":
      client.removeQueries({ predicate: (query) => commandOf(query) !== null && argsOf(query)["project"] === event.project });
      return;
    case "graph_updated": {
      const { project } = event;
      if (event.everything) {
        invalidate(client, (command, args) => (command === "search" ? includes(args, project) : args["project"] === project));
        void client.invalidateQueries({ queryKey: WORKSPACE_KEY });
        return;
      }
      const files = new Set([...event.changedFiles, ...event.removedFiles]);
      const keys = new Set([...event.changedKeys, ...event.changedEntrances]);
      const byKey = (key: string) => keys.has(key) || files.has(keyFile(key));
      const entrances = event.changedEntrances.length > 0 || event.inputsChanged;
      // Typed lines (`line_labels`, `call_contract`, `type_definition`, `type_uses`) read the
      // callee's and the types' files as well as the caller's, so any change refreshes them.
      invalidate(client, (command, args) => {
        if (command === "search") {
          return includes(args, project);
        }
        if (args["project"] !== project) {
          return false;
        }
        switch (command) {
          case "file_view":
            return files.has(text(args, "path"));
          case "card":
            // An entrance card also lists the applications it belongs to, which any file can change.
            return byKey(text(args, "key")) || text(args, "key").startsWith(ENTRANCE_PREFIX);
          case "peek":
            return byKey(text(args, "key"));
          case "relations":
          case "callers":
          case "preview":
          case "project_summary":
          case "line_labels":
          case "call_contract":
          case "type_definition":
          case "type_uses":
            return true;
          case "entrances":
            return entrances;
          default:
            return false;
        }
      });
      return;
    }
    case "files_changed": {
      const { project, truncated } = event;
      const paths = new Set(event.paths);
      invalidate(client, (command, args) => {
        if (command === "search") {
          // The file list behind file search may have gained or lost files.
          const kinds = args["kinds"];
          return includes(args, project) && (!Array.isArray(kinds) || kinds.includes("files"));
        }
        if (args["project"] !== project) {
          return false;
        }
        if (command === "file_view") {
          return truncated || paths.has(text(args, "path"));
        }
        // A card checks its file's bytes before showing them: an edited file shows as changed at once.
        return command === "card" && (truncated || paths.has(keyFile(text(args, "key"))));
      });
      return;
    }
    case "boards_changed":
      invalidate(client, (command, args) => args["project"] === event.project && (command === "list_boards" || command === "project_summary" || command === "board_refs"));
      return;
    case "problems_changed":
      invalidate(client, (command, args) => args["project"] === event.project && (command === "problems" || command === "project_summary"));
      return;
    case "config_changed":
    case "compile_progress":
    case "board_reloaded":
      // Not about the cache: a settings change recompiles (`graph_updated` follows), and the compile and
      // board stores follow the others.
      return;
  }
}

/** The events that change the cache. */
const CACHE_EVENTS: readonly EventType[] = [
  "workspace_changed",
  "project_state",
  "project_reset",
  "graph_updated",
  "files_changed",
  "boards_changed",
  "problems_changed",
];

/**
 * A `workspace` read still in flight may have been answered before the change an event just wrote
 * into the cache (results and events reach the window by different paths), and would put the old
 * state back when it lands. It is cancelled and read again, so the cache ends with the newer state.
 */
function rereadWorkspaceInFlight(client: QueryClient): void {
  if (client.isFetching({ queryKey: WORKSPACE_KEY }) > 0) {
    void client.invalidateQueries({ queryKey: WORKSPACE_KEY });
  }
}

/** Subscribes the cache to engine events. Returns the unsubscribe function. */
export function installEngineInvalidation(client: QueryClient): () => void {
  const unsubscribers = CACHE_EVENTS.map((type) =>
    onEngineEvent(type, (event) => {
      applyEngineEvent(client, event);
    }),
  );
  return () => {
    for (const unsubscribe of unsubscribers) {
      unsubscribe();
    }
  };
}

/**
 * After an engine-host restart: whatever the engine answered before may be out of date
 * (no events arrived while it was down), so every project query is reset and the shown ones fetched
 * again. Settings and app info don't depend on the engine's state and stay.
 */
export function resetEngineQueries(client: QueryClient): Promise<void> {
  return client.resetQueries({
    predicate: (query) => {
      const command = commandOf(query);
      return command !== null && !APP_COMMANDS.has(command);
    },
  });
}
