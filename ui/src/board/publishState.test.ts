import { describe, expect, it } from "vitest";
import { publishState } from "./publishState";

describe("the publish state", () => {
  it("reads Draft, Published, or Published with its unpublished changes", () => {
    expect(publishState("draft", 0, [])).toEqual({ text: "Draft", dot: "faint", publishable: true });
    expect(publishState("published", 0, [])).toEqual({ text: "Published", dot: null, publishable: false });
    expect(publishState("published", 2, [])).toEqual({ text: "Published · 2 unpublished changes", dot: "accent", publishable: true });
    expect(publishState("published", 1, []).text).toBe("Published · 1 unpublished change");
  });

  it("names the other projects a board uses", () => {
    expect(publishState("draft", 0, ["storefront-web"]).text).toBe("Draft · uses storefront-web");
    expect(publishState("published", 3, ["a", "b"]).text).toBe("Published · 3 unpublished changes · uses a, b");
  });
});
