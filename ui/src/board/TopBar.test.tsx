// @vitest-environment jsdom
import "../test/dom";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { resetBoards, adoptBoardState } from "../state/boards/store";
import { getShowAllCalls, setShowAllCalls } from "../state/showAllCalls";
import { boardDocument, boardState, page } from "../test/workspace";
import { TopBar } from "./TopBar";

const P = "p-orders0000";
const ID = ".unfold/boards/b.json";
const PAGE = { board: { project: P, id: ID }, page: "page-1" };

beforeEach(() => {
  resetBoards();
  setShowAllCalls(false);
  adoptBoardState(boardState(P, ID, boardDocument([page("page-1", [], { title: "Charging the card" })])));
});

describe("the board's top bar", () => {
  it("holds the page's name and the Show all calls switch, and no lens", () => {
    render(<TopBar target={PAGE} />);
    expect(screen.getByRole("heading", { name: "Charging the card" })).not.toBeNull();
    expect(screen.getByRole("switch", { name: "Show all calls" }).getAttribute("aria-checked")).toBe("false");
    expect(screen.queryByRole("radiogroup")).toBeNull();
  });

  it("turns Show all calls on and off for the window, and follows it", () => {
    render(<TopBar target={PAGE} />);
    const control = screen.getByRole("switch", { name: "Show all calls" });
    fireEvent.click(control);
    expect(getShowAllCalls()).toBe(true);
    expect(control.getAttribute("aria-checked")).toBe("true");
    act(() => {
      setShowAllCalls(false);
    });
    expect(control.getAttribute("aria-checked")).toBe("false");
  });
});
