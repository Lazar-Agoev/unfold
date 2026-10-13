import { describe, expect, it } from "vitest";
import { folderProblem } from "./folders";

describe("folder names", () => {
  it("accepts one name", () => {
    expect(folderProblem("Checkout")).toBeNull();
    expect(folderProblem("Billing 2")).toBeNull();
  });

  it("refuses a path, since folders are one level deep", () => {
    expect(folderProblem("Checkout/Payments")).toMatch(/can't contain “\/”/);
  });

  it("refuses blank, padded, overlong, and reserved names", () => {
    expect(folderProblem("")).not.toBeNull();
    expect(folderProblem(" Checkout")).not.toBeNull();
    expect(folderProblem("x".repeat(65))).not.toBeNull();
    expect(folderProblem("..")).not.toBeNull();
    expect(folderProblem("a\\b")).not.toBeNull();
  });
});
