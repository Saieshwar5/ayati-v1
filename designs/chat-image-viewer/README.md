# Chat image viewer

Requested 2026-11-21: a screenshot opened from a chat message was shown 728 px
wide inside the 760 px reading dialog, under two header bars, with the chat
still bright behind it. The agreed direction is a viewer of its own:

- the picture uses almost the whole window over a dark backdrop;
- one bar: who shared it and when, the file name and size, zoom, download, close;
- the zoom label is the real percentage, and zooming starts from the fitted size;
- wheel, pinch, double-click and drag for zooming and moving;
- arrows and a "2 of 5" count for the other pictures of the conversation;
- on a phone, full screen with pinch and swipe.

The concept was drawn as a four-screen canvas (today, proposed, zoomed, phone)
in the owner's Claude artifacts; it is private to them and not stored here.

Captures of the implementation:

- [Fitted, desktop](viewer-desktop-fit.png)
- [Zoomed to real size](viewer-desktop-zoomed.png)
- [Phone](viewer-phone.png)

These come from a temporary test page that mounts the real viewer with a stubbed
file source, so the page behind is a placeholder and the pictures shown are other
design captures from this repository. They are not captures of a signed-in
workspace. Two things differ from the concept: the arrows stay visible while
zoomed (hiding them would drop keyboard focus), and phones keep Close on the
right instead of a back arrow on the left.

Behaviour, limits and what was verified:
[runtime/result-reading.md](../../runtime/result-reading.md#image-viewer-2026-11-21).
