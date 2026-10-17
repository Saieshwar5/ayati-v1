# Floating Canvas

Accepted by the owner on 2026-10-17 after interactive Home and Agents prototypes.
This is Ayati's product design direction. It extends the neutral Graphite palette
with raised surfaces, compact chrome and softer contact rows. It supersedes older
square workspace layouts and the prototype-only status of this direction.

## Shared language

- A quiet gray canvas, opaque light/dark surfaces and graphite primary actions.
  Small green, amber and violet indicators mean ready, attention and working;
  always pair them with readable status text.
- A narrow persistent global rail: Home, Apps, Agents and Settings. No repeated
  Ayati/workspace/business-name masthead. Page identity belongs in navigation,
  document titles and accessible landmark labels rather than redundant headings.
- Use the vertical space for work. Start content near the top; use 12–24 px gaps,
  comfortable text and a consistent 20 px surface radius. Round contacts and the
  composer more generously. Avoid giant empty headers or tiny text to force fit.
- Depth is selective: panels, selected contacts, composer and contextual tools
  have restrained shadows. Rows inside those surfaces use spacing and dividers.
  Do not turn every paragraph or control into a card.
- Motion explains a state change: short hover elevation and surface transitions.
  No perpetual bobbing of panels; respect reduced motion and keyboard focus.
- Use shared semantic CSS tokens in `web/src/workspace.css`. Keep native controls,
  existing TypeScript modules and the system sans-serif; no new UI framework.

## Personal Home — accepted refinement

The owner approved the [Personal Home experience](personal-home.md) on 2026-10-17.
It supersedes the earlier agent-card dashboard: saved panels are the main canvas,
with a compact decision strip, small agent presence row, Ask Ayati drawer and
progressive history/settings. Use the same soft raised surfaces and plain-language
conversation-led flows throughout the product. Keep empty states useful, not filled
with zero metrics. Short charts use restrained violet bars with accessible values.

Add panel and Create app select an existing agent and open an unsent conversation
starter. Direct chat requests work too. Preserve drafts. A permanent guide stays
available with ordinary scoped permissions. Panel arrangement persists per browser.
The linked experience specification defines publishing, review and runtime limits.

## Agents

The global rail and agent contacts are separate sidebars. Clicking Agents on the
rail opens/restores the contact list while keeping the selected chat. Contacts
show round initials, a name, actual status, recent activity and attention count.
Use soft, elongated rows; elevate the selected row rather than boxing every row.
Search and New agent belong to this sidebar, not the global navigation.

The selected conversation fills the remaining space. Its compact identity/status
and Browser, Automation and Details actions stay near the top. The opaque rounded
composer floats above the bottom edge, separated from the transcript by space and
shadow. The transcript scrolls independently; messages cannot cover the composer.
Attachments, Send and active-work Stop remain reachable, including short windows.

One contextual tool surface opens at a time. Browser can expand and restore the
conversation. Closing/switching views never returns browser control or resumes
work: owner handback remains explicit. Retain each agent's draft, history and
reading position. On narrow screens use a contact drawer and modal work surfaces,
with native focus containment, Escape and a predictable return to the conversation.

## Apps

Use a searchable directory of floating app surfaces with name, description,
active/preview state and a clear Open or Preview action. Keep the maintaining
agent reachable. Place secondary release review, agent access and archive controls
under Releases and access, beside their app. Do not bury primary launch actions.

Opening an app gives it a large scoped work surface. A draft is labeled Ready to try and previewed before Start using this app;
activation still applies to the exact reviewed release. Preview and active records
remain separate. Access is explicitly assigned per agent and collection. Styling
never implies deployment, granted permissions or a service connection.

## Validation and continued work

Build/type checks, existing web tests and isolated browser journeys cover the
implementation. Check light/dark, 320 px through desktop, short laptop windows,
keyboard dialogs, long messages, drafts, approval recovery and app records.
Screenshots and fixture providers establish UI behavior, not live-provider proof.

Settings adopts shared tokens and chrome; a dedicated Settings layout redesign is
outside this increment. Custom panel layouts, generated apps and future screens
should follow these principles without imposing one fixed business dashboard.
