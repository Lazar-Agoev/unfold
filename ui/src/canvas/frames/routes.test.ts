import { describe, expect, it } from "vitest";
import type { Connection, ConnectionKind } from "../../ipc/generated";
import { NODE_PORT, type Arrow } from "../arrows";
import { routeArrows, routeLinks } from "./routes";

function arrow(source: string, port: string, target: string, extra: Partial<Arrow> = {}): Arrow {
  return { id: `${source}|${port}|${target}`, source, sourceHandle: port, target, project: "p", kind: "call", dashed: false, order: 0, calls: [port], ...extra };
}

/** `checks` (collapsed) holds a and b; `api` (collapsed) holds c. */
const SHOWN = new Map([
  ["a", "checks"],
  ["b", "checks"],
  ["c", "api"],
]);

describe("arrows at collapsed frames", () => {
  it("are the same arrows when nothing is collapsed", () => {
    const arrows = [arrow("x", "s1", "a")];
    expect(routeArrows(arrows, new Map())).toBe(arrows);
  });

  it("attach to the collapsed frame, keeping one call's id and cards", () => {
    const routed = routeArrows([arrow("x", "s1", "a"), arrow("c", "s2", "y")], SHOWN);
    expect(routed).toEqual([
      { ...arrow("x", "s1", "a"), target: "checks", cards: { from: "x", to: "a" } },
      { ...arrow("c", "s2", "y"), source: "api", sourceHandle: NODE_PORT, cards: { from: "c", to: "y" } },
    ]);
  });

  it("aren't drawn inside one collapsed frame, and merge where they would overlap", () => {
    const routed = routeArrows(
      [arrow("a", "s1", "b"), arrow("a", "s1", "c"), arrow("b", "s2", "c", { calls: ["s2", "s3"] }), arrow("b", "field:0", "c", { kind: "type", calls: [] })],
      SHOWN,
    );
    expect(routed).toHaveLength(1);
    expect(routed[0]).toMatchObject({ id: `checks|${NODE_PORT}|api`, source: "checks", target: "api", calls: [], merged: true });
  });
});

describe("connections at collapsed frames", () => {
  const connection = (id: string, from: string, to: string, kind: ConnectionKind = "calls", extra: Partial<Connection> = {}): Connection => ({ id, from, to, kind, author: "agent:claude", ...extra });

  it("attach to the frame, aren't drawn inside it, and merge into one that counts them", () => {
    const routed = routeLinks([connection("one", "x", "a"), connection("inside", "a", "b"), connection("two", "c", "y"), connection("three", "c", "y")], SHOWN);
    expect(routed).toEqual([
      { id: "one", from: "x", to: "checks", link: connection("one", "x", "a"), count: 1, kind: "calls" },
      { id: "api|calls|y", from: "api", to: "y", count: 2, kind: "calls" },
    ]);
  });

  it("stay apart while both ends are drawn as themselves, even between the same two items", () => {
    const links = [connection("w1", "x", "y", "writes", { step: "k#f/call:a" }), connection("w2", "x", "y", "writes", { step: "k#f/call:b" }), connection("d1", "x", "y")];
    expect(routeLinks(links, SHOWN).map((routed) => [routed.id, routed.count])).toEqual([
      ["w1", 1],
      ["w2", 1],
      ["d1", 1],
    ]);
  });

  it("merge only with connections of their kind, which the merged line keeps", () => {
    const routed = routeLinks([connection("q1", "c", "y", "queue"), connection("q2", "c", "y", "queue"), connection("h1", "c", "y", "http")], SHOWN);
    expect(routed).toEqual([
      { id: "api|queue|y", from: "api", to: "y", count: 2, kind: "queue" },
      { id: "h1", from: "api", to: "y", link: connection("h1", "c", "y", "http"), count: 1, kind: "http" },
    ]);
  });
});
