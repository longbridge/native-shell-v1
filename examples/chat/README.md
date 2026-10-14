# Chat

One conversation thread: read it, reply, fix a message that didn't go out.

```bash
native-shell check examples/chat
```

## Design notes

**Primary task.** Read the latest messages and reply. The newest message sits
just above the composer; the composer stays above the keyboard and Return
sends.

**Object.** Messages, each with a sender, a time and, for yours, a delivery
state. History beyond the recent page loads on request.

**Hierarchy.** The header names the person and their presence. Messages from
one sender a few minutes apart form a burst: two-point spacing inside, twelve
between bursts, one tail corner at the end. Days are separated by a quiet
centered label with the time. Your bubbles use the accent; theirs a muted
surface. Bubbles stop at 78% of the width so the sender reads from the edge.

**Delivery state.** Shown once, under your latest message: *Sending…*,
*Delivered*, *Read 9:21 AM*. A failed message says *Not delivered* with an
alert icon (not color alone) and a *Send again* button beside it.

**Message actions.** A tap or a long press on a bubble opens a sheet with the
message's sender and time and *Copy text*, *Delete for me* (and *Send again*
for a failed one). The gesture is a shortcut, never the only path. Delete is
reversible: an Undo bar stays for five seconds.

**States.**

| State | Treatment |
| --- | --- |
| More history | A *Load earlier messages* button at the top of the thread |
| Loading | A spinner and "Loading earlier messages…" while the page is fetched |
| Load failure | The button reads "Couldn’t load. Try again" |
| Start of conversation | "This is the beginning of your conversation with Maya Chen." |
| Empty draft | The send button is disabled and muted |
| Long messages | Wrap inside the bubble |
| Large text | Bubbles and the composer grow with the font scale |

**Interaction path.** Composer → type → Return (or the arrow) sends, the
field clears and the thread scrolls to the new message. Focusing the composer
keeps the newest message in view above the keyboard.
