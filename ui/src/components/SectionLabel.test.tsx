// @vitest-environment jsdom
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { SectionLabel } from "./SectionLabel";

describe("SectionLabel", () => {
  it("is a heading with an optional trailing note", () => {
    render(
      <SectionLabel as="h3" trailing="follow order">
        On this page
      </SectionLabel>,
    );
    expect(screen.getByRole("heading", { level: 3, name: "On this page" })).toBeTruthy();
    expect(screen.getByText("follow order").className).toBe("section-label-trailing");
  });

  it("defaults to h2", () => {
    render(<SectionLabel>Pages</SectionLabel>);
    expect(screen.getByRole("heading", { level: 2, name: "Pages" })).toBeTruthy();
  });
});
