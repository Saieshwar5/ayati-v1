# Authorized people and channel approvals

The [editable OpenPencil concept](approvals.pen) and [source JSX](approvals.jsx)
explore a compact channel reference, a restricted exact-request drawer and explicit
person/channel selection. The concept was created in a fresh connected app document
before implementation. Existing open documents were preserved.

Actual implementation screenshots from the four-person local browser journey:

- [Exact request on desktop](exact-channel-approval-desktop.png)
- [Exact request on a phone](exact-channel-approval-mobile.png)
- [Selected approver and channel](selected-approver-route.png)
- [Restricted inbox in dark mode](restricted-inbox-dark.png)

People, accounts and business records are synthetic test data. The screenshots
were visually inspected. See [the runtime contract](../../runtime/channel-approvals.md)
for authority, source privacy, upgrade instructions and remaining work.
