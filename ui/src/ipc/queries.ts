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
import type { CardView, CodeView, CommandArgs, CommandName, CommandResult, DrawnFlow, EventType, ProjectId, Sequence, WorkspaceEvent, WorkspaceView } from "./generated";
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
function invalidate(client: QueryClient, test: (command: string, args: Args, query: Query) => boolean): void {
  void client.invalidateQueries({
    predicate: (query) => {
      const command = commandOf(query);
      return command !== null && test(command, argsOf(query), query);
    },
  });
}

/** The files the flows of a sequence open in place draw. */
function sequenceFiles(sequence: Sequence): string[] {
  return sequence.steps.flatMap((step) => {
    switch (step.kind) {
      case "call":
        return [...(step.inset ? flowFiles(step.inset) : []), ...step.callbacks.flatMap((callback) => (callback.inset ? flowFiles(callback.inset) : []))];
      case "decision":
      case "loop":
      case "try":
      case "with":
        return step.lanes.flatMap((lane) => sequenceFiles(lane.steps));
      case "combined":
        return step.steps ? sequenceFiles(step.steps) : [];
      case "thin":
      case "exit":
      case "proposed":
        return [];
    }
  });
}

/** The files a drawn flow draws: its definition's, and those of the calls it opens in place. */
function flowFiles(flow: DrawnFlow): string[] {
  return [flow.header.path, ...sequenceFiles(flow.steps)];
}

/** The files a card draws, as its own data says (a start card: the files of the blocks it starts at). */
function cardFiles(view: CardView): string[] {
  return view.type === "definition" ? flowFiles(view.flow) : view.code.flatMap((code) => (code.path !== undefined ? [code.path] : []));
}

/**
 * Whether a cached card draws something `changed` picks: one of its files, or, until it has data (still
 * loading, or missing from the graph), anything.
 */
function cardTouches(query: Query, changed: (path: string) => boolean): boolean {
  const view = query.state.data as CardView | undefined;
  return view === undefined || cardFiles(view).some(changed);
}

/** Whether a cached code panel view shows a file `changed` picks (or isn't loaded yet). */
function codeTouches(query: Query, changed: (path: string) => boolean): boolean {
  const file = (query.state.data as CodeView | undefined)?.file;
  return file === undefined || changed(file);
}

/** Whether a cached card is an app's or entrance's start card (or isn't loaded yet, so may be one). */
function startCard(query: Query): boolean {
  return (query.state.data as CardView | undefined)?.type !== "definition";
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
      const keys = new Set(event.changedKeys);
      // A call's contract and types (`call_contract`, `type_definition`, `type_uses`) read the
      // callee's and the types' files as well as the caller's, so any change refreshes them.
      invalidate(client, (command, args, query) => {
        if (command === "search") {
          return includes(args, project);
        }
        if (args["project"] !== project) {
          return false;
        }
        switch (command) {
          case "card":
            // A start card's code is found, missing, or stale by the compile, wherever it is.
            return keys.has(text(args, "key")) || startCard(query) || cardTouches(query, (path) => files.has(path));
          case "relations":
          case "callers":
          case "project_summary":
          case "call_contract":
          case "type_definition":
          case "type_uses":
          case "entrances":
          case "code_view":
            // The code panel crops again once the compile caught up with a changed file.
            return true;
          default:
            return false;
        }
      });
      return;
    }
    case "files_changed": {
      const { project, truncated } = event;
      const paths = new Set(event.paths);
      invalidate(client, (command, args, query) => {
        if (command === "search") {
          // The file list behind file search may have gained or lost files.
          const kinds = args["kinds"];
          return includes(args, project) && (!Array.isArray(kinds) || kinds.includes("files"));
        }
        if (args["project"] !== project) {
          return false;
        }
        // A card and the code panel check their files' bytes before showing them: an edited file shows
        // as changed at once.
        switch (command) {
          case "card":
            return truncated || cardTouches(query, (path) => paths.has(path));
          case "code_view":
            return truncated || codeTouches(query, (path) => paths.has(path));
          default:
            return false;
        }
      });
      return;
    }
    case "boards_changed":
      invalidate(client, (command, args) => args["project"] === event.project && (command === "list_boards" || command === "project_summary" || command === "board_refs"));
      return;
    case "problems_changed":
      invalidate(client, (command, args) => args["project"] === event.project && (command === "problems" || command === "project_summary"));
      return;
    case "entrances_changed":
      // `[[app]]` tables re-resolved without a compile: what lists or draws apps and entrances.
      invalidate(client, (command, args, query) => {
        if (command === "search") {
          return includes(args, event.project);
        }
        if (args["project"] !== event.project) {
          return false;
        }
        switch (command) {
          case "card":
            return startCard(query);
          case "entrances":
          case "project_summary":
          case "relations":
          case "code_view":
            // The code panel may show an app's or entrance's lines in `unfold.toml`.
            return true;
          default:
            return false;
        }
      });
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
  "entrances_changed",
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
