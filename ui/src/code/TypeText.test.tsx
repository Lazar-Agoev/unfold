// @vitest-environment jsdom
import "../test/dom";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { TypeText as TypeTextModel } from "../ipc/generated";
import { TypeText, typeSegments } from "./TypeText";

const CHARGE = "src/payments.ts#interface:Charge";

/** `Promise<Charge>`: a builtin and a project name. */
const promise: TypeTextModel = {
  text: "Promise<Charge>",
  spans: [
    { start: 0, end: 7, to: { outcome: "builtin" } },
    { start: 8, end: 14, to: { outcome: "project", key: CHARGE } },
  ],
};

/** `Stripe.PaymentIntent | null`: a library name, then a keyword. */
const external: TypeTextModel = {
  text: "Stripe.PaymentIntent | null",
  spans: [{ start: 0, end: 20, to: { outcome: "library", module: "stripe" } }],
};

describe("typeSegments", () => {
  it("cuts the text at its spans, and words apart from punctuation between them", () => {
    expect(typeSegments(promise).map((segment) => [segment.kind, segment.text])).toEqual([
      ["span", "Promise"],
      ["punct", "<"],
      ["span", "Charge"],
      ["punct", ">"],
    ]);
    expect(typeSegments({ text: "CartItem[]", spans: [{ start: 0, end: 8, to: { outcome: "project", key: "k" } }] }).map((segment) => segment.kind)).toEqual(["span", "punct"]);
    expect(typeSegments(external).map((segment) => [segment.kind, segment.text])).toEqual([
      ["span", "Stripe.PaymentIntent"],
      ["punct", " | "],
      ["word", "null"],
    ]);
  });

  it("skips spans that overlap or fall outside the text", () => {
    const text: TypeTextModel = {
      text: "Map<A, B>",
      spans: [
        { start: 4, end: 5, to: { outcome: "project", key: "a" } },
        { start: 4, end: 8, to: { outcome: "project", key: "x" } },
        { start: 7, end: 20, to: { outcome: "project", key: "y" } },
      ],
    };
    expect(typeSegments(text).map((segment) => segment.text).join("")).toBe("Map<A, B>");
    expect(typeSegments(text).filter((segment) => segment.kind === "span")).toHaveLength(1);
  });
});

describe("TypeText", () => {
  it("reads project names as plain names, and a built-in like punctuation", () => {
    render(<TypeText type={promise} />);
    expect(screen.queryByRole("button")).toBeNull();
    expect(screen.getByText("Charge").className).toBe("type-name");
    expect(screen.getByText("Promise").className).toBe("type-builtin");
  });

  it("marks a library and an unknown name apart", () => {
    render(
      <>
        <TypeText type={external} />
        <TypeText type={{ text: "Ghost", spans: [{ start: 0, end: 5, to: { outcome: "unknown", note: "not imported" } }] }} />
      </>,
    );
    expect(screen.getByText("Stripe.PaymentIntent").className).toBe("type-external");
    expect(screen.getByText("Ghost").className).toBe("type-unresolved");
    expect(screen.getByText("null").className).toBe("type-word");
  });
});
