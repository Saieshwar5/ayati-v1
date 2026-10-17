# Personal Home and agent-led creation

Accepted by the owner on 2026-10-17. The Personal Floating Canvas prototype is the
default product direction: useful saved views, restrained depth, compact contact
presence and plain language. Prototype business totals are never production data.

The [Organized Home refinement](organized-home.md) supersedes the starter-text,
expand-in-place attention and free-form panel details below.

## Everyday flow

Home starts with a compact decision strip only when attention is required. Open it
to review exact approvals or outcomes. Saved panels form the main canvas. The agent
presence row and Ask Ayati follow. See what happened reveals recent and queued
work. Empty Home asks what the owner wants to track, with Orders, Stock and
Appointments starters. Offline/stale decisions stay disabled.

Add panel and Create app open the same existing-agent chooser. Selection opens an
unsent starter in that agent's conversation. A previous draft is preserved; Add to
draft explicitly appends the starter. The user can also ask directly from any
conversation. Ask its agent on a panel/app carries the stable item ID into a draft.
Made with this agent links durable panels and apps even if a reply was interrupted.

Updates and details holds panel maintenance, provenance and removal controls.
Arrange offers Move earlier and Make wider. Layout persists in this browser only.
Apps uses Ready to try and Start using this app; preview records remain isolated,
and activation still applies to the exact release that the owner previewed.

## Permanent guide

The built-in local-agent identity is Ayati, pinned before other agents. It has guide
instructions on every turn, even after profile edits. It cannot be deleted through
any supported operation. It has no extra tool or account grants and cannot read
other agents' private histories. Its guide context explains actual navigation,
setup, approvals, panel/app creation and current capability limits.

Ask Ayati on Home and the persistent rail opens the actual conversation in a compact drawer, preserving streaming,
Stop, drafts and approvals. Closing the drawer does not stop work. Only the exact
same-origin guide document permits framing; other pages reject embedding. Generated
apps retain their opaque sandbox and cannot access the guide or its credentials.

## Publication and scheduling

panel_write accepts optional publish and updates_enabled booleans, both false by
default. publish=true always requires an exact owner action approval and activates
the resulting revision without a second Keep on Home step. Preview-only requests
omit publish and retain explicit preview review. Source grants, agent ownership,
revision checks, cancellation and source revocation fences remain enforced.

updates_enabled=true requires publish=true and records the explicitly reviewed
same-source update policy. Publishing without it leaves updates disabled, including
when replacing a previously enabled policy. It never creates a schedule. Recurring
work still needs a separately approved schedule_create with the panel ID.

## Compatibility and limits

The host and guest workspace MCP schema must both be updated to advertise optional
publication fields. Existing guest images retain the legacy preview/review path;
updating frontend assets alone does not upgrade an installed VM. Existing VMs,
profiles, conversations and browser data must not be replaced for a visual upgrade.

The guide is model-assisted guidance, not an autonomous account-setup service.
Cloud deployment, cross-device layout sync and native attachment understanding are
outside this increment. Apps becomes visible after a release is saved; work before
that remains visible in the agent conversation, not a speculative Apps placeholder.

## Verification

Use web build/unit/type checks, Rust panel and authority tests, and isolated Goose
browser journeys. tests/personal_home.mjs checks chooser navigation without model
calls, draft preservation, guide framing and chat, approval-gated publication,
denial, restart persistence, panel layout and mobile guide composer visibility.
Existing attention/recovery, scheduled panel updates, app records/activation and
profile-editing journeys cover retained behavior. Fixture data verifies UI and
execution paths, not production model quality or live connector behavior.
