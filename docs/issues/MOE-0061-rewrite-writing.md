# M10g: Rewrite Writing, and the two writing commands' boundary

- **ID**: MOE-0061
- **State**: done
- **Labels**: ready-for-agent, feature
- **Created**: 2026-10-13
- **Updated**: 2026-10-13
- **Commit**: d5735f7

Requested from use, in full:

> Add new rewrite ai command: new AI command for heavy restructuring. Goal: two complementary writing
> commands. One refines near-final prose at sentence level; the other turns rough, spoken, unstructured
> input into organized prose. **Naming**: `improve-writing` — exists, unchanged; convention to follow:
> `<verb>-writing`; new command `rewrite-writing` (alternative: `restructure-writing`). Same convention
> so both group in autocomplete.
>
> **Command A — improve-writing (reference only, do not modify behavior).** Input: near-final prose,
> written by the user, roughly correct. Scope: sentence level — word choice, break long sentences,
> natural/native register. Must not: reorder or merge paragraphs, change document structure.
>
> **Command B — rewrite-writing (new).** Input: rough drafts, fragments, bullet notes, spoken-style
> text, broken grammar, ideas out of order. Scope: document/argument level, plus all sentence-level
> fixes. May: reorder sections and points for logical flow, group related ideas, merge duplicates, split
> overloaded paragraphs, add topic sentences and transitions, convert fragments/notes into full prose,
> fix grammar and register. Must not: invent facts, examples, or claims; change the user's intent or
> position; drop substantive points; pad with filler; over-formalize beyond the user's voice.
>
> **Prompt requirements (both commands).** Each prompt must state its own scope explicitly, and
> reference the other command by name to define the boundary — the separation lives in the prompts, not
> the names. If input is too ambiguous to restructure, ask one clarifying question rather than guessing.
> Output prose only, no commentary, unless the user asks for a summary of changes.

## Decision

- **The new command is `aicmd.rewrite` / "Rewrite Writing"**, placed directly after Improve Writing in
  the SPECS table so the pair sits together in the palette (the convention is the visible
  `⟨Verb⟩ Writing` title; the ids were never user-facing). Subtitle: "Restructure rough drafts into
  organized prose". Icon: `replace`, newly registered in the Lucide map (ADR-0012).
- **Both prompts carry the boundary**, because the boundary *is* the feature: Improve Writing states
  sentence level and forbids reordering/merging, and names Rewrite Writing as the command for
  restructuring; Rewrite Writing states document/argument level, lists what it may do, forbids
  inventing facts, changing intent, dropping points, padding and over-formalizing, names Improve
  Writing as the sentence-level path, and asks one clarifying question when the input is too ambiguous
  to restructure. Both end on prose-only output unless a summary is asked for.
- **Improve Writing's behavior is otherwise untouched** — only its instruction text changed, as the
  request specifies (the old text said nothing about the boundary, so it had to).

## Delivery

- `crates/moe-extensions/src/ai_commands.rs` — the `aicmd.rewrite` spec, the rewritten
  `aicmd.improve` instruction, the module header, and three tests pinning the pair's contract.
- `ui/src/icons.ts` — `replace` registered.

## Acceptance

- [x] The new command is registered and routable (the existing registry guard covers it).
- [x] The prompts' contract is pinned by a test: each names the other, states its own scope, and keeps
      the prose-only rule; Rewrite Writing asks one clarifying question; the forbidden list is present.
- [x] `just test` green (260 Rust tests), `clippy` green.
- [ ] The live pass: "writing" lists both commands adjacently; a rough note pasted as a selection
      produces organized prose through Rewrite Writing; the same note through Improve Writing stays in
      its original order and structure.

## Comments

- 2026-10-13 (agent): filed and closed in one pass. The one judgment call: the title convention. The
  request named the commands `improve-writing` / `rewrite-writing`, which is neither the existing id
  (`aicmd.improve`) nor exactly its title ("Improve Writing"); read as the *visible* convention, it is
  what the palette groups on, so the new title is "Rewrite Writing" and the id follows the extension's
  own prefix. Renaming the ids would have been a contract change with no user-visible gain.
