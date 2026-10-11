/**
 * What an agent's edit reads from the engine before it applies: card models (through the window's
 * query cache, so the cards draw from them at once), a definition's outgoing calls, and its callers.
 * Everything is loaded first and read synchronously while the operations apply, so the edit can apply
 * again over the person's edits without asking the engine twice.
 */

import type { AgentFollowStop } from "@unfold/desktop/agent-api";
import { cardQuery, isMissing } from "../../../canvas/cardData";
import { queryClient } from "../../../ipc/client";
import type { Callers, CardView, EdgeKind, Key, OutgoingCall, ProjectId, Resolution } from "../../../ipc/generated";
import { call } from "../engine";
import { cardRefKey } from "../keys";

/** The calls following goes through: calls and constructions (not imports or passed references). */
const FOLLOWED: ReadonlySet<EdgeKind> = new Set(["calls", "constructs"]);
/** How many callers one card lists when following callers (the engine's default page). */
const CALLERS_LIMIT = 20;

/** A definition's outgoing calls as following sees them: where resolved calls go, and the ones it can't follow. */
interface FollowableCalls {
  /** One per target, in source order, with the first calling line. */
  readonly resolved: readonly { readonly target: Key; readonly line?: number }[];
  /** External and unresolved calls (one per written call), where following stops. */
  readonly blocked: readonly AgentFollowStop[];
}

/** An external or unresolved call as a stop. */
function blockedStop(call: {
  readonly resolution: Resolution;
  readonly text: string;
  readonly note?: string | null;
  readonly line?: number | null;
}): AgentFollowStop {
  return {
    reason: call.resolution === "external" ? "external" : "unresolved",
    call: call.text,
    ...(call.line !== undefined && call.line !== null ? { line: call.line } : {}),
    ...(call.note !== undefined && call.note !== null ? { note: call.note } : {}),
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

export class AgentData {
  private readonly models = new Map<string, CardView | null>();
  private readonly calls = new Map<string, FollowableCalls>();
  private readonly callerLists = new Map<string, Callers>();

  /** Loads a card's model; null when the key isn't in the project's graph. */
  async loadModel(project: ProjectId, key: Key): Promise<CardView | null> {
    const ref = cardRefKey(project, key);
    if (!this.models.has(ref)) {
      try {
        this.models.set(ref, await queryClient.query(cardQuery(project, key)));
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
  model(project: ProjectId | null, key: Key): CardView | undefined {
    return project === null ? undefined : (this.models.get(cardRefKey(project, key)) ?? undefined);
  }

  /** Every loaded model by `cardRefKey(project, key)`, as arrows are computed from them. */
  loadedModels(): Map<string, CardView> {
    return new Map([...this.models].flatMap(([ref, model]) => (model ? [[ref, model] as const] : [])));
  }

  /** What following goes through from a definition or an entrance (an entrance's handlers are its calls). */
  async followableCalls(project: ProjectId, key: Key): Promise<FollowableCalls> {
    const ref = cardRefKey(project, key);
    const known = this.calls.get(ref);
    if (known) {
      return known;
    }
    const model = await this.loadModel(project, key);
    let calls: FollowableCalls;
    if (model?.type === "entrance") {
      calls = {
        resolved: uniqueBy(
          model.targets.flatMap((target) => (target.resolution === "resolved" && target.target !== null ? [{ target: target.target }] : [])),
          (item) => item.target,
        ),
        blocked: model.targets.filter((target) => target.resolution !== "resolved").map(blockedStop),
      };
    } else {
      const { outgoing } = await call("relations", { project, key });
      const followed = (edge: OutgoingCall) => FOLLOWED.has(edge.kind);
      calls = {
        resolved: uniqueBy(
          [...outgoing.onBoard, ...outgoing.openable]
            .filter(followed)
            .sort((a, b) => a.line - b.line)
            .flatMap((edge) => (edge.target !== undefined ? [{ target: edge.target, line: edge.line }] : [])),
          (item) => item.target,
        ),
        blocked: uniqueBy(outgoing.externalOrUnresolved.filter(followed), (edge) => `${edge.resolution}\u0000${edge.text}`).map(blockedStop),
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
