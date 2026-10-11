/** `get_guide` (design §6.2, §7.3): the guide, by topic. Loaded with the session's first tools. */

import { z } from "zod";
import { GUIDE_TOPICS, guideText } from "../guide";
import { textResult } from "../results";
import { defineTool } from "../tool";

export const getGuide = defineTool({
  name: "get_guide",
  title: "Get guide",
  description:
    'Returns a topic of the Unfold guide. Read "start" before any other Unfold tool: how Unfold works, the workflow, the diagram brief, the review checkpoints, and the rules. The other topics: "diagrams" (levels of detail, views, step groups, frames, layout, recipes; before building or reshaping a page), "connections" (before connect), "data-models" (before adding types or tables), "graph" (what the compiled graph claims and doesn\'t; before explaining why something is or isn\'t connected, or what a call sends), "writing" (labels, descriptions, titles, notes; before writing on a page), "setup" (projects, Compile, settings; before setting things up), and "repairs" (before repair).',
  input: z.object({ topic: z.enum(GUIDE_TOPICS).describe("The topic; start first") }),
  annotations: { readOnlyHint: true },
  alwaysLoad: true,
  run({ topic }, context) {
    const text = guideText(topic);
    const written = GUIDE_TOPICS.filter((name) => guideText(name) !== null);
    return Promise.resolve(
      text === null
        ? textResult(
            context.header(),
            `The guide topic "${topic}" isn't written yet. Written topics: ${written.join(", ")}. Work from the start topic and the tools' descriptions.`,
          )
        : textResult(context.header(), text),
    );
  },
});
