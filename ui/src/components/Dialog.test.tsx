// @vitest-environment jsdom
import "../test/dom";
import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { isModalOpen } from "../keyboard";
import { Button } from "./Button";
import { ConfirmDialog, Dialog, PromptDialog } from "./Dialog";

function Harness({ onClose }: { onClose?: () => void }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <Button
        onClick={() => {
          setOpen(true);
        }}
      >
        Delete board…
      </Button>
      <Dialog
        open={open}
        title="Delete “Order placement”?"
        description="The board file is removed from .unfold/boards."
        onClose={() => {
          onClose?.();
          setOpen(false);
        }}
        footer={
          <>
            <Button>Cancel</Button>
            <Button variant="danger">Delete</Button>
          </>
        }
      >
        <input aria-label="Type the name" />
      </Dialog>
    </>
  );
}

describe("Dialog", () => {
  it("traps Tab and Shift+Tab inside, closes on Escape, and returns focus", async () => {
    const user = userEvent.setup();
    const root = document.createElement("div");
    root.id = "root";
    document.body.append(root);
    const onClose = vi.fn();
    render(<Harness onClose={onClose} />, { container: root });
    const opener = screen.getByRole("button", { name: "Delete board…" });
    opener.focus();
    fireEvent.click(opener);

    const dialog = screen.getByRole("dialog", { name: "Delete “Order placement”?" });
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    expect(dialog.getAttribute("aria-describedby")).not.toBeNull();
    expect(root.hasAttribute("inert")).toBe(true);
    expect(isModalOpen()).toBe(true);

    const field = screen.getByRole("textbox", { name: "Type the name" });
    expect(document.activeElement).toBe(field);
    await user.tab();
    await user.tab();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Delete" }));
    await user.tab();
    expect(document.activeElement).toBe(field);
    await user.tab({ shift: true });
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Delete" }));

    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(root.hasAttribute("inert")).toBe(false);
    expect(isModalOpen()).toBe(false);
    expect(document.activeElement).toBe(opener);
    root.remove();
  });
});

describe("ConfirmDialog", () => {
  it("starts on Cancel for destructive confirmations", () => {
    const onConfirm = vi.fn();
    render(
      <ConfirmDialog open title="Delete board?" message="This can't be undone." confirmLabel="Delete" danger onConfirm={onConfirm} onCancel={vi.fn()} />,
    );
    expect(screen.getByRole("alertdialog", { name: "Delete board?" })).toBeTruthy();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Cancel" }));
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });
});

describe("PromptDialog", () => {
  it("validates before submitting and reports the problem on the field", () => {
    const onSubmit = vi.fn();
    render(
      <PromptDialog
        open
        title="Publish board"
        label="Name"
        defaultValue="Order placement"
        confirmLabel="Publish"
        validate={(value) => (value === "Order placement" ? "A board with this name exists." : null)}
        onSubmit={onSubmit}
        onCancel={vi.fn()}
      />,
    );
    const field = screen.getByRole("textbox", { name: "Name" });
    expect(document.activeElement).toBe(field);
    fireEvent.click(screen.getByRole("button", { name: "Publish" }));
    expect(onSubmit).not.toHaveBeenCalled();
    expect(field.getAttribute("aria-invalid")).toBe("true");
    expect(screen.getByText("A board with this name exists.")).toBeTruthy();
    fireEvent.change(field, { target: { value: "Order placement 2" } });
    const form = field.closest("form");
    expect(form).not.toBeNull();
    if (form) {
      fireEvent.submit(form);
    }
    expect(onSubmit).toHaveBeenCalledWith("Order placement 2");
  });
});
