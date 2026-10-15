// @vitest-environment jsdom
import "../test/dom";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { BoardSummary } from "../ipc/generated";
import { installFakeEngine, type FakeEngine } from "../test/engine";
import { MoveToFolderDialog, RenameFolderDialog } from "./FolderDialogs";

const P = "p-shop000000";
const BOARD = { project: P, id: ".unfold/boards/checkout.json" };
/** 33 emoji: 33 characters, as the engine counts a name, and 66 UTF-16 units. */
const EMOJI = "📦".repeat(33);
const REFUSED = { code: "invalid_folder", message: "A folder's name is 1 to 64 characters, without “/”." };

let engine: FakeEngine;
let refuse: typeof REFUSED | null;

function summary(folder: string | null): BoardSummary {
  return { project: P, id: BOARD.id, name: "checkout", title: "Checkout", folder, pages: 1, cards: 0, updatedAtMs: 0, label: null, error: null };
}

/** Throws the refusal the engine is set to give, as the preload rejects: plain `{code, message}` data. */
function refused(): void {
  if (refuse !== null) {
    // eslint-disable-next-line @typescript-eslint/only-throw-error
    throw refuse;
  }
}

beforeEach(() => {
  refuse = null;
  engine = installFakeEngine({
    handlers: {
      rename_folder: () => {
        refused();
        return { boards: 1 };
      },
      set_board_folder: ({ folder }) => {
        refused();
        return summary(folder ?? null);
      },
    },
  });
});

afterEach(() => {
  engine.uninstall();
});

/** Types `name` and presses the dialog's `confirm` button. */
function submit(name: string, confirm: string): void {
  const dialog = screen.getByRole("dialog");
  fireEvent.change(within(dialog).getByRole("textbox", { name: "Name" }), { target: { value: name } });
  fireEvent.click(within(dialog).getByRole("button", { name: confirm }));
}

describe("folder dialogs", () => {
  it("leave a name to the engine: 33 emoji are 33 characters, and the folder is renamed", async () => {
    const onClose = vi.fn();
    render(<RenameFolderDialog project={P} folder="Checkout" onClose={onClose} />);
    submit(EMOJI, "Rename");
    await waitFor(() => {
      expect(onClose).toHaveBeenCalled();
    });
    expect(engine.callsOf("rename_folder")).toEqual([{ project: P, from: "Checkout", to: EMOJI }]);
  });

  it("show the engine's refusal on the field and stay open", async () => {
    const onClose = vi.fn();
    render(<RenameFolderDialog project={P} folder="Checkout" onClose={onClose} />);
    refuse = REFUSED;
    submit("Checkout/Refunds", "Rename");
    expect(await within(screen.getByRole("dialog")).findByText(REFUSED.message)).not.toBeNull();
    expect(onClose).not.toHaveBeenCalled();
    // Typing again clears it.
    fireEvent.change(within(screen.getByRole("dialog")).getByRole("textbox", { name: "Name" }), { target: { value: "Refunds" } });
    expect(within(screen.getByRole("dialog")).queryByText(REFUSED.message)).toBeNull();
  });

  it("move a board into a new folder, or show why the engine refused the name", async () => {
    const onClose = vi.fn();
    render(<MoveToFolderDialog request={{ board: BOARD, folder: null }} onClose={onClose} />);
    refuse = { code: "folder_exists", message: "A folder named “Checkout” is already there." };
    submit("Checkout", "Move");
    expect(await within(screen.getByRole("dialog")).findByText("A folder named “Checkout” is already there.")).not.toBeNull();
    expect(onClose).not.toHaveBeenCalled();
    refuse = null;
    submit("Payments", "Move");
    await waitFor(() => {
      expect(onClose).toHaveBeenCalled();
    });
    expect(engine.callsOf("set_board_folder").at(-1)).toEqual({ project: P, id: BOARD.id, folder: "Payments" });
  });
});
