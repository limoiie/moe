# M7q: reasoning wrappers — match tag-like tokens tolerantly, log what is left

- **ID**: MOE-0010
- **State**: done
- **Labels**: bug
- **Created**: 2026-10-09
- **Updated**: 2026-10-09
- **Assignee**: agent
- **Commit**: 51bf986

After MOE-0008/0009 the user's MiniMax stream still leaks: the card shows the reasoning and the
answer merged (the wrapper tags are stripped by the HTML sanitizer, so they look like one body), and
⌥⏎ copies the raw text with the tags. The exact-string matcher (`<thinking>` / `</thinking>`) does not
fire on their stream.

## What the evidence says

- The card renders the reasoning as ordinary text with **no visible tag bytes**: only real `<...>`
  tokens are removed by DOMPurify — so the wrapper *is* an angle-bracket tag, just not the exact
  spelling the matcher knows (internal whitespace, different case, or a different name — the user's
  paste consistently shows a "response"-flavoured closing token).
- The previous turn had concluded "canonical pair, stale build"; the rebuilt app disproves the name
  assumption. Exact-string matching is the wrong shape for a value that is a provider/template
  detail.

## Decision

- **Match tag-like tokens tolerantly**: `<` [`/`] name [whitespace] `>` with ASCII-letter names —
  openers by name {thinking, think, reasoning} anywhere, plus {response} at the very start of the
  message; closers by name {thinking, think, reasoning, response}. Attributes are not accepted
  (wrappers are static template strings; prose with `<word ...>` must not match). Any opener may be
  closed by any closer (templates pair `<thinking>` with `</response>`), interleaved blocks keep
  working, and a lone closing token still means a dropped opener.
- Unknown tokens stay answer text; the older same-delimiter `…` style is unchanged.
- **Log what is left**: on completion, if the reasoning or the answer still contains a tag-like
  token, log it escaped (`{:?}`) once — the terminal line names the exact bytes, so a new wrapper
  spelling can be added from the log instead of a screenshot.

## Delivery

- `crates/moe-extensions/src/ai.rs`: `tag_token`, `is_named`, `split_tagged`, `next_closing_tag`,
  `log_unconsumed_tag`; `split_reasoning` reduced to the same-delimiter style + the tag scanner.
  Tests: internal whitespace, case-insensitive names, cross-name pair (`<thinking>`/`</response>`),
  `response` at start, unknown tokens kept in the answer.
- Docs: ADR-0024 amendment line (tolerant matching + the diagnostics log).

## Acceptance

- The wrapper variants above all split; the reported-stream test still passes; ordinary answers with
  `<word>` tokens (XML-ish content) are untouched.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test
  --workspace` green.

## Comments

- 2026-10-09 (agent): filed from the user's second report (rebuilt app still leaks); implementing.
- 2026-10-09 (agent): delivered in `51bf986`; `cargo fmt --check`, `cargo clippy --workspace
  --all-targets -- -D warnings`, `cargo test --workspace` (150) green. Rebuild pending on the user's
  side; if anything still leaks, the terminal now prints the exact tag bytes to extend the name
  sets from.