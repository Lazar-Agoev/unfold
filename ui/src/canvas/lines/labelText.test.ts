import { describe, expect, it } from "vitest";
import type { LabelInput, LineLabel, TypeText } from "../../ipc/generated";
import { TYPE_LABEL_SHORT, countForms, labelForms, labelParts, labelText, partsText, typeForms } from "./labelText";

const type = (text: string): TypeText => ({ text, spans: [] });
const typed = (text: string): LabelInput => ({ type: type(text) });
const label = (inputs: LabelInput[], output?: string, inputsUnknown = false): LineLabel => ({
  inputs,
  inputsUnknown,
  ...(output !== undefined ? { output: type(output) } : {}),
});

describe("labelText", () => {
  it("writes the inputs' types and the output", () => {
    expect(labelText(label([typed("CartItem[]")], "Promise<Hold>"))).toBe("(CartItem[]) → Promise<Hold>");
    expect(labelText(label([typed("Cart"), typed("User")], "Promise<Charge>"))).toBe("(Cart, User) → Promise<Charge>");
    expect(labelText(label([], "void"))).toBe("() → void");
  });

  it("folds three or more inputs to the first and a count, and lists them all in full", () => {
    const three = label([typed("Cart"), typed("User"), typed("Money")], "Receipt");
    expect(labelText(three)).toBe("(Cart, +2) → Receipt");
    expect(labelText(three, false)).toBe("(Cart, User, Money) → Receipt");
    expect(labelText(label([typed("A"), typed("B")], "C"))).toBe("(A, B) → C");
  });

  it("writes an unannotated parameter's name in gray and a missing output as ?", () => {
    const parts = labelParts(label([{ name: "order" }, typed("string")]));
    expect(parts).toEqual([
      { text: "(", tone: "punct" },
      { text: "order", tone: "name" },
      { text: ", ", tone: "punct" },
      { text: "string", tone: "type" },
      { text: ") → ", tone: "punct" },
      { text: "?", tone: "unknown" },
    ]);
    // An unpaired argument has neither a type nor a name.
    expect(labelText(label([{}], "int"))).toBe("(…) → int");
  });

  it("keeps a Python keyword argument's keyword", () => {
    expect(labelText(label([typed("Cart"), { keyword: "currency", type: type("str") }], "Receipt"))).toBe("(Cart, currency=str) → Receipt");
    const untyped = labelParts(label([{ keyword: "currency", name: "currency" }], "Receipt"));
    expect(untyped[1]).toEqual({ text: "currency", tone: "name" });
  });

  it("writes inputs the reader can't list as (…)", () => {
    expect(labelText(label([typed("Cart")], "Receipt", true))).toBe("(…) → Receipt");
  });

  it("writes a passed reference's own signature", () => {
    // `notify(order, summary)`: the dashed line to `summary` carries `summary`'s parameters.
    expect(labelText(label([typed("Order")], "string"))).toBe("(Order) → string");
  });

  it("offers shorter forms that keep the output, for a label that doesn't fit beside the cards", () => {
    const forms = (value: LineLabel) => labelForms(value).map(partsText);
    expect(forms(label([typed("str"), { keyword: "retries", type: type("int") }], "Gateway"))).toEqual(["(str, retries=int) → Gateway", "(+2) → Gateway", "→ Gateway"]);
    expect(forms(label([typed("Cart")], "Receipt", true))).toEqual(["(…) → Receipt", "→ Receipt"]);
    expect(forms(label([], "void"))).toEqual(["() → void", "→ void"]);
  });

  it("reads an arrow that stands for several calls as a count", () => {
    expect(countForms(3).map(partsText)).toEqual(["3 calls"]);
  });

  it("labels a type arrow with its field's written type, cut short when it is long", () => {
    expect(typeForms(type("CartItem[]")).map(partsText)).toEqual(["CartItem[]"]);
    const long = typeForms(type("Record<string, Map<CartItem, Promise<Receipt>>>")).map(partsText);
    expect(long[0]).toBe("Record<string, Map<CartItem, Promise<Receipt>>>");
    expect(long[1]).toHaveLength(TYPE_LABEL_SHORT);
    expect(long[1]?.endsWith("…")).toBe(true);
  });
});
