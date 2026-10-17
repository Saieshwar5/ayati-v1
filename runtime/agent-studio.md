# Agent Studio

Agents uses a separate tinted contact sidebar, compact avatar/name rows and approval
count badges. The conversation has no masthead. Browser, Automation, Artifacts and
Settings live in a floating tool dock. One contextual panel opens at a time; desktop
panels sit beside chat and narrow screens use focus-trapped native dialog sheets.
The floating composer and per-agent drafts remain in place. Motion respects reduced
motion preferences. Browser controls retain existing takeover and permission checks.

Artifacts contains this agent's stored uploads and outputs, with downloads and safe
plain-text previews. Multiple saved output records with the same name are labeled
as saved versions, newest first; these are saved snapshots, not a live file watcher.
Home panels and Apps created by the agent are discoverable here too.

## Attachments

Both runtimes accept uploads up to 25 MB. A message may reference up to eight stored
artifact IDs. The server resolves each ID in the selected agent's private store;
paths supplied by the browser are not trusted or passed to the VM. Draft attachment
selections survive reloads in per-agent session storage. Retry identity includes the
attachment selection, and immutable archives keep retry content deterministic.

UTF-8 txt, md, csv, tsv, json, yaml and yml files support previews (64 KiB maximum)
and bounded text excerpts in prompts. Attached-message text is limited to 8000 bytes,
excerpts share a 4800-byte budget, and the final serialized request must fit 16000
bytes. Truncation is explicit. Symlinks and internal logs cannot be used as text
attachments. Preview text is rendered as text, never interpreted as HTML.

Images, PDFs, audio and other formats remain downloadable originals; this feature
adds no OCR, document parser, speech recognition or vision capability. Files are
not automatically copied into the VM workspace. Existing Goose VM files are not
automatically exported to Artifacts: this view displays records already archived by
Ayati, while prototype workspace outputs retain their existing archive behavior.

## Checks

`cargo test --test file_attachments`, the web unit/build/e2e type checks, and
`node --test tests/agent_studio.mjs tests/agent_profile.mjs tests/personal_home.mjs`
cover isolated storage, UTF-8 bounds, symlink rejection, receipt identity, real upload
and prompt delivery, non-executing previews, drafts, contextual tools and mobile
sheets. Fixtures use a scripted provider; they do not establish live model accuracy.
