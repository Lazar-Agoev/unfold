/**
 * The agent mark (design §4.10): a quiet spark and the word "agent" wherever an agent's words show, so
 * they never read as a person's, in both themes and without color. Selected's bylines read "Claude ·
 * agent"; on the canvas, a description or label an agent wrote carries the mark.
 */

import { authorOf } from "../code/text";
import { Icon } from "../icons";
import "./agent.css";

/** The mark itself. `name` (the agent's) titles it: "Written by Claude, an agent". */
export function AgentMark({ name }: { readonly name?: string }) {
  return (
    <span className="agent-mark" title={name !== undefined ? `Written by ${name}, an agent` : undefined}>
      <Icon name="agent" size={10} />
      agent
    </span>
  );
}

/** Who wrote a piece of text: a person's name, or an agent's name and the mark ("Claude · agent"). */
export function Author({ author }: { readonly author: string }) {
  const { name, agent } = authorOf(author);
  return agent ? (
    <span className="agent-author">
      {name} · <AgentMark />
    </span>
  ) : (
    <>{name}</>
  );
}
