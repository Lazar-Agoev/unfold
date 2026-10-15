/**
 * The page's routed lines (`routePageLines`) for its edges to draw: one store per canvas. The canvas
 * sets them all at once; each edge reads its own by id, and draws again only when its route changed.
 */

import { createContext, useContext, useSyncExternalStore } from "react";
import type { RoutedLine } from "./route";

function sameRoute(a: RoutedLine, b: RoutedLine): boolean {
  return a.from === b.from && a.to === b.to && JSON.stringify(a.points) === JSON.stringify(b.points);
}

export class LineRoutes {
  private routes: ReadonlyMap<string, RoutedLine> = new Map();
  private readonly listeners = new Set<() => void>();

  /** Sets every line's route. Unchanged routes keep their identity. */
  set(next: ReadonlyMap<string, RoutedLine>): void {
    const kept = new Map<string, RoutedLine>();
    let changed = next.size !== this.routes.size;
    for (const [id, route] of next) {
      const old = this.routes.get(id);
      const same = old !== undefined && sameRoute(old, route);
      kept.set(id, same ? old : route);
      changed ||= !same;
    }
    if (!changed) {
      return;
    }
    this.routes = kept;
    for (const listener of this.listeners) {
      listener();
    }
  }

  route(id: string): RoutedLine | undefined {
    return this.routes.get(id);
  }

  readonly subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
}

export const LineRoutesContext = createContext<LineRoutes | null>(null);

const NO_ROUTES = new LineRoutes();

/** A line's route, kept current; undefined until it is routed. */
export function useLineRoute(id: string): RoutedLine | undefined {
  const routes = useContext(LineRoutesContext) ?? NO_ROUTES;
  return useSyncExternalStore(routes.subscribe, () => routes.route(id));
}
