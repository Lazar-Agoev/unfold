// @vitest-environment jsdom
import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { authorName, authorOf } from "../code/text";
import { AgentMark, Author } from "./AgentMark";

describe("who wrote it", () => {
  it("never makes an agent read as a person", () => {
    expect(authorOf("agent:claude")).toEqual({ name: "Claude", agent: true });
    expect(authorOf("person:maya")).toEqual({ name: "Maya", agent: false });
    expect(authorName("agent:codex")).toBe("Codex · agent");
    expect(authorName("person:maya")).toBe("Maya");
  });

  it("bylines an agent with its name and the mark, and a person with their name", () => {
    expect(render(<Author author="agent:claude" />).container.textContent).toBe("Claude · agent");
    const person = render(<Author author="person:maya" />).container;
    expect(person.textContent).toBe("Maya");
    expect(person.querySelector(".agent-mark")).toBeNull();
  });

  it("draws the mark as a quiet glyph and the word, titled with the agent's name when given", () => {
    const mark = render(<AgentMark name="Claude" />).container.querySelector(".agent-mark");
    expect(mark?.textContent).toBe("agent");
    expect(mark?.querySelector("[data-icon='agent']")).not.toBeNull();
    expect(mark?.getAttribute("title")).toBe("Written by Claude, an agent");
  });
});
