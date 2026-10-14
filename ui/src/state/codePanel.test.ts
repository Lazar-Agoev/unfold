import { beforeEach, describe, expect, it } from "vitest";
import { boardDocument, boardState, page } from "../test/workspace";
import * as store from "./boards/store";
import { closeCode, getCodeOpen, openCode, quietly, setCodeHover, takeQuiet, widenCode } from "./codePanel";
import { showBoard, showHome } from "./shownBoard";

const REF = { project: "p-orders0000", id: ".unfold/boards/b.json" };
const PAGE = { board: REF, page: "page-1" };
const KEY = "src/routes/orders.ts#createOrder";

beforeEach(() => {
  store.resetBoards();
  store.adoptBoardState(boardState(REF.project, REF.id, boardDocument([page("page-1", [{ id: "create", ref: { block: KEY }, at: [0, 0] }])])));
  closeCode();
  takeQuiet();
});

describe("the code panel's state", () => {
  it("opens on a selection's own lines, widens, and moves back to the own lines on the next opening", () => {
    openCode({ project: REF.project, target: { kind: "block", key: "src/routes/orders.ts#createOrder/if[0]" }, cardId: "create", gap: null });
    widenCode({ kind: "block", key: KEY });
    expect(getCodeOpen()?.show).toEqual({ kind: "block", key: KEY });
    openCode({ project: REF.project, target: { kind: "block", key: KEY }, cardId: "create", gap: null });
    expect(getCodeOpen()?.show).toBeNull();
    closeCode();
    widenCode({ kind: "file", path: "src/routes/orders.ts" });
    expect(getCodeOpen()).toBeNull();
  });

  it("forgets the hovered line when it moves or closes", () => {
    openCode({ project: REF.project, target: { kind: "block", key: KEY }, cardId: null, gap: null });
    setCodeHover({ key: KEY, path: "src/routes/orders.ts", line: 25 });
    closeCode();
    openCode({ project: REF.project, target: { kind: "block", key: KEY }, cardId: null, gap: null });
    setCodeHover(null);
    expect(getCodeOpen()?.target).toEqual({ kind: "block", key: KEY });
  });

  it("closes when the window shows another board, or Home", () => {
    showBoard(REF);
    openCode({ project: REF.project, target: { kind: "block", key: KEY }, cardId: null, gap: null });
    showBoard({ project: REF.project, id: ".unfold/boards/other.json" });
    expect(getCodeOpen()).toBeNull();
    showBoard(REF);
    openCode({ project: REF.project, target: { kind: "block", key: KEY }, cardId: null, gap: null });
    showHome();
    expect(getCodeOpen()).toBeNull();
  });

  it("holds a selection made quietly, once, and nothing when the selection didn't change", () => {
    quietly(PAGE, () => {
      store.select(PAGE, { kind: "card", cardId: "create", key: null });
    });
    expect(takeQuiet()).toBe(true);
    expect(takeQuiet()).toBe(false);
    quietly(PAGE, () => {
      store.select(PAGE, { kind: "card", cardId: "create", key: null });
    });
    expect(takeQuiet()).toBe(false);
  });
});
