# Shop

A small store for everyday goods, from browsing to a placed order: a catalog
with categories, product pages, search, a cart and checkout, and the order
history. The brand (Larch Supply), its products and their photos are invented
for this example; the illustrations are original and regenerated with
`tools/shop-images/generate.sh`.

```bash
native-shell check examples/shop
```

## Design notes

**Primary task.** Find a product, add it to the cart, and place the order.
Each screen has one filled button for the step that moves the task forward
(Add to Cart, Checkout, Place Order); everything else is tinted or plain.

**Structure.** Four tabs, as store apps on iOS have them: *Shop*, *Search*,
*Cart* (its badge counts the items) and *Account*. Each tab keeps its state
while another shows, so a search or a filtered grid is still there on return.
Product, checkout and order pages push over the tabs with a back button
naming where they return to.

**Objects.** Products (price, an earlier price when on sale, rating, colors
and sizes, details), cart lines (a product variant and a quantity), and
orders. The cart and orders live in `localStorage`; a first run shows two
past orders so Account explains itself.

**Information for a decision.** The grid shows the photo, name, price (the
sale price in red with the earlier price struck through, and a *Sale* tag),
the rating with its review count, and how many colors exist. The product page
adds the photos of each color and a close-up, the details, and what delivery
and returns cost. The cart shows how far it is from free delivery.

**Feedback.**

- Adding to the cart: a success haptic and a toast naming what was added,
  with *Undo*.
- Removing from the cart (the *Remove* button, or a swipe left on the line):
  immediate, with an *Undo* toast. Reversible work gets undo, not a
  confirmation.
- Steppers disable at their limits (1 and 10) instead of doing nothing.
- Promo codes say why they don't apply under the field: unknown, expired, or
  below the code's minimum.

**Forms.** The checkout address labels each field above it, uses the right
keyboard (number pad for the postal code, phone pad for the phone), moves
field to field with Return and puts the keyboard away on the last field.
Problems are reported under each field when the order is placed, and the
keyboard moves to the first of them.

**States.**

| State | Treatment |
| --- | --- |
| Loading the catalog | Placeholders in the grid's shape, so nothing jumps when products arrive |
| Catalog error | An explanation and *Try again*; pull to refresh keeps the products shown and reports a failed refresh in a toast |
| Search, nothing typed | Popular searches to tap |
| Search, no results | The query named, a hint, and *Clear search* |
| Empty cart | What the cart is for and *Browse the Shop* |
| Placing an order | The button shows a spinner and *Placing Order…*; the form and back are locked |
| Order failed | An error alert above the button says the card wasn't charged; *Try Again* keeps everything typed |
| Order placed | A confirmation with the order number and arrival date, then *Continue Shopping* or *View Orders* |
| No orders | An empty state in the Orders section |
| Cart not saved | An alert on the cart that it will be lost when the app closes |

**Simulating errors.** *Account › Demo › Simulate network errors* makes
loading the catalog and placing orders fail, so the error states can be
seen without turning the network off.

**Promo codes to try.** `WELCOME10` (10% off), `SAVE15` ($15 off orders over
$100), `SUMMER24` (expired).
