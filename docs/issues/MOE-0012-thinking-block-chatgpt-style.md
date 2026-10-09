# M7t: the Thinking block adopts ChatGPT's language

- **ID**: MOE-0012
- **State**: done
- **Labels**: ux
- **Created**: 2026-10-09
- **Updated**: 2026-10-09
- **Assignee**: agent
- **Commit**: e31a3f5 (pre-registration; see the renumber note below)

User request: "optimize the ui&ux of these in-chat blocks, I prefer chat-gpt's style — please research
their ui&ux design, and apply to ours."

## Research

`docs/research/chatgpt-thinking-block.md` (primary sources: OpenAI's o1 / o3 / GPT-5 announcement
transcripts and the platform "Reasoning models" docs; `openai.com` and `help.openai.com` refused
direct fetches, so the blog pages were read via Wayback captures; purely visual properties are
labelled observed/inferred there).

Verified label shapes: `Thought for 5 seconds` / `Thought for 1m 19s` / `Reasoned about …`; the shown
text is a markdown-formatted summary, not raw reasoning. Observed: `Thinking` (shimmer) while active;
trailing chevron rotating right → down; ~13 px muted gray, regular weight, borderless; collapsed by
default and **not** auto-expanded; inline expansion, no inner scroll.

## Decision (applied to both surfaces)

- **Labels**: `Thinking` while the model reasons (shimmering); once settled `Thought for {n} seconds`
  (`1 second` singular, `Thought for {m}m {s}s` beyond a minute, `Thought for a few seconds` when
  under a second). The duration is measured from the first reasoning frame to the settled frame.
- **Collapsed by default, no auto-expand, no auto-collapse** — the row is the hit target; the label
  swap is the only state change. The result card re-renders per frame and therefore preserves the
  user's toggle across re-renders.
- **Visuals**: borderless; `text-xs` muted gray (`fg-subtle`, label hover over `surface-float`);
  full-row hover; trailing chevron (right → down on open); markdown-rendered muted content, inline,
  no inner scroll (`md` styles); ~8 px above the answer.
- **Motion**: the shimmer (grayscale gradient sweep) is gated behind `prefers-reduced-motion`.
- **Divergence from the note's inferred a11y advice**: we keep native `<details>/<summary>` (the
  recommended `button` + `aria-expanded` pair buys nothing over it here).

## Delivery

- `ui/src/messages.ts`: `thinkingLabel`, reworked `reasoningParts`/`reasoningEl(reasoning, pending,
  seconds)`, the streaming bubble's start-time + label/shimmer state (replacing the auto-open/fold
  logic); content rendered through the shared `markdown()` sanitizer.
- `ui/src/main.ts`: the result card's thinking start-time state, passed to `reasoningEl`, and the
  open-state carry-over across per-frame re-renders (`clearDetail` resets the timer).
- `ui/src/icons.ts`: `chevron-right`.
- `ui/src/styles.css`: `.moe-thinking` marker/chevron/shimmer rules (reduced-motion gated).
- Docs: ADR-0024 增补 (behaviour change + the visual language, pointing at the research note);
  README (AI Commands / Quick Ask bullets); CONTEXT (`Thinking`).

## Acceptance

- A transform through a reasoning model: card shows a muted `Thinking` row with a shimmer, then
  `Thought for N seconds`, collapsed; clicking expands the markdown reasoning inline; the answer
  follows below; nothing auto-opens or snaps shut — including while frames keep arriving.
- Quick Ask bubbles and the Side View behave identically (one shared builder).
- Reduced-motion systems get no shimmer.
- `pnpm build` green.

## Comments

- 2026-10-09 (agent): researched (docs/research/chatgpt-thinking-block.md) and implementing.
- 2026-10-09 (agent): delivered in `e31a3f5`; `pnpm build` green. GUI check pending: a minimax-M3
  transform should show the collapsed "Thinking" row shimmering, then "Thought for N seconds";
  clicking expands the muted markdown inline; nothing auto-opens or snaps shut.
- 2026-10-09 (agent): renumbered MOE-0011 → MOE-0012 — the `MOE-0011` slot is held by
  reasoning-tag tolerance (reconciled onto origin/main first, together with the rebase of
  this worktree); this issue was allocated concurrently from the pre-reconcile numbering.
  The commit subject above still cites MOE-0011.
