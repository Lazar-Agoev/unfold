// @vitest-environment jsdom
import "../test/dom";
import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { Button } from "./Button";
import { ConfirmDialog, Dialog, PromptDialog } from "./Dialog";

function Harness({ onClose }: { onClose?: () => void }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <Button
        variant="secondary"
        onClick={() => {
          setOpen(true);
        }}
      >
        Delete board…
      </Button>
      <Dialog
        open={open}
        width={400}
        placement="window"
        title="Delete “Order placement”?"
        description="The board file is removed from .unfold/boards."
        onClose={() => {
          onClose?.();
          setOpen(false);
        }}
        footer={
          <>
            <Button variant="secondary">Cancel</Button>
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
    expect(document.activeElement).toBe(opener);
    root.remove();
  });
});

describe("Dialog placement", () => {
  it("centers a canvas dialog over #content-area, with the scrim over just that area", () => {
    const area = document.createElement("div");
    area.id = "content-area";
    area.getBoundingClientRect = () => DOMRect.fromRect({ x: 264, y: 38, width: 1176, height: 862 });
    document.body.append(area);
    render(
      <Dialog open width={400} placement="canvas" title="Delete “Order lifecycle”?" onClose={vi.fn()}>
        <p>This deletes its file.</p>
      </Dialog>,
    );
    const scrim = screen.getByRole("dialog").parentElement;
    expect(scrim?.classList.contains("dialog-scrim-canvas")).toBe(true);
    expect([scrim?.style.left, scrim?.style.top, scrim?.style.width, scrim?.style.height]).toEqual(["264px", "38px", "1176px", "862px"]);
    expect(screen.getByRole("dialog").classList.contains("dialog-400")).toBe(true);
    area.remove();
  });

  it("covers the window for window dialogs, and gives Settings a close button", () => {
    const onClose = vi.fn();
    render(<Dialog open width={470} placement="window" title="Settings" onClose={onClose} />);
    const dialog = screen.getByRole("dialog", { name: "Settings" });
    expect(dialog.parentElement?.classList.contains("dialog-scrim-window")).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});

describe("ConfirmDialog", () => {
  it("starts on Cancel for destructive confirmations", () => {
    const onConfirm = vi.fn();
    render(
      <ConfirmDialog
        open
        title="Delete board?"
        message="This can't be undone."
        confirmLabel="Delete"
        danger
        placement="canvas"
        onConfirm={onConfirm}
        onCancel={vi.fn()}
      />,
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
        title="Rename folder"
        label="Name"
        defaultValue="Order placement"
        confirmLabel="Rename"
        placement="window"
        validate={(value) => (value === "Order placement" ? "A board with this name exists." : null)}
        onSubmit={onSubmit}
        onCancel={vi.fn()}
      />,
    );
    const field = screen.getByRole("textbox", { name: "Name" });
    expect(document.activeElement).toBe(field);
    fireEvent.click(screen.getByRole("button", { name: "Rename" }));
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
