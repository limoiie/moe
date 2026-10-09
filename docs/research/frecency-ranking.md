# Frecency ranking for the root "Suggestions" list

Research note, 2026-10-09. Every claim below cites a primary source (official
docs or source code) fetched on that date; URLs are in [Sources](#sources).
Claims that could not be verified against a live primary source are marked in
[Verification notes](#verification-notes).

## Context

- The Suggestions section on the empty-query page is capped at 5 rows
  (`SUGGESTION_LIMIT = 5`) and currently sorts by **raw last-used time only**:
  `used.sort_by(|a, b| b.0.cmp(&a.0))` over `frecency.last_used(...)` —
  i.e. pure LRU (`crates/moe-core/src/registry.rs`, L148–153).
- A `Frecency` module already exists with `count × freshness bucket`
  (×4 ≤ 1 h, ×2 ≤ 1 d, ×1 ≤ 1 w, ×0.25 beyond), but that weight is used only
  to break ties in non-empty-query search — not to order Suggestions
  (`crates/moe-core/src/frecency.rs`, L35–51; `registry.rs` L124–129).
- Concrete failure to fix: a command used 1000 times loses slot #1 to a
  command used 10 times merely because the latter was used once just now.
  Pure recency ignores frequency; the two candidate designs are
  **(a)** `count × last-use bucket decay` (zoxide / classic Firefox) and
  **(b)** an exponentially decayed counter updated on each use
  (modern Firefox, Redis LFU-with-decay, standard streaming math).
- Note: the repo's existing bucket table deviates from zoxide's actual table
  (`×1` vs `×0.5` for "within a week"); both are shown below.

## Findings per system

### 1. zoxide — count × last-use bucket, plus global aging

**Framing in the README.** The README describes zoxide as remembering "which
directories you use most frequently" and links the algorithm to the project
wiki; `_ZO_MAXAGE` is described as configuring "the aging algorithm, which
limits the maximum number of entries in the database" with a default of 10000
([README](https://github.com/ajeetdsouza/zoxide/blob/main/README.md)).

**Frecency (wiki + source).** Each directory starts at `rank = 1`; each access
adds 1. When queried, the score is multiplied by a freshness factor based on
*last access only* ([wiki, Algorithm](https://github.com/ajeetdsouza/zoxide/wiki/Algorithm)):

| Last access | Multiplier |
| --- | --- |
| within the last hour | `rank × 4` |
| within the last day | `rank × 2` |
| within the last week | `rank × 0.5` |
| otherwise | `rank × 0.25` |

This is exactly `src/db/dir.rs::Dir::score` (`rank*4.0` if `duration < HOUR`,
`*2.0` if `< DAY`, `*0.5` if `< WEEK`, else `*0.25`; `HOUR/DAY/WEEK` are
constants in `src/util.rs`) —
[dir.rs](https://github.com/ajeetdsouza/zoxide/blob/main/src/db/dir.rs),
[util.rs](https://github.com/ajeetdsouza/zoxide/blob/main/src/util.rs).

**Aging (the anti-fossil mechanism).** If the sum of all ranks exceeds
`_ZO_MAXAGE` (source default `10_000.0`, `src/config.rs`), every rank is scaled
by `factor = 0.9 * max_age / total_age`, and any entry whose rank then falls
below 1.0 is deleted (`src/db/mod.rs::Database::age`). The wiki notes the
theoretical entry cap is `4 × _ZO_MAXAGE` "although it is lower in practice".
Separately, entries whose path no longer exists on the filesystem and whose
last access is older than 90 days are pruned lazily ([wiki, Pruning](
https://github.com/ajeetdsouza/zoxide/wiki/Algorithm#pruning);
[db/mod.rs](https://github.com/ajeetdsouza/zoxide/blob/main/src/db/mod.rs)).

### 2. Mozilla Firefox Places — origin of the term "frecency"

**Classic algorithm (archived MDN page; the live MDN page and
`wiki.mozilla.org/Places/Frecency_Algorithm` are both 404 today).**
Snapshot of the MDN "Frecency algorithm" doc (last updated Feb 2018):
sample the **10 most recent visits** (`places.frecency.numVisits` = 10);
per-visit *type bonus*: embed 0, link 120, typed 200, bookmark 140, download 0,
permanent redirect 0, temporary redirect 0, default 0; recency *bucket weights*
by age of the visit: first bucket (cutoff 4 days) 100, second (14 days) 70,
third (31 days) 50, fourth (90 days) 30, beyond 90 days 10
([archived MDC page](https://web.archive.org/web/2018id_/https://developer.mozilla.org/en-US/docs/Mozilla/Tech/Places/Frecency_algorithm)).
Formula: `points(visit) = (bonus/100) × bucketWeight`; final score =
`ceil(total_visit_count × Σ points / num_sampled_visits)`. The page's own
example: a bookmarked page visited yesterday and last week, plus two visits
>90 days old → `140 + 84 + 14 + 14 = 252`.

**Current algorithm (mozilla-central tip, deployed 2026-10-08).**
`CalculateFrecencyFunction` in `SQLFunctions.cpp` has replaced buckets with
**exponential decay** over the sampled visits: with
`lambda = ln(2) / halfLifeDays`, each sampled visit scores
`weight × exp(−lambda × (referenceDay − visitDay))`, where `referenceDay` is
the most recent visit; the function returns
`sum(score)/samples_count × max(visit_count, samples_count)`, transformed into
"number of days after which the score would become 1" (`referenceDay + ln(...)/lambda`)
— see the SQL comment "Exponentially decay each visit with an half-life of
halfLifeDays" ([SQLFunctions.cpp](https://hg.mozilla.org/mozilla-central/file/tip/toolkit/components/places/SQLFunctions.cpp)).

The experimental/alternative page frecency in
`PlacesFrecencyRecalculator.sys.mjs` passes these defaults directly in source:
`veryHighWeight 200`, `highWeight 100`, `mediumWeight 50`, `lowWeight 20`,
`halfLifeDays 30`, `numSampledVisits 10` (pref names
`places.frecency.pages.alternative.*`). The same file decays the adaptive
history counts once per idle-day: `use_count = use_count * :decay_rate` with
`places.frecency.decayRate` default `0.975` — the source comment states "A
scaling factor of .975 results in an half-life of 28 days" — and *deletes*
rows whose decayed count falls below `0.975^90 ≈ 0.10` (90 days unused).
Origins frecency is recalculated for origins not visited recently ("otherwise
they'd be stuck at the last calculated value"), and the file carries the
TODO (Bug 1943104): "we should replace frecency with an exponential
self-decaying value, so we don't need to recalculate these." —
[PlacesFrecencyRecalculator.sys.mjs](https://hg.mozilla.org/mozilla-central/file/tip/toolkit/components/places/PlacesFrecencyRecalculator.sys.mjs).

### 3. Redis LFU with dynamic aging

Redis reuses the 24-bit `lru` field per object as **16 bits of last-access
time in minutes + 8-bit logarithmic counter**; new keys start at
`LFU_INIT_VAL = 5` (`server.h`, L4554). The source comment states the
rationale directly: "this field must also be decremented otherwise what used
to be a frequently accessed key in the past, will remain ranked like that
forever, while we want the algorithm to adapt to access pattern changes"
([evict.c](https://github.com/redis/redis/blob/unstable/src/evict.c),
[server.h](https://github.com/redis/redis/blob/unstable/src/server.h)).

- **Increment (probabilistic, logarithmic):** on access,
  `p = 1 / ((counter − 5) × lfu_log_factor + 1)`, increment with probability
  `p`, saturating at 255. Default `lfu-log-factor 10`; the docs table
  (factor 10): 10 hits → counter 10, 100 → 18, 100K → 142, 1M → 255 —
  "Saturate the counter at, around, one million requests"
  ([redis.io, Key eviction](https://redis.io/docs/latest/develop/reference/eviction/);
  `LFULogIncr`, `evict.c`).
- **Decay:** `num_periods = elapsed_minutes / lfu_decay_time`;
  `counter = max(0, counter − num_periods)`. Default `lfu-decay-time 1`
  ("decay the counter every one minute"); `0` disables decay. Decay is applied
  lazily — on access, decay-then-increment (`updateLFU` in `db.c`) and while
  sampling eviction candidates (`LFUDecrAndReturn` called from
  `evictionPoolPopulate`). The docs describe the goal: "combined with a decay
  period so that the counter is reduced over time. At some point we no longer
  want to consider keys as frequently accessed, even if they were in the past"
  ([db.c](https://github.com/redis/redis/blob/unstable/src/db.c), `evict.c`,
  redis.io as above).
- The 8-bit logarithmic counter is a memory optimization (Morris counter);
  it is not needed for a desktop palette — but the *decay* design is the
  transferable anti-fossilization mechanism.

### 4. Command-palette-like surfaces (first-party only)

- **VS Code — documented by source, pure MRU with a bounded LRU.**
  `CommandsHistory` is an `LRUCache<string, number>` keyed by command id,
  holding a monotonically increasing use counter; the provider sorts
  "more recently used command before older", falls back to label order, and
  marks the top group "recently used". History length =
  `workbench.commandPalette.history`, default 50 (`DEFAULT_COMMANDS_HISTORY_LENGTH`);
  a command that falls out of the LRU is forgotten entirely. There is no
  personal frequency weighting (a static `suggestedCommandIds` product list,
  labeled "commonly used", is separate) —
  [commandsQuickAccess.ts](https://github.com/microsoft/vscode/blob/main/src/vs/platform/quickinput/browser/commandsQuickAccess.ts).
  I.e. VS Code is the recency-only baseline this task replaces.
- **Raycast — undocumented.** The public Manual (manual.raycast.com) documents
  features but no result-ranking spec; no first-party formula found.
- **IntelliJ — undocumented.** First-party help documents that Search
  Everywhere "displays the list of recent files" by default, but publishes no
  ranking formula —
  [JetBrains help](https://www.jetbrains.com/help/idea/searching-everywhere.html).

### 5. Exponentially decayed counter (standard math)

The standard formulation is a time-decayed sum with half-life `T`
(`λ = ln 2 / T`):

```
S(t) = Σ_i 2^(−(t − t_i)/T) = Σ_i e^(−λ (t − t_i))        (one unit per use)
incremental update:   S ← S · 2^(−Δt/T) + 1                (on each use)
```

`2^(−Δt/T) = exp(−Δt · ln2/T)`; a primary implementation with this exact
identity and half-life parameterization is Twitter's Algebird `DecayedValue`
("Represents a decayed value… `Σ_i e^{−(t_i − t)} v_i`", storing
`time × ln2 / halfLife`, `average` normalized by `halfLife/ln2`) —
[DecayedValue.scala](https://github.com/twitter/algebird/blob/develop/algebird-core/src/main/scala/com/twitter/algebird/DecayedValue.scala).
Properties: order-independent, bounded for any use rate or burst, recent uses
dominate, and old mass decays geometrically (derivation is standard math, not
a product spec). This is also what modern Firefox computes per page (finding 2)
and what Redis approximates with a decaying counter (finding 3).

## Comparison & recommendation

**(a) `count × last-use bucket` (zoxide / repo's current `score`)**

- Meets the stated requirement (i) trivially: 1000 uses with last use within
  the week score `1000 × 1 = 1000` under the repo's table (`× 0.5 = 500`
  under zoxide's), versus `10 × 4 = 40` for a just-used 10-use command.
- Fails the "stale fossil" requirement (ii): after a week of no use, 1000 uses
  still score `1000 × 0.25 = 250` **forever** (both tables' "otherwise" row).
  That still beats a fresh 10-use command (40); displacing it takes ~63 fresh
  uses (`63 × 4 = 252`) or ~250 uses within the week. One abandoned command
  can pin a slot indefinitely.
- Its real-world remedies are coarse: zoxide's aging is a **global**
  renormalization (`×0.9 × max_age / total` on everything, only when the whole
  DB exceeds `_ZO_MAXAGE`, whose theoretical entry cap is 4×`_ZO_MAXAGE` =
  40,000 entries; rarely reached by a personal palette); classic Firefox
  mitigated by *recomputing* frecency from visit dates so old visits fall into
  the 10-point bucket — which requires retaining visit history.
- It is also a step function of last use alone: all uses within a week score
  identically whether the last was 1 minute or 6 days ago.

**(b) Exponentially decayed counter** — recommended

- Meets (i): a command used at rate `r`/day has steady-state
  `S ≈ r / λ = 1.4427 · r · T`. With `T = 30 d`: 1 use/day → ≈ 43;
  3 uses/day → ≈ 130. A 10-use command whose uses are concentrated in the
  last ~10 days scores ≈ 9; a single fresh use = 1. So even a modest
  once-a-day heavy command stays ~5× ahead of the best case for a 10-use
  command — "clearly ahead" as required.
- Meets (ii): scores decay by 2 per half-life. A steady ≈43 fossil falls
  below a fresh 10-use command in ~68 days and below 1 in ~163 days; a
  1000-use burst falls below 1 in ~10 half-lives (~300 days). Abandoned
  commands therefore fade over **months**, not never, and pruning is trivial
  (drop entries lazily when `S < 0.1` — a single use survives ≈ 3.3
  half-lives ≈ 100 days; this mirrors Firefox's `0.975^90 ≈ 0.10` deletion
  threshold almost exactly).
- No global state, no visit history, no unbounded count: the stored score is
  bounded by `1/(1 − 2^(−Δt/T))` for a given use cadence, and any
  lifetime "count" is implicitly the decayed sum. It also unifies with the
  frecency already used for search tie-breaking (one score instead of two).
- Precedent: it is what current Firefox computes for page frecency
  (`lambda = ln2 / halfLifeDays`, default `halfLifeDays 30`) and for adaptive
  history (daily `×0.975` ≈ 28-day half-life, delete below 0.10); Firefox's own
  TODO asks to replace recalculation with "an exponential self-decaying
  value". Redis's decay exists for exactly the fossil reason; zoxide's
  global aging is (a)'s blunt approximation of it.

**Recommended parameters and implementation**

- `T = 30 days` → `λ = ln2/T ≈ 0.0231 /day ≈ 2.674 × 10⁻⁷ /s`.
  Rationale: Firefox's current default for page frecency is `halfLifeDays 30`
  (and its adaptive-history half-life is 28 days); it yields the ~5× margin in
  requirement (i) and month-scale fading in (ii). `T = 14 d` adapts faster
  (fossil < 1 in ~76 d) but compresses the heavy/light margin to ≈ 2.3×;
  `T = 90 d` preserves rank longer but keeps fossils for ~1.3 years.
- Update on each use:
  `score ← score · 2^(−(now − last_used_unix)/T) + 1 ; last_used_unix ← now`.
- Score lookup: `score · 2^(−(now − last_used_unix)/T)`.
- Prune lazily when `score < 0.1`; keep `last_used_unix` for the existing
  `FrecencyLookup::last_used` API. Optionally require `score ≥ ~0.5` for
  Suggestions eligibility (a single use stays ≥ 0.5 for one half-life).
- Surgical change: replace `Entry { count, last_used_unix }` with
  `{ score, last_used_unix }` in `crates/moe-core/src/frecency.rs`; migrate
  persisted JSON by treating the old `count` as the initial score
  (approximation, or multiply it by the old bucket factor at migration time);
  change the Suggestions sort in `registry.rs` L148–153 from `last_used` to
  the frecency score.
- Honest caveat: if a 1000-use command's last use is ~a week ago **and** its
  long-run rate was very low (1000 uses over years → `r ≈ 0.27/day`, `S ≈ 12`),
  the margin over a 10-use recent command (≈9) is thin — but that command is
  no longer "consistently heavy use" in any recent window, and if it resumes
  use it re-accumulates to the ≈43+ steady state after a few weeks. Any
  monotone frequency+recency rule has such boundary cases; the exponential
  form makes them continuous rather than a bucket cliff.

## Verification notes

- **Blocked/unreachable hosts:** `searchfox.org` and `developer.mozilla.org`
  (live pages) were unreachable/404 from this environment; the Mozilla wiki
  page `wiki.mozilla.org/Places/Frecency_Algorithm` contains no article.
  The classic Firefox bucket table was therefore taken from the Internet
  Archive snapshot of the MDN page (labeled as such); the *current* Firefox
  algorithm was verified against hg.mozilla.org raw source.
- **Not verified:** the numeric defaults of the non-alternative
  `places.frecency.pages.{veryHigh,high,medium,low}Weight` /
  `halfLifeDays` StaticPrefs (names verified in `SQLFunctions.cpp`; their
  defining YAML fragment could not be located, and searchfox search was
  blocked). The alternative-helper defaults (200/100/50/20, 30 days, 10
  samples) are verified directly in `PlacesFrecencyRecalculator.sys.mjs`.
- **Not verified:** any Raycast ranking behavior beyond "no first-party
  documentation found" (manual index checked only).
- **Repository claims** are from this repo's working tree at
  `crates/moe-core/src/{frecency.rs,registry.rs}` (IIE4AD-395/346, ADR-0025).

## Sources

- zoxide README — https://github.com/ajeetdsouza/zoxide/blob/main/README.md
- zoxide, Algorithm wiki (frecency table, aging, pruning) — https://github.com/ajeetdsouza/zoxide/wiki/Algorithm
- zoxide `src/db/dir.rs` (score multipliers) — https://github.com/ajeetdsouza/zoxide/blob/main/src/db/dir.rs
- zoxide `src/db/mod.rs` (`Database::age`) — https://github.com/ajeetdsouza/zoxide/blob/main/src/db/mod.rs
- zoxide `src/config.rs` (`_ZO_MAXAGE` default 10000) — https://github.com/ajeetdsouza/zoxide/blob/main/src/config.rs
- zoxide `src/util.rs` (HOUR/DAY/WEEK) — https://github.com/ajeetdsouza/zoxide/blob/main/src/util.rs
- Firefox Places frecency, classic table (archived MDN, last updated 2018-02-05) — https://web.archive.org/web/2018id_/https://developer.mozilla.org/en-US/docs/Mozilla/Tech/Places/Frecency_algorithm (live pages 404)
- Firefox `CalculateFrecencyFunction` / `CalculateAltFrecencyFunction` — https://hg.mozilla.org/mozilla-central/file/tip/toolkit/components/places/SQLFunctions.cpp
- Firefox `PlacesFrecencyRecalculator` (decayRate 0.975, 90-day deletion, alt weights/half-life) — https://hg.mozilla.org/mozilla-central/file/tip/toolkit/components/places/PlacesFrecencyRecalculator.sys.mjs
- Redis `evict.c` (LFU increment/decay, rationale comments) — https://github.com/redis/redis/blob/unstable/src/evict.c
- Redis `server.h` (`LFU_INIT_VAL 5`) — https://github.com/redis/redis/blob/unstable/src/server.h
- Redis `db.c` (`updateLFU`) — https://github.com/redis/redis/blob/unstable/src/db.c
- Redis docs, "Key eviction" (LFU, log-factor table, decay defaults) — https://redis.io/docs/latest/develop/reference/eviction/
- VS Code `commandsQuickAccess.ts` (MRU + LRU history) — https://github.com/microsoft/vscode/blob/main/src/vs/platform/quickinput/browser/commandsQuickAccess.ts
- JetBrains Help, "Search everywhere" — https://www.jetbrains.com/help/idea/searching-everywhere.html
- Raycast Manual (no ranking documented) — https://manual.raycast.com/
- Twitter Algebird `DecayedValue.scala` (half-life decay) — https://github.com/twitter/algebird/blob/develop/algebird-core/src/main/scala/com/twitter/algebird/DecayedValue.scala
- This repo — `crates/moe-core/src/frecency.rs`, `crates/moe-core/src/registry.rs`