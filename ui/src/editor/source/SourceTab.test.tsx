// @vitest-environment jsdom
import "../../test/dom";
import { fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { CallLink, CodeLine, CodeSpan, FileView, FileViewArgs, GraphMatch } from "../../ipc/generated";
import type { SourceTab as SourceTabModel } from "../../state/tabs";
import { installFakeEngine, type FakeEngine } from "../../test/engine";
import { renderWithClient } from "../../test/render";
import { SourceTab } from "./SourceTab";

const boards = vi.hoisted(() => ({
  openBlockCard: vi.fn<(key: string) => void>(),
  keys: new Set<string>(["big.ts#function:f2"]),
}));

vi.mock("../../state/boards/api", () => ({
  openBlockCard: boards.openBlockCard,
  useActiveBoardKeys: () => boards.keys,
}));

const LINES = 5000;
const lineOf = (n: number) => `const v${n} = f${n}();`;

/** A 5,000-line file served in pages, like the engine's `file_view`. */
function fileView(graph: GraphMatch) {
  const starts: number[] = [0];
  for (let n = 1; n <= LINES; n += 1) {
    starts.push((starts[n - 1] ?? 0) + lineOf(n).length + 1);
  }
  return ({ path, range }: FileViewArgs): FileView => {
    const first = Math.max(1, range?.startLine ?? 1);
    const last = Math.min(LINES, range?.endLine ?? LINES);
    const lines: CodeLine[] = [];
    const spans: CodeSpan[] = [];
    const links: CallLink[] = [];
    let text = "";
    for (let n = first; n <= last; n += 1) {
      const start = starts[n - 1] ?? 0;
      lines.push({ number: n, start, end: start + lineOf(n).length, indent: 0 });
      spans.push([start, start + 5, "keyword"]);
      const callee = start + lineOf(n).indexOf(`f${n}`);
      links.push({
        source: `${path}#variable:v${n}/call[0]`,
        owner: `${path}#`,
        kind: "calls",
        resolution: "resolved",
        target: `${path}#function:f${n}`,
        text: `f${n}`,
        span: [callee, callee + `f${n}`.length],
        line: n,
      });
      text += `${lineOf(n)}\n`;
    }
    return {
      path,
      language: "typescript",
      graph,
      fileState: "current",
      unshowable: null,
      lineCount: LINES,
      startLine: first,
      endLine: last,
      textStart: starts[first - 1] ?? 0,
      text,
      lines,
      spans,
      outline: [],
      links: graph === "current" ? links : [],
    };
  };
}

const TAB: SourceTabModel = { kind: "source", id: "source:big.ts", path: "big.ts", line: null, reveal: 0 };

let engine: FakeEngine;

function install(graph: GraphMatch) {
  engine = installFakeEngine({
    handlers: {
      file_view: fileView(graph),
      file_outline: () => ({
        path: "big.ts",
        language: "typescript",
        state: "current",
        items: [1, 2].map((n) => ({
          key: `big.ts#function:f${n}`,
          kind: "function" as const,
          name: `f${n}`,
          qualifiedName: `f${n}`,
          startLine: n,
          endLine: n,
          children: [],
        })),
      }),
      peek: ({ key }) => ({ key, kind: "function", name: "f1", qualifiedName: "f1", signature: "function f1() {", path: "big.ts", line: 1, language: "typescript" }),
    },
  });
}

beforeEach(() => {
  boards.openBlockCard.mockClear();
});

afterEach(() => {
  engine.uninstall();
});

describe("SourceTab", () => {
  it("renders only the lines in view, fetching the first page", async () => {
    install("current");
    const { container } = renderWithClient(<SourceTab tab={TAB} active />);
    await screen.findByText("Graph current");
    const rows = container.querySelectorAll(".source-row");
    expect(rows.length).toBeGreaterThan(5);
    expect(rows.length).toBeLessThan(100);
    expect(engine.callsOf("file_view")).toEqual([{ path: "big.ts", range: { startLine: 1, endLine: 200 } }]);
    expect(rows[0]?.textContent).toContain("const v1 = f1();");
  });

  it("draws calls as links that open the target's card", async () => {
    install("current");
    renderWithClient(<SourceTab tab={TAB} active />);
    const [link] = await screen.findAllByRole("link");
    expect(link?.textContent).toBe("f1");
    if (link) {
      fireEvent.click(link);
    }
    expect(boards.openBlockCard).toHaveBeenCalledWith("big.ts#function:f1");
  });

  it("puts ⊕ beside definitions not on the board, and ● beside those that are", async () => {
    install("current");
    renderWithClient(<SourceTab tab={TAB} active />);
    fireEvent.click(await screen.findByRole("button", { name: "Add f1 to the board" }));
    expect(boards.openBlockCard).toHaveBeenCalledWith("big.ts#function:f1");
    expect(screen.getByLabelText("f2 is on the board")).toBeTruthy();
  });

  it("labels text the graph hasn't seen and draws no links on it", async () => {
    install("not_compiled");
    const { container } = renderWithClient(<SourceTab tab={TAB} active />);
    expect(await screen.findByText("Not yet compiled")).toBeTruthy();
    await waitFor(() => {
      expect(container.querySelector(".source-row")?.textContent).toContain("const v1");
    });
    expect(screen.queryAllByRole("link")).toEqual([]);
    expect(screen.queryByRole("button", { name: /to the board/ })).toBeNull();
    expect(engine.callsOf("file_outline")).toEqual([]);
  });
});
