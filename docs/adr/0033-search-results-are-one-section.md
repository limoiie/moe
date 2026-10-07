# Search results are one scored section

The root page used to group every list by source (ADR-0020) — including search results. With
several extensions installed, typing a query split the page into many small groups whose order
follows each group's best item, interrupting the score order the user is scanning for. Raycast's
searching page is one flat scored list.

## Decision

- **A non-empty query returns exactly one section, titled "Results".** Items keep the matcher's
  score order and the existing ordering contract: exact-title bonus > prefix bonus > nucleo score
  > frecency > id.
- **Fallback captures land in the same section.** When nothing matches and extensions capture the
  input (e.g. `Ask "…"`), the captured commands also render under "Results"; an empty fallback
  returns no sections at all (the panel's empty state).
- **The empty query is unchanged**: pinned Favorites (ADR-0027) → Suggestions (IIE4AD-395) → per
  source groups (ADR-0020 keeps governing browsing, where grouping is the point).

## Cost

- Source grouping is gone while searching. The row still labels its owner: the extension name is
  the row's secondary text and the kind badge marks the type (ADR-0030).
- A search page now always spends one section header; result lists make one less row visible than
  a header-less list would.
- ADR-0020's "group order follows each group's best item" now applies to the empty query only.