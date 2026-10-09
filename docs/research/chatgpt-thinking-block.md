# ChatGPT "thinking" block — UI/UX research note

Scope: the collapsible reasoning-summary row ChatGPT renders above an answer
("Thinking" while reasoning; "Thought for N seconds" once done), so a compact
desktop utility panel can adopt the same visual language.

Method: primary sources only (OpenAI blogs, platform docs). `openai.com` and
`help.openai.com` refused direct fetches (Cloudflare 403); blog pages were read
via Wayback Machine captures of the original pages, which preserve the primary
text. The live ChatGPT UI cannot be fetched, and help-center articles were not
retrievable, so purely visual properties below are explicitly labelled
**observed/inferred (not primary-sourced)**.

## 1. Collapsed state

| Claim | Status | Source |
| --- | --- | --- |
| A model-generated summary of the reasoning is shown; the raw chain of thought is never shown | Verified | "Learning to Reason with LLMs", 2024-09-12: "we have decided not to show the raw chains of thought to users… For the o1 model series we show a model-generated summary of the chain of thought." |
| Completed label renders as `Thought for 5 seconds` (also `Thought for 4 seconds`) directly above the answer | Verified (rendered in OpenAI's own transcripts) | Same page — o1-preview demo responses show the label "Thought for 5 seconds" between prompt and answer |
| Completed label can be topic-aware and abbreviated for long durations: `Reasoned about polynomial construction for 55 seconds` (o3), `Thought for 1m 19s` (o1) | Verified (rendered in OpenAI's own transcripts) | "Introducing OpenAI o3 and o4-mini", 2025-04-16 |
| While reasoning, label is `Thinking` (optionally animated shimmer); on completion swaps to `Thought for Ns`, or `Thought for a few seconds` when short | Observed/inferred (not primary-sourced) | Product memory; no fetched source documents the in-progress label |
| Chevron sits at the right end of the row; points right (collapsed) and rotates ~90° to point down when expanded | Observed/inferred (not primary-sourced) | — |
| Typography: small (~13 px), muted/secondary gray, regular weight, not italic | Observed/inferred (not primary-sourced) | — |
| Borderless — no box, background fill, or left rule; left-aligned with the answer column; modest gap (~8–12 px) above the answer | Observed/inferred (not primary-sourced) | — |

## 2. Expanded state

| Claim | Status | Source |
| --- | --- | --- |
| Content is the reasoning *summary*, not raw reasoning tokens; only present when summaries are opted in ("Reasoning summary output is part of the `summary` array in the `reasoning` output item… will not be included unless you explicitly opt in") | Verified | Platform docs, "Reasoning models" → Reasoning summaries |
| Summary text is Markdown: bold lead-ins and paragraph/bullet structure (example: `"**Answering a simple question**\n\nI'm looking at a straightforward question…"`) | Verified | Same page (example payload) |
| Summary detail level is configurable: `auto`, `concise`, `detailed` (`auto` ≈ most detailed available; computer-use model supports `concise`, o4-mini supports `detailed`) | Verified | Same page |
| Visually muted relative to the answer (lighter gray), same or slightly smaller size, Markdown rendered, indented into the answer column, no inner scroll or documented max-height (expands inline) | Observed/inferred (not primary-sourced) | — |

## 3. Behavior

| Claim | Status | Source |
| --- | --- | --- |
| Reasoning runs before any answer text appears; the model can emit a long internal chain of thought first | Verified | "Learning to Reason with LLMs" ("o1 thinks before it answers"); "Introducing OpenAI o1-preview" ("designed to spend more time thinking before they respond") |
| GPT-5-era routing decides when reasoning happens: a unified system with a router picks quick answer vs "GPT-5 thinking", and "applying reasoning automatically when the response would benefit from it" | Verified | "Introducing GPT-5", 2025-08-07 |
| The block renders collapsed by default, does not auto-expand while reasoning, and stays collapsed after completion; the whole header row is clickable to toggle | Observed/inferred (not primary-sourced) | — |
| While thinking, the label has a shimmer/grayscale-gradient animation; it stops when the duration label replaces it | Observed/inferred (not primary-sourced) | — |
| Reduced-motion handling | Unknown — no source found (documented or observed with confidence) | — |

## 4. Accessibility semantics

No primary source documents the DOM semantics of the block. Verified statement:
none. **Inferred recommendation (not observed):** a `<button>` header with
`aria-expanded` + `aria-controls`, content hidden via `hidden` when collapsed.

## Design takeaways

Concrete values to implement, each tagged by evidence strength:

- **Labels:** `Thinking` while active (animated); on completion, `Thought for {N}s`
  — short durations as `Thought for a few seconds`; optionally topic-aware
  `Reasoned about {topic} for {N}` (verified label shapes, in-progress label
  observed/inferred).
- **Chevron:** trailing end of the row, closed orientation, rotates ~90°
  (right → down) when open; the entire row is the hit target
  (observed/inferred).
- **Typography:** ~13 px, secondary/muted gray, regular weight, no italics; no
  border, no background, no left rule; left-aligned with content; ~8–12 px gap
  above the answer (observed/inferred).
- **Content:** render the reasoning as sanitized Markdown (bold lead-ins,
  paragraphs, bullets), muted color, inline expansion without inner scroll
  (Markdown verified at the API level; styling observed/inferred).
- **Behavior:** render collapsed; label swap on completion; click toggles;
  do not auto-expand; gate the shimmer behind `prefers-reduced-motion`
  (observed/inferred, except summary generation itself which is verified).
- **A11y:** button + `aria-expanded`/`aria-controls` with `hidden` content
  (inferred, no primary source).

## Source notes

- Platform docs: `https://platform.openai.com/docs/guides/reasoning`
  (fetched 2026-10-09; "Reasoning summaries" section).
- "Learning to Reason with LLMs": `https://web.archive.org/web/2024/https://openai.com/index/learning-to-reason-with-llms/`
  (original 2024-09-12).
- "Introducing OpenAI o1-preview":
  `https://web.archive.org/web/20240914000000/https://openai.com/index/introducing-openai-o1-preview/`.
- "Introducing OpenAI o3 and o4-mini":
  `https://web.archive.org/web/2025/https://openai.com/index/introducing-o3-and-o4-mini/`
  (original 2025-04-16).
- "Introducing GPT-5":
  `https://web.archive.org/web/2025/https://openai.com/index/introducing-gpt-5/`
  (original 2025-08-07).
- `help.openai.com` articles and search engines were unreachable from this
  environment (Cloudflare challenge), so no help-center quote is cited; visual
  specifics therefore remain observed/inferred rather than verified.