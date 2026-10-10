// @vitest-environment jsdom
import "../test/dom";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { BoardState, PublishOutcome } from "../ipc";
import { openBoard, type BoardRef } from "../state/boards";
import { resetBoards } from "../state/boards/store";
import { useTabs } from "../state/tabs";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import { renderWithClient } from "../test/render";
import { boardDocument, boardState, projectRow, workspaceView } from "../test/workspace";
import { PublishDialog } from "./PublishDialog";

const P = "p-1";
const DRAFT: BoardRef = { project: P, id: ".unfold/local/drafts/board-1.json" };
const PUBLISHED: BoardRef = { project: P, id: ".unfold/boards/order-placement.json" };

let engine: FakeEngine | null = null;

afterEach(() => {
  engine?.uninstall();
  engine = null;
  resetBoards();
  useTabs.setState({ boards: [], active: "home" });
});

function publishedState(changes: number): BoardState {
  return boardState(P, PUBLISHED.id, boardDocument(), { place: "published", published: { revision: "p1", changes, baseChanged: false } });
}

async function open(ref: BoardRef, state: BoardState, publish: (args: Record<string, unknown>) => PublishOutcome) {
  const fake = installFakeEngine({
    handlers: {
      workspace: () => workspaceView([projectRow(P, "storefront-api")]),
      open_board: () => ({ status: "opened", board: state }),
      list_boards: () => ({
        project: P,
        emptyFolders: [],
        boards: [
          {
            project: P,
            id: ref.id,
            place: state.place,
            name: state.name,
            title: state.document.title,
            folder: null,
            pages: 1,
            cards: 0,
            projects: [],
            changes: state.published?.changes ?? 0,
            updatedAtMs: Date.now(),
            heldBy: 1,
            label: null,
            error: null,
          },
        ],
      }),
      publish_board: (args) => publish(args),
      discard_changes: () => publishedState(0),
    },
  });
  engine = fake;
  await openBoard(ref, { activate: true });
  let closed = 0;
  const { rerender } = renderWithClient(
    <PublishDialog
      board={ref}
      onClose={() => {
        closed += 1;
        rerender(<></>);
      }}
    />,
  );
  const dialog = await screen.findByRole("dialog");
  return { fake, dialog, closed: () => closed };
}

describe("Publish", () => {
  it("publishes a draft under its title's slug, and offers Rust's free name when that is taken", async () => {
    let calls = 0;
    const { fake, dialog, closed } = await open(DRAFT, boardState(P, DRAFT.id), () => {
      calls += 1;
      return calls === 1
        ? { status: "name_taken", path: ".unfold/boards/order-placement.json", suggestion: "order-placement-2" }
        : { status: "done", board: publishedState(0) };
    });
    expect(within(dialog).getByRole("heading").textContent).toBe("Publish Order placement");
    expect(within(dialog).getByLabelText("File name")).toHaveProperty("value", "order-placement");
    expect(dialog.textContent).toContain(".unfold/boards/order-placement.json");
    fireEvent.click(within(dialog).getByRole("button", { name: "Publish" }));
    await waitFor(() => {
      expect(within(dialog).getByLabelText("File name")).toHaveProperty("value", "order-placement-2");
    });
    expect(dialog.textContent).toContain("already exists");
    fireEvent.click(within(dialog).getByRole("button", { name: "Publish" }));
    await waitFor(() => {
      expect(closed()).toBe(1);
    });
    expect(fake.callsOf("publish_board").map((args) => args["name"])).toEqual(["order-placement", "order-placement-2"]);
  });

  it("publishes a board's changes, and keeps mine over a changed published file", async () => {
    let calls = 0;
    const { fake, dialog, closed } = await open(PUBLISHED, publishedState(2), () => {
      calls += 1;
      return calls === 1 ? { status: "conflict", theirsRevision: "p2" } : { status: "done", board: publishedState(0) };
    });
    expect(within(dialog).getByRole("heading", { level: 2 }).textContent).toBe("Publish changes to Order placement");
    expect(dialog.textContent).toContain("2 changes");
    fireEvent.click(within(dialog).getByRole("button", { name: "Publish" }));
    expect(await within(dialog).findByRole("radiogroup")).toBeTruthy();
    expect(dialog.textContent).toContain("The published file changed since you started.");
    expect(within(dialog).getByRole("radio", { name: "Keep mine" })).toHaveProperty("checked", true);
    fireEvent.click(within(dialog).getByRole("button", { name: "Publish" }));
    await waitFor(() => {
      expect(closed()).toBe(1);
    });
    expect(fake.callsOf("publish_board").map((args) => args["resolve"])).toEqual([undefined, "keep_mine"]);
  });

  it("drops my changes when I take theirs", async () => {
    const { fake, dialog, closed } = await open(PUBLISHED, publishedState(2), () => ({ status: "conflict", theirsRevision: null }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Publish" }));
    expect(await within(dialog).findByText(/was deleted since you started/)).toBeTruthy();
    fireEvent.click(within(dialog).getByRole("radio", { name: "Take theirs and drop my changes" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Discard my changes" }));
    await waitFor(() => {
      expect(closed()).toBe(1);
    });
    expect(fake.callsOf("discard_changes")).toEqual([{ project: P, id: PUBLISHED.id }]);
    expect(fake.callsOf("publish_board")).toHaveLength(1);
  });
});
