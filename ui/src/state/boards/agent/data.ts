/**
 * What an agent's edit reads from the engine before it applies: card models and type definitions
 * (through the window's query cache, so the cards draw from them at once), a definition's outgoing
 * calls, and its callers.
 * Everything is loaded first and read synchronously while the operations apply, so the edit can apply
 * again over the person's edits without asking the engine twice.
 */

import type { AgentFollowStop } from "@unfold/desktop/agent-api";
import { queryClient } from "../../../ipc/client";
import type { Callers, CardView, Key, OutgoingView, ProjectId, StartTarget, TargetState, TypeDefinition } from "../../../ipc/generated";
import { engineQueryOptions } from "../../../ipc/queries";
import { cardQuery, isMissing } from "../../cards";
import { call } from "../engine";
import { cardRefKey } from "../keys";

/** How many callers one card lists when following callers (the engine's default page). */
const CALLERS_LIMIT = 20;

/**
 * A definition's outgoing calls as following sees them: where project calls go, and the ones it can't
 * follow. For an app or entrance, the code it starts at, as tracing it by hand opens it.
 */
interface FollowableCalls {
  /** One per target, in source order. */
  readonly resolved: readonly { readonly target: Key }[];
  /** Library and unknown calls (one per written call), or a start's code that names no start, where following stops. */
  readonly blocked: readonly AgentFollowStop[];
}

/** A library or unknown call as a stop. */
function blockedStop(call: Pick<OutgoingView, "to" | "text" | "line">): AgentFollowStop {
  return {
    reason: call.to.outcome === "library" ? "external" : "unresolved",
    call: call.text,
    line: call.line,
    ...(call.to.note !== undefined ? { note: call.to.note } : {}),
  };
}

/** Keeps the first entry for each key. */
function uniqueBy<T>(items: readonly T[], key: (item: T) => string): T[] {
  const seen = new Set<string>();
  return items.filter((item) => {
    const id = key(item);
    if (seen.has(id)) {
      return false;
    }
    seen.add(id);
    return true;
  });
}

/** Why following can't go through a start's code key, by its state; found and stale code is followed. */
const UNFOLLOWED: Readonly<Partial<Record<TargetState, string>>> = {
  missing: "no block has this key",
  not_analyzed: "its file isn't analyzed",
  not_a_start: "not a start",
};

/** Where a start leads: its code that is found or stale (its last good block), each once, and the rest as stops. */
function startCalls(code: readonly StartTarget[]): FollowableCalls {
  return {
    resolved: uniqueBy(
      code.flatMap((target) => (UNFOLLOWED[target.state] === undefined ? [{ target: target.key }] : [])),
      (item) => item.target,
    ),
    blocked: uniqueBy(
      code.flatMap((target) => {
        const note = UNFOLLOWED[target.state];
        return note === undefined ? [] : [{ reason: "unresolved" as const, call: target.key, note }];
      }),
      (stop) => stop.call,
    ),
  };
}

export class AgentData {
  private readonly models = new Map<string, CardView | null>();
  private readonly definitions = new Map<string, TypeDefinition | null>();
  private readonly calls = new Map<string, FollowableCalls>();
  private readonly callerLists = new Map<string, Callers>();

  /** Loads a card's model; null when the key isn't in the project's graph. */
  async loadModel(project: ProjectId, key: Key): Promise<CardView | null> {
    const ref = cardRefKey(project, key);
    if (!this.models.has(ref)) {
      try {
        this.models.set(ref, await queryClient.query(cardQuery({ project, key })));
      } catch (error) {
        if (!isMissing(error)) {
          throw error;
        }
        this.models.set(ref, null);
      }
    }
    return this.models.get(ref) ?? null;
  }

  /** A loaded model (undefined when it wasn't loaded, or the key isn't in the graph). */
  model(project: ProjectId, key: Key): CardView | undefined {
    return this.models.get(cardRefKey(project, key)) ?? undefined;
  }

  /** A model as loaded: null when the key isn't in the graph, undefined when it wasn't loaded yet. */
  loaded(project: ProjectId, key: Key): CardView | null | undefined {
    return this.models.get(cardRefKey(project, key));
  }

  /** Every loaded model by `cardRefKey(project, key)`, as arrows are computed from them. */
  loadedModels(): Map<string, CardView> {
    return new Map([...this.models].flatMap(([ref, model]) => (model ? [[ref, model] as const] : [])));
  }

  /** Loads a type card's definition; null when the key isn't a type declaration or class in the project's graph. */
  async loadDefinition(project: ProjectId, key: Key): Promise<TypeDefinition | null> {
    const ref = cardRefKey(project, key);
    if (!this.definitions.has(ref)) {
      try {
        this.definitions.set(ref, await queryClient.query(engineQueryOptions("type_definition", { project, key })));
      } catch (error) {
        if (!isMissing(error)) {
          throw error;
        }
        this.definitions.set(ref, null);
      }
    }
    return this.definitions.get(ref) ?? null;
  }

  /** A loaded type definition (undefined when it wasn't loaded, or the key isn't a type). */
  definition(project: ProjectId, key: Key): TypeDefinition | undefined {
    return this.definitions.get(cardRefKey(project, key)) ?? undefined;
  }

  /** Every loaded type definition by `cardRefKey(project, key)`, as type arrows are computed from them. */
  loadedDefinitions(): Map<string, TypeDefinition> {
    return new Map([...this.definitions].flatMap(([ref, definition]) => (definition ? [[ref, definition] as const] : [])));
  }

  /** What following goes through from a definition, or an app or entrance (the code it starts at). */
  async followableCalls(project: ProjectId, key: Key): Promise<FollowableCalls> {
    const ref = cardRefKey(project, key);
    const known = this.calls.get(ref);
    if (known) {
      return known;
    }
    const model = await this.loadModel(project, key);
    let calls: FollowableCalls;
    if (model?.type === "start") {
      calls = startCalls(model.code);
    } else {
      const { outgoing } = await call("relations", { project, key });
      const followed = outgoing.filter((edge) => edge.kind === "calls").sort((a, b) => a.line - b.line);
      calls = {
        resolved: uniqueBy(
          followed.flatMap((edge) => (edge.to.outcome === "project" && edge.to.key !== undefined ? [{ target: edge.to.key }] : [])),
          (item) => item.target,
        ),
        blocked: uniqueBy(
          followed.filter((edge) => edge.to.outcome === "library" || edge.to.outcome === "unknown"),
          (edge) => `${edge.to.outcome}\u0000${edge.text}`,
        ).map(blockedStop),
      };
    }
    this.calls.set(ref, calls);
    return calls;
  }

  /** The first callers of a definition (`callers`, 20 at most) and how many there are. */
  async callers(project: ProjectId, key: Key): Promise<Callers> {
    const ref = cardRefKey(project, key);
    let callers = this.callerLists.get(ref);
    if (!callers) {
      callers = await call("callers", { project, key, limit: CALLERS_LIMIT });
      this.callerLists.set(ref, callers);
    }
    return callers;
  }
}
