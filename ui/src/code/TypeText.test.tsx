// @vitest-environment jsdom
import "../test/dom";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { TypeText as TypeTextModel } from "../ipc/generated";
import { TypeText, typeSegments } from "./TypeText";

const CHARGE = "src/payments.ts#interface:Charge";

/** `Promise<Charge>`: a built-in and a declared name. */
const promise: TypeTextModel = {
  text: "Promise<Charge>",
  spans: [
    { start: 0, end: 7, style: "builtin" },
    { start: 8, end: 14, style: "declared", target: CHARGE },
  ],
};

/** `Stripe.PaymentIntent | null`: an external name, then a keyword. */
const external: TypeTextModel = {
  text: "Stripe.PaymentIntent | null",
  spans: [{ start: 0, end: 20, style: "external", module: "stripe" }],
};

describe("typeSegments", () => {
  it("cuts the text at its spans, and words apart from punctuation between them", () => {
    expect(typeSegments(promise).map((segment) => [segment.kind, segment.text])).toEqual([
      ["span", "Promise"],
      ["punct", "<"],
      ["span", "Charge"],
      ["punct", ">"],
    ]);
    expect(typeSegments({ text: "CartItem[]", spans: [{ start: 0, end: 8, style: "declared", target: "k" }] }).map((segment) => segment.kind)).toEqual(["span", "punct"]);
    expect(typeSegments(external).map((segment) => [segment.kind, segment.text])).toEqual([
      ["span", "Stripe.PaymentIntent"],
      ["punct", " | "],
      ["word", "null"],
    ]);
  });

  it("skips spans that overlap or fall outside the text", () => {
    const text = { text: "Map<A, B>", spans: [{ start: 4, end: 5, style: "declared" as const, target: "a" }, { start: 4, end: 8, style: "declared" as const, target: "x" }, { start: 7, end: 20, style: "declared" as const, target: "y" }] };
    expect(typeSegments(text).map((segment) => segment.text).join("")).toBe("Map<A, B>");
    expect(typeSegments(text).filter((segment) => segment.kind === "span")).toHaveLength(1);
  });
});

describe("TypeText", () => {
  it("reads declared names as plain names, and a built-in like punctuation", () => {
    render(<TypeText type={promise} />);
    expect(screen.queryByRole("button")).toBeNull();
    expect(screen.getByText("Charge").className).toBe("type-name");
    expect(screen.getByText("Promise").className).toBe("type-builtin");
  });

  it("marks an external and an unresolved name apart", () => {
    render(
      <>
        <TypeText type={external} />
        <TypeText type={{ text: "Ghost", spans: [{ start: 0, end: 5, style: "unresolved", note: "not imported" }] }} />
      </>,
    );
    expect(screen.getByText("Stripe.PaymentIntent").className).toBe("type-external");
    expect(screen.getByText("Ghost").className).toBe("type-unresolved");
    expect(screen.getByText("null").className).toBe("type-word");
  });
});
