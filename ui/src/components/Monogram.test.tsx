// @vitest-environment jsdom
import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Monogram } from "./Monogram";

describe("Monogram", () => {
  it("shows the name's first letter, uppercased", () => {
    const { container } = render(
      <>
        <Monogram name="storefront-api" size={14} />
        <Monogram name="ödeme" size={20} />
        <Monogram name="Lazar's workspace" size={20} tone="workspace" />
      </>,
    );
    const marks = [...container.querySelectorAll(".monogram")];
    expect(marks.map((mark) => mark.textContent)).toEqual(["S", "Ö", "L"]);
    expect(marks[0]?.className).toBe("monogram monogram-14 monogram-default");
    expect(marks[1]?.className).toBe("monogram monogram-20 monogram-default");
    expect(marks[2]?.classList.contains("monogram-workspace")).toBe(true);
    expect(marks[0]?.getAttribute("aria-hidden")).toBe("true");
  });
});
