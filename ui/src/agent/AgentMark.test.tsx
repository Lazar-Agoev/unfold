// @vitest-environment jsdom
import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { authorName, authorOf } from "../code/text";
import { engineKey } from "../ipc/queries";
import { renderWithClient, testQueryClient } from "../test/render";
import { AgentMark, Author } from "./AgentMark";

/** Renders `author`'s byline in the window of the person `lazar`. */
function byline(author: string): HTMLElement {
  const client = testQueryClient();
  client.setQueryData(engineKey("app_info", {}), { username: "lazar", engineVersion: "0" });
  return renderWithClient(<Author author={author} />, client).container;
}

describe("who wrote it", () => {
  it("never makes an agent read as a person", () => {
    expect(authorOf("agent:claude")).toEqual({ name: "Claude", agent: true });
    expect(authorOf("person:maya")).toEqual({ name: "Maya", agent: false });
    expect(authorName("agent:codex")).toBe("Codex · agent");
    expect(authorName("person:maya")).toBe("Maya");
  });

  it("bylines an agent with its mark and name, the person at this window as you, and another person by name", () => {
    const agent = byline("agent:codex");
    expect(agent.textContent).toBe("Codex");
    expect(agent.querySelector(".agent-mark [data-icon='agent']")).not.toBeNull();
    expect(byline("person:lazar").textContent).toBe("you");
    const other = byline("person:maya");
    expect(other.textContent).toBe("Maya");
    expect(other.querySelector(".agent-mark")).toBeNull();
  });

  it("draws the mark as a quiet glyph and the agent's name, titled with it", () => {
    const mark = render(<AgentMark name="Claude" />).container.querySelector(".agent-mark");
    expect(mark?.textContent).toBe("Claude");
    expect(mark?.querySelector("[data-icon='agent']")).not.toBeNull();
    expect(mark?.getAttribute("title")).toBe("Written by Claude, an agent");
  });
});
