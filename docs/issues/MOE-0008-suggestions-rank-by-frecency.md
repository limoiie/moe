# M7p: Suggestions rank by frecency, not raw recency

- **ID**: MOE-0008
- **State**: done
- **Labels**: feature, ux
- **Created**: 2026-10-09
- **Updated**: 2026-10-09
- **Assignee**: agent
- **Commit**: d16b8a3

The root page's Suggestions section (ADR-0023) is ordered by `last_used_unix` — pure LRU.
User report: after heavily using Improve Writing (1000 runs) and Quick Ask only 10 times, one
Quick Ask run put Quick Ask first — even though Improve Writing was still in regular use within a
reasonable recent period. Pure recency is unfair to frequency: one fresh use displaces a
consistently used command.

## Requirements

1. Suggestions order by **frequency weighted by recency** ("frecency"): a single fresh use must not
   displace a heavily used, recently used command.
2. Abandoned commands must eventually fade — a stale fossil cannot pin a slot forever.
3. Section shape is unchanged: at most 5 rows, hidden with no usage; ⌃X / ⌃⇧X forget slots
   (ADR-0025) keep working; search ordering (nucleo score, frecency tie-break) stays coherent.
4. Existing persisted usage keeps its standing (no reset).
5. Research the field against primary sources before choosing.

## Research — `docs/research/frecency-ranking.md`

Surveyed zoxide (count × last-use bucket + *global* aging), Firefox Places (the term's origin;
current implementation: per-page exponential decay, default `halfLifeDays 30`), Redis LFU dynamic
aging, VS Code (plain MRU over a 50-entry LRU), and the standard time-decayed counter. Conclusion:
**per-command exponential decay** is the best practice. Bucket × count can fossilize
(`1000 × 0.25 = 250` forever) and zoxide's global renormalization preserves that order; Firefox and
Redis both fade each entry individually.

## Design — ADR-0023 amendment

- `Frecency` stores a **decayed counter** instead of a lifetime count: each use does
  `score ← score·2^(−Δt / 30 d) + 1`; `score()` decays at read time. Half-life 30 days (Firefox's
  current default).
- Entries below `MIN_SCORE = 0.1` read as forgotten (leave Suggestions and search weight) and are
  pruned lazily on the next `record` (Firefox deletes adaptive history below `0.975^90 ≈ 0.10`;
  Redis ages on access).
- Suggestions sort by score desc — ties by most recent use, then id. `SUGGESTION_LIMIT` stays 5.
- Legacy files' lifetime `count` loads through a serde alias as the initial score and decays away
  over ~10 half-lives; no migration step.
- In the reported case, a steady 1-use/day command converges to ≈ 43 while a single fresh use is 1,
  so the heavy command stays clearly ahead; an abandoned heavy command falls below the best case
  for a fresh 10-use command in ≈ 68 days.

## Acceptance

- `cargo test --workspace` green, including new tests: heavy-recent beats fresh-light; score halves
  per half-life; faded entries read as gone and are pruned; legacy `count` loads as score;
  Suggestions order by score, not recency (ties by last use).
- No UI change: the section is still found by title and rendered the same (`ui/src/main.ts`).

## Comments

- 2026-10-09 (agent): filed from the user's report; research note written; implementing in the
  working tree (Rust-only change).
- 2026-10-09 (agent): implemented in the working tree (uncommitted): `frecency.rs` decayed counter
  + lazy aging, `registry.rs` Suggestions sort by score; docs updated (ADR-0023 amendment,
  CONTEXT, README). Validation: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`,
  `cargo test --workspace` — all green (150 tests, 0 failures).
- 2026-10-09 (agent): renumbered MOE-0003 → MOE-0008 — that id was allocated on `main`
  (enter-stops-ai-command-streams) while this work sat in its worktree; branch rebased onto main.
- 2026-10-09 (agent): delivered via PR limoiie/moe#1 (rebase-merged as `d16b8a3`; CI green: rust
  macOS/Ubuntu + ui). Review (Standards/Spec axes) surfaced only vocabulary/doc items — fixed in
  the two follow-up commits before merge.
