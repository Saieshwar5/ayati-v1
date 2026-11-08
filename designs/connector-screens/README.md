# Connector screens (proposal)

Drawn 2026-11-08 for owner review. **Not approved and not built.** It changes how
admins add and manage connected services on Apps, and adds health states where
people meet those services. It follows the connector plan in
[connector layer](../connector-layer/README.md): Composio now, Cuckoo's own
connectors and other kinds of connection later, with screens that never depend on
which service connects an app. Editable source: [mockups.html](mockups.html),
opened in a browser from this folder. The example company is fictional.

## How the screens look today

These are the real client from `main` at schema 103, rendered with Vite and
sample answers to its requests (no server), at 1440 × 900.

| File | Screen |
| --- | --- |
| `current-1-apps.png` | Apps with three connected services and Gmail selected. The panel shows each action's rule read-only. |
| `current-2-manage-drawer.png` | Add app → Connect a service opens "Add and manage apps": the former Settings, Connections list, with Approve system, Connect account and Remove approval for every app, and each action's rule and kind in a long list. |
| `current-3-create-tools.png` | New agent, step 2 Tools. Gmail's expired sign-in is not shown. The help line points to "Settings → Connections", which moved to Apps. |
| `current-4-agent-tools.png` | An agent's Tools tab. Same: no health shown. |

## Problems they have

1. Rules are read on Apps but changed in a separate drawer that lists every app.
2. Connecting takes two decisions (Approve system, then Connect account), and the
   rules appear only after both.
3. There is no catalog. Admins see a fixed list, with no search or categories, so
   it cannot grow to hundreds of Composio apps.
4. An unconfirmed write blocks every later write on that account and nothing in
   the client can clear it. Only Activity's "Check the business system" text hints at it.
5. Health is shown only as "Reconnect" on Apps. Agent setup and the Tools tab show
   broken accounts as working.
6. Three help lines still send people to Settings → Connections.
7. Other kinds of connection have no place: MCP servers are operator files only.

## Proposed screens

| File | Screen and notes |
| --- | --- |
| `1-apps-service-panel.png` | Apps, changed. (1) Cards name the state: Writes paused, Reconnect, Not used yet, and when the app was last used. (2) The side panel is the one place to manage a service: admins change rules there; the kind menu (▾) corrects an action's kind and requires a second approver. (3) Writes paused explains itself, with Check what happened. (4) Account: who connected it, when, last used, and sign-in health; the connection service is named in small text for admins only. (5) Disconnect moves to the bottom and confirms. |
| `2-add-app-catalog.png` | Add app, new. (1) One dialog for every kind of connection: services by category, then MCP server (next), your company's app and build an app. (2) Server-side search and paging. (3) Each tile names what agents could do, in Controls' words. (4) Help for apps not listed. |
| `3-connect-a-service.png` | Connect, new. (1) Approve and connect in one step. (2) Starting rules shown first: Cuckoo's safe defaults. (3) Sign-in on the app's own page, whoever runs it. (4) After return: the account and next steps. (5) Failed sign-in saves nothing, as today. |
| `4-check-unconfirmed-write.png` | Did this go out?, new. (1) Why writes are paused. (2) The exact action and fields. (3) What Cuckoo already checked. (4) Two answers, by the agent's managers or admins. (5) Saving releases the pause. |
| `5-agent-setup-tools.png` | Agent setup step 2, small changes. (1) Health tags. (2) Ask an admin, using the existing app requests. (3) Admins can connect an app without losing the draft. |
| `6-states.png` | States. (1) Five card states, including Paused by Cuckoo during a provider incident. (2) Members browse the same catalog and Request. (3) The Tools tab shows the same tags. (4) Activity's Needs review opens the same dialogs. |

## What stays

The Apps page layout, filters, company apps, built apps and requests; the
five-step agent setup; Allow / Ask first / Block and the five kinds; Controls;
reconnect through the provider's page; members' read-only view; light and dark
themes and phone layouts. No screen names Composio or any provider except the
admin-only line under Account.

## What building it needs

Client (React, `web/src/react/apps/` and `agents/`):

- Move rule and kind editing from `settings/workspace-connections.tsx` and
  `connector-safety.tsx` into the Apps panel, then remove the drawer once its
  replacement is verified. Old `section=connections` links open Apps as today.
- New catalog dialog, connect dialog and outcome dialog, each with loading,
  empty, error, double-submission and stale-response handling.
- Health tags in agent setup and the Tools tab; fix the three help lines.

Server (each is in the connector plan):

- `GET …/apps` gains `state` (`ok`, `reconnect`, `writes_paused`, `paused`),
  `last_used`, `connected_by` and `connected_at`.
- `GET …/apps/catalog?query=&category=&cursor=` with categories in `catalog.json`.
- One connect request that approves, starts sign-in and applies safe defaults.
- `POST …/control/actions/{id}/outcome` to settle an unconfirmed write.

## Owner decisions

1. Approve the Apps panel as the only place to manage a service, and removing
   the "Add and manage apps" drawer.
2. Approve one-step connect with the rules shown first.
3. Approve "Did this go out?" and who may answer it (managers and admins).
4. Approve showing the connection service to admins only, in small text.
5. Approve "MCP server" appearing in Add app as the next kind, before it is built.
