# Pages in agent replies

A long answer becomes a card in the reply and opens as a full-size, read-only
page. Behaviour and limits: [runtime/agent-pages.md](../../runtime/agent-pages.md).

The concept canvas (six screens, including ideas not built yet) is kept outside
the repository: <https://claude.ai/artifact/5r6ZUUtwXcU8RZ9cB4yB4F> (private to
its owner). After review, pages were made reading-only: no buttons that answer or
decide inside a page.

Captures of the implemented React components, rendered with the app's
stylesheets and a sample reply (fictional couriers and figures):

- [Card in a reply, desktop](app-card-desktop.png)
- [Card in a reply, 320 px](app-card-phone.png)
- [Page open, desktop](app-page-desktop.png)
- [Page scrolled to table and step diagram](app-page-desktop-scrolled.png)
- [Page open, phone, dark](app-page-phone-dark.png)

These come from a temporary preview harness, not a running workspace, so they do
not show the Chat shell around the reply.
