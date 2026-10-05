# Issue tracker: Linear

Issues and specs for this repo live in the Linear workspace **iie4limo**
(https://linear.app/iie4limo), team **IIE4AD**, project **Moe**
(`P-IIE4AD-12`). All operations use the Linear MCP tools — no CLI.

## Conventions

- **Create an issue**: `save_issue` with team "IIE4AD", project "Moe",
  description as Markdown, labels via `labels: [...]`.
- **Read an issue**: `get_issue` (`includeRelations: true`), plus
  `list_comments` with the issueId.
- **List issues**: `list_issues` filtered by team "IIE4AD", project "Moe",
  and `state`/`label`/`assignee` ("me" for my work).
- **Comment**: `save_comment` with issueId + body.
- **Apply / remove labels**: `save_issue` `addLabels` / `removeLabels`;
  missing labels created with `save_issue_label` (team IIE4AD) on first use.
- **Close**: `save_issue` state "Done" (resolve exact status names once with
  `list_issue_statuses` before first use).
- **Claim**: `save_issue` assignee "me".

**PRs as a triage surface: no.** _(Set to `yes` if this repo treats external
PRs as feature requests; `/triage` reads this flag.)_

## When a skill says "publish to the issue tracker"

Create a Linear issue as above.

## When a skill says "fetch the relevant ticket"

`get_issue` + `list_comments` on that issue.

## Wayfinding operations

Used by `/wayfinder`. The **map** is a single issue labeled `wayfinder:map`
in project Moe; **child tickets** are its sub-issues (`parentId` = map),
labeled `wayfinder:<research|prototype|grilling|task>`. **Blocking** uses
Linear relations (`blockedBy` / `blocks` via `save_issue`). **Frontier
query**: list open children of the map, drop any with open blockers
(`get_issue` with `includeRelations`) or an assignee; first in map order
wins. **Claim**: assign to me. **Resolve**: answer as `save_comment`, set
state Done, then append the context pointer to the map's "Decisions so far".
