/**
 * The agent mark: a quiet spark and the agent's name (`✦ Claude`) wherever an agent's words show, so
 * they never read as a person's, without color of its own. On the canvas a frame's title and a note
 * carry it; Selected's bylines read "by ✦ Claude", and "by you" for the person at this window.
 */

import { authorOf } from "../code/text";
import { Icon } from "../icons";
import { useAuthor } from "../state/author";
import "./agent.css";

/** The mark itself, with the agent's name: "✦ Claude", titled "Written by Claude, an agent". */
export function AgentMark({ name }: { readonly name: string }) {
  return (
    <span className="agent-mark" title={`Written by ${name}, an agent`}>
      <Icon name="agent" size={10} />
      {name}
    </span>
  );
}

/** Who wrote a piece of text: "you" (the person at this window), another person's name, or an agent's mark. */
export function Author({ author }: { readonly author: string }) {
  const you = useAuthor();
  const { name, agent } = authorOf(author);
  if (agent) {
    return <AgentMark name={name} />;
  }
  return <>{author === you ? "you" : name}</>;
}
