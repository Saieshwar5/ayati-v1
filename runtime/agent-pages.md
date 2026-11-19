# Pages in agent replies

First increment, 2026-11-19. A long answer (research, a comparison, a report, a
plan) no longer has to be read inside a chat bubble. The agent keeps its reply to
the conclusion and puts the full document in one **page**. Chat shows a card; the
card opens the page full size. [Design and real captures](../designs/agent-pages/README.md).

## What a page is

A page is part of the agent's saved reply, not a separate record. The agent writes

```text
Courier B is the best fit. The full comparison is in this page.

<page title="Courier comparison for next quarter">
## Recommendation
> Move to Courier B from January.
…
</page>
```

The opening and closing lines each stand alone. The title is 1–120 characters
without `"`, `<` or `>`. Only the first page in a reply counts; anything that does
not fit stays ordinary reply text. A page with no closing line yet is still being
written. `src/response_page.rs` and `web/src/react/pages/page-format.ts` read the
same format and their tests use the same cases.

Because the page lives in the reply, it reuses the [saved reading](result-reading.md)
contract unchanged: the reply's conversation and Library access, its content
version, frozen reading with an explicit "load latest", revocation, the 512 KiB
reader bound and the Markdown download. There is no new table, route, tool or
migration, and opening a page never starts work or calls a model.

## What Chat shows

- `response::reference` adds `page: { title, sections, minutes, blocks, complete }`
  to the reply's reference. `long` then describes the words around the page.
- Compact history, focus and live events replace the reply text with those words
  (shortened as before when long). Uncompacted and streamed text is split in the
  browser, so page text never appears as chat text.
- The card sits in the reply box under the words. It shows the title, the number
  of `##` sections, which blocks it has and a reading time at 200 words a minute,
  or "Writing page…" until the closing line arrives.

## What a page can hold

Pages are for reading only. They never hold questions, choices, approvals, forms
or links that act; those stay with [questions](agent-questions.md) and approvals.

| Block | How the agent writes it | How Cuckoo draws it |
| --- | --- | --- |
| Text | Markdown, sections starting with `## ` | The Chat Markdown boundary (`markdown.ts`); raw HTML and remote images stay text |
| Callout | A Markdown quotation (`> …`) | A soft filled box |
| Table | A Markdown table | A framed table that scrolls sideways when wide |
| Chart | A fenced `chart` block holding one [Home panel chart section](general-home-panels.md) as JSON | The Home panel chart, with its values table |
| Diagram | A fenced `steps` block, one `Step \| detail` line per step (2–8) | Numbered boxes joined by arrows, stacked on phones |

The browser checks chart JSON against the Home panel bounds again (12 points, four
series, matching labels, finite numbers, non-negative donut and stacked data),
because page text is model output. A chart or diagram that does not fit is shown
as its text with a short note, never guessed at. The page view lists `##` headings
as "On this page" and scrolls to them; phones hide that list.

## Who is told about pages

The `PAGES` section of the system prompt (`agents/system_prompt.rs`) is added for
requests made in Cuckoo Chat. Requests that arrive through a Communication MCP
transport do not get it, because other platforms show the reply as plain text.

## Limits and next work

- Not yet checked with a real model or in a live workspace: checks so far are unit
  tests, the client suite and browser captures of the real components with a
  sample reply.
- Quoted, forwarded and notification text made from a reply with a page is not
  rewritten yet and can show the `<page …>` line.
- No images inside pages, no Mermaid or free-form SVG/HTML, no saving a page to the
  Library and no sharing a page on its own.
- Chart axis labels shorten long category names (shared with Home panels); the
  values table keeps full names.
- Related ideas from the same design are not built: one-tap answer buttons in the
  reply, suggested next steps, and "Open as page" for an ordinary long reply.
