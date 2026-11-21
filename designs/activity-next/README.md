# Activity: review and three redesign proposals (2026-11-21)

**Status: proposal, not approved.** Nothing here changes the built page. The approved
design is still [`designs/activity`](../activity/README.md) (2026-10-30) and the built
behaviour is in [`runtime/activity.md`](../../runtime/activity.md).

`mockups.html` is one page: what Activity is for, a review of the page as built today
(annotated screenshots in `current/`), and three live mockups in Cuckoo's own tokens,
light and dark. It is written as page content without a document wrapper because it is
also published as an artifact; it opens directly in a browser.

## Why

Activity was approved with three tabs and now has six, plus a scope switch, Needs
fixing and Pause all agents. The wording and the safety rules are strong; the structure
is not:

- Things that need a person are in three places (Needs me, Now → Needs fixing,
  Tasks → Needs fixing / Late). Only the first is in the rail count.
- Every item opens a dialog; the approved mockup had list and details side by side.
- On Now and History, banners, tiles, charts and filters push the lists below the fold
  of a 900 px window. On a phone about a third of the screen is controls.

## The three designs

| | Idea | First thing you see | Distance from the approved design |
| --- | --- | --- | --- |
| A · One queue | One list and one count for everything that waits on a person, with a reading pane and the decision at the bottom | The most urgent item, opened | Closest |
| B · Team board | One flat row per agent; the selected agent's needs, work, plans and history in a side panel | Every agent's state | Medium |
| C · Day timeline | Now, Upcoming and History as one column in time order around a "now" line | What is running now | Furthest |

Recommended: build A and use B's agent rows as A's Work tab. C's "now" line can go on
the Planned tab later.

## Design A as a working prototype

`design-a/prototype.html` is Design A at full size with sample data, light and dark,
and a phone layout below 860 px. It is plain HTML, CSS and scripts with no build step
(`data.js`, then `ui.js`, one script per tab, and `app.js`); open the file in a browser.
The **Guide** button (or `?`) explains the rules and lists things to try.

What it shows beyond the first mockup:

- The same two parts on every list tab (Needs you, Work, History): the list on the
  left, the picked thing on the right. Nothing opens in a dialog.
- Every item type from the built page, with the built page's own wording and choices:
  an approval (Approve this action, Reject, Request changes), several approvals for the
  same action, a choice question, a browser hand-over, a stopped connection, a failed
  request, an unconfirmed action ("Did this go out?") and a late task.
- Keyboard: J / K to move, A / R / C to decide, `/` to search the current tab.
- After a decision: the item leaves, the next one opens, the decision is written to
  History, and "Decided today" keeps it readable. There is no Undo because the first
  decision counts.
- A request that arrives while you read joins its group without moving your row.
- Work: flat agent rows whose state follows the same items as Needs you, so deciding
  there changes it here; Tasks; Pause all agents inside the ⋯ menu.
- Planned starts at a "Now" line. History opens on the log with totals as one line.
  Insights holds Review and the charts.
- My agents / All agents appears only on the tabs it changes (Work, Planned).

Talking about an item (`talk.js`, and `chat.js` for a simplified Chat page):

- **Today** "Open conversation" is a full page load of Chat at that message
  (`?view=chat&conversation=…&message=…&thread=…`), with no way back to the item;
  "Discuss in chat" opens the direct chat with the agent with a draft started; "Add to
  this request" is the only thing said without leaving, and only for the asker's
  running request.
- **In Design A** each item shows the last messages of its conversation and a reply box
  that posts in that conversation as the person (it says who will see it). "Request
  changes" uses the same box, so the note and the decision go together. "Open in Chat"
  still loads Chat at the message, now with "‹ Needs you" in the header (the pattern
  "‹ For you" already uses) and the item kept in the address, so the same item is open
  on return. An approval decided on its card in Chat leaves the list. "Ask <agent>
  privately" is today's Discuss in chat. Someone who is not in the conversation, or a
  run in a schedule's own work room, gets no link and is told why.
- This needs: a read of the messages around a request for people who may see it, the
  existing send route used from Activity, `from=activity` handled like `from=for-you`,
  and the open item in Activity's address.

Two things in it are new behaviour, not just layout, and need a decision: search is one
box that searches the current tab, and decided items stay listed under "Decided today".

## Decisions for the owner before building

- Whether Fix items count in the rail's red number. The design system currently defines
  that count as the Needs me count.
- Tab names: Needs you, Work, Planned, History, Insights.

## Bugs found in the built page (independent of the redesign)

1. `web/src/style.css:25` — a global `.working` rule leaks into `.activity-status.working`
   (14 px bottom padding under every Working label in Activity).
2. `web/src/react/activity/activity.css:20` — `h2.activity-group:first-child` removes the
   top margin of every group heading, because each is first in its own section.
3. History rows print the type twice ("Schedule · Schedule · Shared inbox",
   "Trigger · trigger in marketing"): `Row` in `history.tsx` plus `place` in `model.ts`.
4. Phone: the six tabs wrap into three rows; Needs me titles are squeezed beside
   non-wrapping buttons.
5. Review: the goal form uses unstyled native inputs, and weekly review text is larger
   than the page's headings.

## How it was checked

The current page was rendered from this repository with the Vite dev server and
fixture responses for `/api/**` (no backend, no model calls), at 1440 × 900 and
390 × 844, light and dark. The company, people and numbers are fictional. The approval
and question dialogs were not reviewed: they need a live server. No user testing.
