# Events

What's on in the city: find something to do this week, see it on a
calendar, book tickets and show them at the door.

```bash
native-shell check examples/events
```

## Design notes

**Primary task.** Find an event and book it. Everything else (the calendar,
adding to it, sharing, cancelling) supports that task.

**Objects.** An *event* (a title, a venue, a day and time, seat tiers with
prices and seats left), and a *ticket* (an event, a tier, a quantity and the
name on the booking). The program is invented and bundled; its dates are
relative to today, so it is always current. Tickets and calendar additions
live on the device (`localStorage`).

**Structure.** Three tabs for the three questions people bring:

- *Discover*: what's on. Featured events first, category filters, then
  everything upcoming by day ("Today", "Fri, Oct 16").
- *Calendar*: what am I doing when. A month with a dot under each day that
  has events, in the tint when you're going or have added one; the selected
  day's agenda beneath.
- *Tickets*: where are my tickets. Soonest first, one tap from the code.

An event opens as a pushed page; booking is a pushed three-step flow
(Tickets → Details → Review) that ends on the ticket.

**Information for a decision.** The event page answers when (date, time
range), where (venue, district, a street plan), how much (the price range) and
how full ("Few seats left · 4", "Sold out") before the one primary action,
*Get Tickets*, pinned above the home indicator with the lowest price beside
it. Rows in lists carry the same: time, venue, lowest price, and "4 left" when
it's nearly gone.

**Booking.**

- *Tickets*: seat tiers as radios with what each one is and what's left; sold
  out tiers are disabled, not hidden. Quantity is a stepper clamped to what's
  left (at most six per order); the running total is always in the footer.
- *Details*: name, email, optional phone, each with the right keyboard,
  content type and Return key (Next, Next, Done → Continue). Errors appear
  under the field they are about when the user continues, and clear as soon
  as the field is fixed.
- *Review*: everything that will be booked and the card it is charged to;
  the button names the amount (`Pay $162`). While paying it shows progress and
  the page can't be left (a half-finished payment is the one thing worse than
  a failed one).

**Destructive actions.** *Cancel Booking* on a ticket asks first, naming the
event and the refund (`Cancel tickets for "Cities and Water"?` with
*Keep Tickets* / *Cancel Booking*), then returns to the list with an Undo
toast. *Add to Calendar* is not destructive; it toggles and offers Undo.

**States.**

| State | Treatment |
| --- | --- |
| Loading | The screen's shape in placeholders (featured card, filters, rows), not a spinner |
| Offline | A failed refresh keeps the program on screen, says how old it is, and offers Try Again |
| Payment failure | An inline alert above the review: what happened, that nothing was charged; the button becomes Try Again |
| Sold out | "Sold out" in the row and on the event; Get Tickets is disabled and says so |
| Few seats | "4 left" in rows, "Few seats left · 4" on the event, per tier in the booking |
| Empty day | What's on next, with a button that jumps to that day |
| No tickets | What the tab is for, and a button to Discover |
| Save failure | An inline alert on Tickets: kept in memory until the app closes |
| Long content | Titles and venues wrap; the event page scrolls under a fixed footer |
| Large text | Rows, fields and buttons grow with the font scale |

**The network.** `service.js` stands in for a box office API: the same calls
an app would make, answered from the bundled program after a network-like
delay. So that every state above is reachable in one session, it fails on a
short script: the first pull to refresh finds no connection, and the first
payment times out. Every later call succeeds.

**Drawn, not downloaded.** Event posters are the category's colors and glyph,
the venue's street plan is drawn with shapes, and the ticket code is a
QR-like grid generated from the booking code (dark on white in both
appearances, as door scanners need). Nothing needs a network or an asset
license, and nothing sits on a photo where contrast can't be guaranteed.
