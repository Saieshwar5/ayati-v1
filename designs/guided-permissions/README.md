# Guided permission setup

The [OpenPencil concept](setup.pen) and [source JSX](setup.jsx) explore the
work/accounts → review → preview/activate journey within the existing workspace
frame. The concept was created and saved in the connected desktop app before
implementation. The implemented UI uses the shared React drawer and controls.

Actual application screenshots from the local Goose/MCP/browser journey:

- [Desktop review](guided-review-desktop.png)
- [Phone review](guided-review-mobile.png)
- [Dark preview](guided-preview-dark.png)

The screenshots use synthetic people/accounts and a scripted local model; they
were visually inspected. No business operation runs during preview. The source
contract, upgrade requirements and material limits are in
[guided permissions](../../runtime/guided-permissions.md).
