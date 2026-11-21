# Pictures, files and voice in a conversation (2026-11-21)

**Status: proposal, not built.** The owner asked for a better way to show attachments
after seeing a file as a bordered button with a loose size and arrow, and a picture
with a file row repeated under it.

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

## Where today's version lives

One row serves every attachment: `web/src/react/results/files.tsx` (`MessageFiles`),
used by chat messages, threads and the forward window. The player is
`web/src/react/results/voice-player.tsx`.

## Needs from the server

- Previews of 640 px on the long side; `src/media.rs` caps them at 256 px.
- Width and height on a picture attachment, so its space is held before it loads.
- Nothing for files or voice. First-page PDF previews are left out.

## Choices open

1. Pictures to the bubble's corners (shown) or inside a 3 px white frame.
2. Coloured type tiles (shown) or grey for all.
3. The speed chip on voice messages (shown) or the player as it is today.
