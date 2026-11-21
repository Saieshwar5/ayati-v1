# Pictures, files and voice in a conversation (2026-11-21)

**Status: built 2026-11-21**, with the three choices as shown on the screens, after
the owner said to work on it. What is built, its checks and its limits are in
[`runtime/chat-experience.md`](../../runtime/chat-experience.md#pictures-files-and-voice-in-a-message-2026-11-21).

The owner asked for a better way to show attachments after seeing a file as a
bordered button with a loose size and arrow, and a picture with a file row repeated
under it.

What differs from the screens:

- **A single picture is always 16:10** and grid tiles are square, cropped from the
  middle. The screens assumed the picture's own shape, which needs its size stored.
- **An uploaded recording keeps the waveform player**, with its name, size and
  download under it, instead of a file row with Play. The row would have lost seeking.
- **Sending is not shown in the conversation.** A file is stored when it is attached,
  so it shows in the message box as "Uploading…" with a spinner; there is no ring,
  percentage or "Not sent · Retry" row.
- **Three pictures** (not drawn on the screens) are one wide tile over two squares.

Open [`mockups.html`](mockups.html) for the screens: each kind as it looks today and
as proposed, the message box, sending and not sent, both themes and a 320 px width.

## The design in short

- **A picture is the message.** No name, size or download under it (the image viewer
  has them), 320 px on its long side, filling the bubble. The time sits on the
  picture; with text, the text and time go under it. Several pictures share a 2×2
  grid with "+3" on the last tile.
- **A file is one row.** A 40 px type tile (PDF, DOC, XLS, grey otherwise), the name
  with its middle shortened, then type and size. The whole row opens the file;
  download shows on hover, focus and touch. Every row is 300 px wide, with no box
  inside the bubble.
- **Voice keeps its player** at the shared width. The length counts while playing and
  a speed chip (1×, 1.5×, 2×) appears. An uploaded recording is a file row with Play
  in place of the type tile.
- **The message box** shows waiting pictures as thumbnails and files as small tiles.
  A message being sent is in place with a ring that fills; a failed one keeps its row
  with Retry.

## Where it lives

`web/src/react/results/files.tsx` (`MessagePictures`, `MessageFiles`) with
`message-files.css` and `file-kind.ts`, used by chat messages, threads and forwarded
copies. The player is `web/src/react/results/voice-player.tsx`; the message box's
previews are `web/src/react/chat/attached-file.tsx`.

## Server

- Done: previews are 640 px on the long side (`src/media.rs`), up from 256 px.
- Not done: width and height on a picture attachment, so it can keep its own shape
  with its space held before it loads.
- First-page PDF previews are left out.

## Choices made

1. Pictures run to the bubble's corners.
2. Type tiles are coloured for PDF, documents and spreadsheets, grey otherwise.
3. Voice messages have the speed chip.
