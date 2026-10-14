// Status and empty states, in a shop's order screen: a status tag and a
// banner for each delivery state (on time, delayed, payment declined), badges
// on the tab bar and on avatars, an inbox that can be emptied (and undone), a
// feed that shows still placeholders while it loads, and the shortcuts a
// hardware keyboard offers.

import { View, div, h_flex, v_flex, Tabs } from "native-shell";
import {
  SuccessAlert, WarningAlert, ErrorAlert, Badge, Tag, Skeleton, Empty, EmptyHeader, EmptyMedia, EmptyTitle,
  EmptyDescription, EmptyContent, Kbd, Label, Separator, ShimmerText, Notification,
} from "gpui-component";
import { button, card, heading, icon, page } from "../ui.js";

const STATES = ["On time", "Delayed", "Declined"];

const ITEMS = [
  { name: "Field notebook, dot grid", qty: 2, price: 24 },
  { name: "Brass pocket pen", qty: 1, price: 38 },
  { name: "Canvas pencil roll", qty: 1, price: 19 },
];

const MESSAGES = [
  { id: "courier", from: "Northline Courier", initials: "NC", text: "Your parcel is at the Memphis hub.", time: "9:41", online: true },
  { id: "seller", from: "Fieldnotes Supply", initials: "FS", text: "Thanks for your order! Packing it today.", time: "Yesterday", online: false },
  { id: "support", from: "Store support", initials: "SS", text: "Your return label is ready to print.", time: "Mon", online: true },
];

const PICKS = [
  { id: "ink", name: "Iron gall ink, 50 ml", price: 16 },
  { id: "folio", name: "Leather folio", price: 64 },
  { id: "clips", name: "Brass binder clips", price: 9 },
];

const SHORTCUTS = [
  { label: "Search orders", keys: "cmd-f" },
  { label: "New message", keys: "cmd-n" },
  { label: "Archive message", keys: "cmd-backspace" },
  { label: "Refresh", keys: "cmd-r" },
];

const money = (n) => `$${n.toFixed(2)}`;

export default class StatusPage extends View {
  init(_props, cx) {
    this.state = 1;
    this.delayDismissed = false;
    this.unread = new Set(["courier", "seller"]);
    this.archived = false;
    this.loading = false;
    this.task = null;
    this.load(cx);
  }

  release() {
    this.task?.cancel();
  }

  /** Stands in for fetching recommendations. */
  load(cx) {
    this.loading = true;
    this.task?.cancel();
    this.task = cx.timer.after(1200, (cx) => {
      this.loading = false;
      cx.notify();
    });
    cx.notify();
  }

  archiveAll(cx) {
    const unread = new Set(this.unread);
    this.archived = true;
    this.unread.clear();
    cx.haptic("success");
    cx.push_toast({
      title: `Archived ${MESSAGES.length} messages`,
      level: "info",
      action: {
        label: "Undo",
        on_click: (cx) => {
          this.archived = false;
          this.unread = unread;
          cx.notify();
        },
      },
    });
    cx.notify();
  }

  orderCard(cx) {
    const t = cx.theme();
    const tag = [
      Tag.new().variant("success").rounded_full().child("Shipped"),
      Tag.new().variant("warning").rounded_full().child("Delayed"),
      Tag.new().variant("danger").rounded_full().child("On hold"),
    ][this.state];
    const banner = [
      SuccessAlert.new("on-time", "Arrives Thursday, October 16, by 8 PM.").title("On its way"),
      WarningAlert.new("delayed", "A storm held your parcel in Memphis. New estimate: Saturday, October 18.")
        .title("Running late")
        .visible(!this.delayDismissed)
        .on_close((_e, cx) => {
          this.delayDismissed = true;
          cx.notify();
        }),
      ErrorAlert.new("declined", "Your card ending 4242 was declined. Update it by Friday to keep your order.").title(
        "Payment declined",
      ),
    ][this.state].test_id("order-alert");
    const total = ITEMS.reduce((sum, i) => sum + i.qty * i.price, 0);
    return card(cx)
      .test_id("order")
      .child(
        h_flex()
          .gap_3()
          .px_4()
          .pt_4()
          .pb_3()
          .child(
            v_flex()
              .flex_1()
              .min_w_0()
              .child(div().role("header").text_lg().font_semibold().text_color(t.foreground).child("Order #4821"))
              .child(div().text_sm().text_color(t.muted_foreground).child("Placed October 9 · 4 items")),
          )
          .child(tag.test_id("order-tag")),
      )
      .child(div().px_4().pb_3().child(banner))
      .when(this.state === 2, (el) =>
        el.child(
          div()
            .px_4()
            .pb_3()
            .child(button(cx, "update-payment", "Update payment method", { variant: "primary", full: true, on_click: (_e, cx) => {
              this.state = 0;
              cx.haptic("success");
              cx.push_toast({ title: "Payment updated", level: "success", timeout: 2000 });
              cx.notify();
            } })),
        ),
      )
      .child(Separator.new())
      .children(
        ITEMS.flatMap((item, i) => [
          i > 0 ? Separator.new().ml_4() : null,
          h_flex()
            .px_4()
            .py_3()
            .gap_3()
            .child(Label.new(item.name).secondary(`× ${item.qty}`).flex_1().min_w_0().text_base().text_color(t.foreground))
            .child(div().flex_none().text_base().text_color(t.muted_foreground).child(money(item.qty * item.price))),
        ]),
      )
      .child(Separator.new())
      .child(
        h_flex()
          .px_4()
          .py_3()
          .child(div().flex_1().text_base().font_semibold().text_color(t.foreground).child("Total"))
          .child(div().test_id("order-total").text_base().font_semibold().text_color(t.foreground).child(money(total))),
      );
  }

  tabBar(cx) {
    const t = cx.theme();
    const unread = this.unread.size;
    const item = (id, name, label, badge) =>
      v_flex()
        .test_id(`tab-${id}`)
        .flex_1()
        .items_center()
        .gap_1()
        .py_2()
        .accessibility_label(label)
        .child(badge.child(icon(name, id === "inbox" ? t.primary : t.muted_foreground, 26)))
        .child(div().text_xs().font_medium().text_color(id === "inbox" ? t.primary : t.muted_foreground).child(label));
    return h_flex()
      .border_t_1()
      .border_color(t.border)
      .bg(t.surface)
      .child(item("inbox", "mail", "Inbox", Badge.new().count(unread).test_id("inbox-badge")))
      .child(item("updates", "bell", "Updates", Badge.new().count(128).test_id("updates-badge")))
      .child(item("account", "user", "Account", this.state === 2 ? Badge.new().dot().test_id("account-badge") : Badge.new()));
  }

  inbox(cx) {
    const t = cx.theme();
    if (this.archived) {
      return Empty.new()
        .test_id("inbox-empty")
        .header(
          EmptyHeader.new()
            .media(EmptyMedia.new().variant("icon").child(icon("mail", t.muted_foreground, 32)))
            .title(EmptyTitle.new().child("No Messages"))
            .description(EmptyDescription.new().child("Messages from sellers and couriers about your orders appear here.")),
        )
        .content(
          EmptyContent.new().child(
            button(cx, "contact-support", "Contact Support", { variant: "ghost", on_click: (_e, cx) => cx.push_toast({ title: "Support will reply within a day", level: "info", timeout: 2000 }) }),
          ),
        );
    }
    return v_flex()
      .children(
        MESSAGES.flatMap((m, i) => {
          const isUnread = this.unread.has(m.id);
          const avatar = h_flex()
            .size_10()
            .justify_center()
            .rounded_full()
            .bg(t.secondary)
            .text_sm()
            .font_semibold()
            .text_color(t.secondary_foreground)
            .child(m.initials);
          return [
            i > 0 ? Separator.new().ml(72) : null,
            h_flex()
              .test_id(`message-${m.id}`)
              .gap_3()
              .px_4()
              .py_3()
              .accessibility_label(`${isUnread ? "Unread, " : ""}${m.from}, ${m.text}, ${m.time}`)
              .child(m.online ? Badge.new().dot().color("success").child(avatar) : avatar)
              .child(
                v_flex()
                  .flex_1()
                  .min_w_0()
                  .child(
                    h_flex()
                      .gap_2()
                      .child(div().flex_1().text_base().font_semibold().text_color(t.foreground).truncate().child(m.from))
                      .child(div().text_sm().text_color(t.muted_foreground).child(m.time)),
                  )
                  .child(div().text_sm().text_color(t.muted_foreground).child(m.text)),
              )
              .when(isUnread, (el) => el.child(div().size_2p5().flex_none().rounded_full().bg(t.primary))),
          ];
        }),
      )
      .child(Separator.new())
      .child(
        h_flex()
          .px_2()
          .py_1()
          .child(button(cx, "mark-read", "Mark All as Read", { variant: "ghost", disabled: this.unread.size === 0, on_click: (_e, cx) => {
            this.unread.clear();
            cx.haptic("selection");
            cx.notify();
          } }))
          .child(div().flex_1())
          .child(button(cx, "archive-all", "Archive All", { variant: "ghost", danger: true, on_click: (_e, cx) => this.archiveAll(cx) })),
      );
  }

  picks(cx) {
    const t = cx.theme();
    if (this.loading) {
      return v_flex()
        .test_id("picks-loading")
        .accessibility_label("Loading recommendations")
        .gap_4()
        .p_4()
        .child(ShimmerText.new("Finding picks for you…").text_sm())
        .children(
          [0, 1, 2].map(() =>
            h_flex()
              .gap_3()
              .child(Skeleton.new().size_12().rounded_lg())
              .child(v_flex().flex_1().gap_2().child(Skeleton.new().w_3_4()).child(Skeleton.new().secondary().w_1_3())),
          ),
        );
    }
    return v_flex()
      .test_id("picks")
      .children(
        PICKS.flatMap((p, i) => [
          i > 0 ? Separator.new().ml(76) : null,
          h_flex()
            .gap_3()
            .px_4()
            .py_3()
            .child(h_flex().size_12().justify_center().rounded_lg().bg(t.muted).child(icon("package", t.muted_foreground, 22)))
            .child(
              v_flex()
                .flex_1()
                .min_w_0()
                .child(div().text_base().text_color(t.foreground).child(p.name))
                .child(div().text_sm().text_color(t.muted_foreground).child(money(p.price))),
            )
            .when(i === 0, (el) => el.child(Tag.new().variant("info").size("small").rounded_full().child("New"))),
        ]),
      );
  }

  render(cx) {
    const t = cx.theme();
    return page(cx, {
      title: "Status",
      intro: "A shop’s order screen. Each delivery state gets a tag and a banner in its color; counts sit on the tabs, and lists show still placeholders while they load.",
      children: [
        v_flex()
          .gap_3()
          .child(heading(cx, "Delivery", "Preview how the order reads in each state."))
          .child(
            Tabs.new("delivery-state")
              .test_id("delivery-state")
              .accessibility_label("Delivery state")
              .items(STATES)
              .selected(this.state)
              .on_change((i, cx) => {
                this.state = i;
                this.delayDismissed = false;
                cx.haptic("selection");
                cx.notify();
              }),
          )
          .child(this.orderCard(cx))
          .child(
            Notification.new("notify-delivery", "Notify Me on Delivery", (message, cx) =>
              cx.push_toast({ title: "Couldn’t turn on alerts", description: message, level: "error" }),
            )
              .test_id("notify-delivery")
              .title("Delivery alerts on")
              .message("We’ll tell you when order #4821 arrives.")
              .type("success"),
          ),
        v_flex()
          .gap_3()
          .child(heading(cx, "Messages", "The tab bar counts unread messages; a dot marks who is online."))
          .child(card(cx).child(this.inbox(cx)).child(this.tabBar(cx))),
        v_flex()
          .gap_3()
          .child(
            h_flex()
              .gap_3()
              .child(div().flex_1().child(heading(cx, "Recommended for you")))
              .child(button(cx, "refresh-picks", "Refresh", { variant: "ghost", loading: this.loading, on_click: (_e, cx) => this.load(cx) })),
          )
          .child(card(cx).child(this.picks(cx))),
        v_flex()
          .gap_3()
          .child(heading(cx, "Keyboard shortcuts", "With a hardware keyboard connected."))
          .child(
            card(cx).children(
              SHORTCUTS.flatMap((s, i) => [
                i > 0 ? Separator.new().ml_4() : null,
                h_flex()
                  .px_4()
                  .py_3()
                  .min_h_11()
                  .child(div().flex_1().text_base().text_color(t.foreground).child(s.label))
                  .child(Kbd.new(s.keys)),
              ]),
            ),
          ),
      ],
    });
  }
}
