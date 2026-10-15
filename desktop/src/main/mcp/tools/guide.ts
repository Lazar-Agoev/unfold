/** `get_guide`: the guide, by topic. Loaded with the session's first tools. */

import { z } from "zod";
import { GUIDE_TOPICS, guideText } from "../guide";
import { textResult } from "../results";
import { defineTool } from "../tool";

export const getGuide = defineTool({
  name: "get_guide",
  title: "Get guide",
  description:
    'Returns a topic of the Unfold guide. Read "start" before any other Unfold tool: how Unfold works, compiled first, the workflow, the diagram brief, ids, keys, and revisions, the review checkpoints, and the rules. Then read the topic a job needs, once a session: "diagrams" (levels of detail, tracing entrances, compact and expanded cards, what you add and how much, shaping a card, proposed steps, frames, layout, density, what check finds; before building or reshaping a page), "connections" (the seven kinds, steps, channels, carried types, anchors; before connect), "data-models" (type cards and tables; before adding types or tables), "graph" (what the compiled graph proves and doesn\'t; before explaining why something is or isn\'t connected, or what a call sends), "writing" (titles, names, descriptions, notes; before writing on a page), "setup" (projects, Compile, settings, apps and entrances and proposing them, boards; before setting things up, proposing entrances, or writing unfold.toml), and "repairs" (health problems and who may sign off; before repair).',
  input: z.strictObject({ topic: z.enum(GUIDE_TOPICS).describe("The topic; start first") }),
  annotations: { readOnlyHint: true },
  alwaysLoad: true,
  run({ topic }, context) {
    return Promise.resolve(textResult(context.header(), guideText(topic)));
  },
});
