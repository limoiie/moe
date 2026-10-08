# Issue tracker: local files

Issues and specs for this repo live as Markdown files in **`docs/issues/`**, one file per issue:
**`MOE-NNNN-slug.md`** (e.g. `MOE-0001-quick-ask-panel-conversation-page.md`). There is no external
tracker and no CLI — every operation is a plain file read or edit, so it works offline, in any agent
session, and the history travels with the repository.

The Linear-era issues (workspace `iie4limo`, team `IIE4AD`, project Moe `P-IIE4AD-12`) remain the
historical record and stay readable at https://linear.app/iie4limo, but the free workspace is out of
issues: **new work is filed locally**. A migrated issue would keep its story — a new `MOE-NNNN` id
plus a `**Linear**: IIE4AD-NNN` line in the header.

## Issue file shape

```markdown
# M7i: Quick Ask v2 — the panel's conversation page

- **ID**: MOE-0001          <!-- allocated at creation; never renumbered -->
- **State**: in-progress    <!-- triage | todo | in-progress | done | wontfix -->
- **Labels**: feature, ux   <!-- triage roles + free-form labels -->
- **Created**: 2026-10-09
- **Updated**: 2026-10-09
- **Parent**: MOE-0002      <!-- optional: wayfinder child -->
- **Blocked by**: MOE-0003  <!-- optional; keep the blocker's **Blocks**: in sync -->
- **Assignee**: Mo Li       <!-- optional -->
- **Commit**: abc1234       <!-- optional: the commit that delivered it -->

Problem, requirements, design, acceptance — plain Markdown sections.

## Comments

- 2026-10-09 (Mo Li): decision or answer — appended, never rewritten.
```

## Conventions

- **Create an issue**: the next id is the highest existing number + 1 (zero-padded to 4 digits);
  filename `MOE-NNNN-slug.md`; header as above (`triage` unless it has already been reviewed); the
  title keeps the milestone prefix when the work belongs to one (`M7i: …`).
- **Read an issue**: open `docs/issues/MOE-NNNN-*.md` — the id lives in the filename
  (`ls docs/issues` when unsure) — and read the whole file including `## Comments`.
- **List issues**: `ls docs/issues/`, then filter by the header's `State` / `Labels`
  (e.g. `grep -l "State.*todo" docs/issues/*.md`).
- **Comment**: append a dated bullet to the issue's `## Comments`; update `**Updated**`.
- **Apply / remove labels**: edit the `Labels` line (vocabulary in `triage-labels.md`).
- **Close**: set `**State**: done` (or `wontfix`), update `**Updated**`, and record the `**Commit**`
  sha that delivered it.
- **Claim**: set `**Assignee**`.
- **Never delete or renumber** a file; a superseded issue gets `wontfix` plus a comment pointing at
  its replacement.

## When a skill says "publish to the issue tracker"

Create a local issue file as above.

## When a skill says "fetch the relevant ticket"

Read that issue file (all of it) and its `## Comments`.

**PRs as a triage surface: no.** _(Set to `yes` if this repo treats external
PRs as feature requests; `/triage` reads this flag.)_

## Wayfinding operations

Used by `/wayfinder`. The **map** is one issue file labeled `wayfinder:map` in `docs/issues/`;
**child tickets** are files with `**Parent**: <map id>` and a
`wayfinder:<research|prototype|grilling|task>` label. **Blocking** uses the `**Blocked by**:` line
(keep the counterpart's `**Blocks**:` in sync). **Frontier query**: list open children of the map,
drop any with an open blocker (its `Blocked by` file is not `done`) or an assignee; first in map
order wins. **Claim**: set `**Assignee**` to me. **Resolve**: answer as a `## Comments` entry, set
`**State**: done`, then append the context pointer to the map's "Decisions so far".