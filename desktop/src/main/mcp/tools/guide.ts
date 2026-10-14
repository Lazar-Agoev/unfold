/** `get_guide` (design §6.2, §7.3): the guide, by topic. Loaded with the session's first tools. */

import { z } from "zod";
import { GUIDE_TOPICS, guideText } from "../guide";
import { textResult } from "../results";
import { defineTool } from "../tool";

export const getGuide = defineTool({
  name: "get_guide",
  title: "Get guide",
  description:
    'Returns a topic of the Unfold guide. Read "start" before any other Unfold tool: how Unfold works, the workflow, the diagram brief, ids and revisions, the review checkpoints, and the rules. Then read the topic a job needs, once a session: "diagrams" (levels of detail, views, frames, layout, density, recipes, what check finds; before building or reshaping a page), "connections" (each kind, steps, channels, carried types, anchors; before connect), "data-models" (type cards and tables; before adding types or tables), "graph" (what the compiled graph claims and doesn\'t; before explaining why something is or isn\'t connected, or what a call sends), "writing" (titles, descriptions, notes; before writing on a page), "setup" (projects, Compile, settings, boards; before setting things up), and "repairs" (health problems and who may sign off; before repair).',
  input: z.object({ topic: z.enum(GUIDE_TOPICS).describe("The topic; start first") }),
  annotations: { readOnlyHint: true },
  alwaysLoad: true,
  run({ topic }, context) {
    return Promise.resolve(textResult(context.header(), guideText(topic)));
  },
});
