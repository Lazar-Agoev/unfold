import { beforeEach, describe, expect, it } from "vitest";
import { folderKey, initialLayout, setFolderExpanded, useLayout } from "./layout";

beforeEach(() => {
  useLayout.setState(initialLayout);
});

describe("window layout", () => {
  it("remembers expanded folders per project, without duplicates", () => {
    setFolderExpanded("p-a", "Checkout", true);
    setFolderExpanded("p-b", "Checkout", true);
    setFolderExpanded("p-a", "Checkout", true);
    expect(useLayout.getState().folders).toEqual([folderKey("p-b", "Checkout"), folderKey("p-a", "Checkout")]);
    setFolderExpanded("p-b", "Checkout", false);
    expect(useLayout.getState().folders).toEqual([folderKey("p-a", "Checkout")]);
  });
});
