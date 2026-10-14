# Chat

One conversation thread: read it, reply, fix a message that didn't go out.
Built from gpui-component's conversation parts: `MessageScroller`, `Message`,
`Bubble`, `Attachment` and `Marker`.

```bash
nshell check examples/chat
```

## Design notes

**Primary task.** Read the latest messages and reply. The thread opens at the
newest message (a `MessageScroller`, a bottom-aligned list), which stays just
above the composer as messages arrive and the keyboard rises; the composer
stays above the keyboard and Return sends.

**Object.** Messages, each with a sender, a time and, for yours, a delivery
state. A message is text, a photo or a file. History beyond the recent page
loads on request, a page at a time.

**Hierarchy.** The header names the person and their presence. Messages from
one sender a few minutes apart form a burst: two-point spacing inside, twelve
between bursts, one tail corner at the end. Days are separated by a quiet
centered label with the time, and the first unread message by a tinted
"3 unread messages" line. Your bubbles use the accent; theirs iOS's gray.
Bubbles stop at 80% of the width so the sender reads from the edge. A photo
is a square tile; a file is a card with its name, kind and size.

**Delivery state.** Shown once, under your latest message: *Sending…*
(*Uploading…* for a file), *Delivered*, *Read 9:21 AM*. A failed message says
*Not delivered* with an alert icon (not color alone) and a *Send again*
button beside it; a failed file also turns its card red and says *Upload
failed*. Sending a file again shows its progress in the card (a bar along its
edge and the percentage) until it is delivered.

**Message actions.** A tap or a long press on a bubble opens a sheet with the
message's sender and time and *Copy text* (text only), *Delete for me* (and
*Send again* for a failed one). The gesture is a shortcut, never the only path. Delete is
reversible: an Undo bar stays for five seconds.

**States.**

| State | Treatment |
| --- | --- |
| More history | A *Load earlier messages* button at the top of the thread |
| Loading | A spinner and "Loading earlier messages…" while the page is fetched |
| Unread | A tinted separator line above the first unread message |
| Uploading | The card shows a spinner, the percentage and a progress bar; the footer says *Uploading…* |
| Upload failure | A red card edge, "Upload failed", and *Not delivered · Send again* |
| Scrolled up | A round jump-to-latest button over the thread's corner |
| Load failure | The button reads "Couldn’t load. Try again" |
| Start of conversation | "This is the beginning of your conversation with Maya Chen." |
| Empty draft | The send button is disabled and muted |
| Long messages | Wrap inside the bubble |
| Large text | Bubbles and the composer grow with the font scale |

**Interaction path.** Composer → type → Return (or the arrow) sends, the
field clears and the thread scrolls to the new message. Focusing the composer
keeps the newest message in view above the keyboard: the bottom-aligned list
does it, with no timers.
