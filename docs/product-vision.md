# Unfold product vision

Unfold makes actual code visible as connected diagrams, so a developer can understand how a system works,
follow its behavior, and inspect the source behind the blocks.

**Fixed product intent.** Proposals, designs, and implementation must follow this vision. Agents must not edit
this document unless the user explicitly requests a vision change. Surface conflicts with other documents;
do not silently redefine the product.

## Why Unfold exists

AI-generated diagrams can invent or omit behavior and drift from the code. IDEs expose source, but leave the
developer reconstructing the flow by jumping between files. Unfold connects the visual explanation to the
actual code, making it possible to understand both the overall behavior and the details behind it.

## Code supplies the diagram

The compiler reads code in the background and turns its written structure into blocks, steps, decisions,
and proven connections. The same code and saved diagram choices produce the same code-derived structure.
As code changes, that structure updates. Uncertainty stays visible; agents must never invent compiler facts.

A developer can trace what handles an HTTP request, what a worker does, and how functions connect. Expanding
blocks reveals more detail. Combine summarizes consecutive steps as one concise, named block, while keeping
the original steps accessible. Every code block and combined group opens the actual source it represents,
with the diagram still available beside it.

## People and agents add meaning

People and agents choose, arrange, label, describe, and group existing blocks to make the diagram readable.
They can also add architectural nodes and connections, such as backend to queue to worker, or backend to
database, when the compiler cannot establish those relationships. These are useful, intentional parts of
the diagram, with source links where available. Code-derived facts, authored explanations, and proposed
behavior remain distinguishable.

Proposals describe intended changes and appear clearly marked as proposed. People and agents can
add, edit, and remove them through simple diagram actions.

ERD diagrams belong in the product. People and MCP agents can create tables, columns, key markers, and
relationships with cardinality, and connect tables to the code that reads or writes them. ERD capabilities
must be preserved and remain useful without automatic ERD generation.

## The working experience

The workspace follows Paper's familiar organization: projects, folders, boards, and pages. Separate diagrams
can explain production behavior, changes on a branch, or different parts of a system.

A person can create and edit a diagram manually, or create a page, copy its link, and ask Claude Code or
Codex to diagram a service through MCP from their existing coding environment. The agent uses compiled
blocks and the same diagram actions available to the person, then adds explanations and architectural
connections. The person sees the diagram update as the agent works and can inspect and edit it directly.

## Simplicity is a product requirement

Keep understanding code straightforward: expand, combine, connect, explain, and view source on one canvas.
Every added feature must earn its place by helping a developer see or understand the system. Prefer the
smallest clear interaction; avoid parallel diagram workflows and lifecycle machinery that make reading
the code harder.
