import { beforeEach, describe, expect, it, vi } from "vitest";

const call = vi.fn();
const showToast = vi.fn();
vi.mock("../ipc/commands", () => ({ call }));
vi.mock("../components", () => ({ showToast }));

const { openInEditor } = await import("./files");

beforeEach(() => {
  call.mockReset();
  showToast.mockReset();
});

describe("openInEditor", () => {
  it("names the project, the file, and the line when there is one", async () => {
    call.mockResolvedValue("editor");
    await openInEditor("p-abcdefghij", "src/orders/create.ts", 14);
    await openInEditor("p-abcdefghij", "src/orders/create.ts", null);
    expect(call.mock.calls).toEqual([
      ["open_in_editor", { project: "p-abcdefghij", path: "src/orders/create.ts", line: 14 }],
      ["open_in_editor", { project: "p-abcdefghij", path: "src/orders/create.ts" }],
    ]);
    expect(showToast).not.toHaveBeenCalled();
  });

  it("turns a failure into a toast instead of throwing", async () => {
    call.mockRejectedValue({ code: "not_found", message: "src/gone.ts doesn't exist." });
    await openInEditor("p-abcdefghij", "src/gone.ts");
    expect(showToast).toHaveBeenCalledWith({ title: "Couldn't open the file", body: "src/gone.ts doesn't exist.", tone: "danger" });
  });
});
