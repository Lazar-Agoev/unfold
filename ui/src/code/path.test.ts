import { describe, expect, it } from "vitest";
import { fileForms, letters, shortenPath } from "./path";

// As the panel draws its path: 12 px JetBrains Mono (7.2 px a letter) in a 305 px row. A crumb has 6 px
// of padding; a separator is a letter and 4 px; the `› …` of the steps left out is two separators.
const ROOM = 305;
const LETTER = 7.2;
const SEPARATOR = LETTER + 4;

function crumb(label: string): number {
  return letters(label) * LETTER + 6;
}

/** How `labels` (the file, then each step) are shortened in `room`: the file as shown, and the places left out. */
function shorten(labels: readonly string[], showing: number, room = ROOM): { readonly file: string | undefined; readonly out: number[] } {
  const [file = "", ...steps] = labels;
  const forms = fileForms(file);
  const { out, form } = shortenPath({ file: forms.map(crumb), steps: steps.map((step) => SEPARATOR + crumb(step)), gap: 2 * SEPARATOR }, showing, room);
  return { file: forms[form], out: [...out].sort((a, b) => a - b) };
}

const ORDERS = "src/routes/orders.ts";
const DEEP = ["shop/service.py", "place_order", "try", "except PaymentDeclined", "return", "mark_failed"];

describe("a file's shorter forms", () => {
  it("shortens its folders in the middle, keeping its name", () => {
    expect(fileForms(ORDERS)).toEqual([ORDERS, "src/…/orders.ts", "…/orders.ts"]);
    expect(fileForms("src/cli.ts")).toEqual(["src/cli.ts", "…/cli.ts"]);
    expect(fileForms("unfold.toml")).toEqual(["unfold.toml"]);
    expect(fileForms("a/b.ts")).toEqual(["a/b.ts"]);
  });
});

describe("shortening the code panel's path", () => {
  it("leaves it whole when it fits", () => {
    expect(shorten(["unfold.toml", "Server", "Place an order"], 2)).toEqual({ file: "unfold.toml", out: [] });
  });

  it("first leaves out the steps between the definition and the selection, deepest first, each run as one `…`", () => {
    expect(shorten(DEEP, 5, 400)).toEqual({ file: "shop/service.py", out: [3, 4] });
    // The step showing stays, with a run on each side of it.
    expect(shorten(DEEP, 3, 530)).toEqual({ file: "shop/service.py", out: [2, 4] });
  });

  it("then shortens the file's folders in the middle", () => {
    expect(shorten([ORDERS, "createOrder", "line 26"], 2)).toEqual({ file: "src/…/orders.ts", out: [] });
    expect(shorten([ORDERS, "createOrder", "orderService"], 2)).toEqual({ file: "…/orders.ts", out: [] });
  });

  it("then leaves out the definition, unless it is the step showing, and the file takes back what room it can", () => {
    expect(shorten([ORDERS, "createOrder", "orderService.place"], 2)).toEqual({ file: "src/…/orders.ts", out: [1] });
    expect(shorten([ORDERS, "createOrder", "orderService.place"], 0)).toEqual({ file: "src/…/orders.ts", out: [1] });
    expect(shorten(DEEP, 5)).toEqual({ file: "shop/service.py", out: [1, 2, 3, 4] });
    expect(shorten([ORDERS, "createOrder", "orderService.place"], 1)).toEqual({ file: "…/orders.ts", out: [] });
  });

  it("never leaves out the step showing or the selection: past everything else, the selection's label shortens", () => {
    expect(shorten(["src/cli.ts", "main", "switch (command)", 'case "report"'], 2)).toEqual({ file: "…/cli.ts", out: [1] });
    expect(shorten(DEEP, 3, 400)).toEqual({ file: "…/service.py", out: [1, 2, 4] });
    expect(shorten(["src/lib/retry.ts"], 0, 50)).toEqual({ file: "…/retry.ts", out: [] });
  });
});
