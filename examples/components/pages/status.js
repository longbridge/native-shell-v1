// Status and empty states, in a shop's order screen: a status tag and a
// banner for each delivery state (on time, delayed, payment declined), badges
// on a tab bar and on avatars, an inbox that can be emptied (and undone), a
// list that shows still placeholders while it loads, and the shortcuts a
// hardware keyboard offers.

import { View, div, h_flex, v_flex, Tabs } from "native-shell";
import {
  SuccessAlert, WarningAlert, ErrorAlert, Badge, Tag, Skeleton, Empty, EmptyHeader, EmptyMedia, EmptyTitle,
  EmptyDescription, EmptyContent, Kbd, Label, Separator, ShimmerText, Notification,
} from "gpui-component";
import { filledButton, icon, label, page, row, section, textButton, textStyle } from "../ui.js";
/** @import { Context, Task } from "native-shell" */

const STATES = ["On Time", "Delayed", "Declined"];

const ITEMS = [
  { name: "Field notebook, dot grid", qty: 2, price: 24 },
  { name: "Brass pocket pen", qty: 1, price: 38 },
  { name: "Canvas pencil roll", qty: 1, price: 19 },
];

const MESSAGES = [
  { id: "courier", from: "Northline Courier", initials: "NC", text: "Your parcel is at the Memphis hub.", time: "9:41", online: true },
  { id: "seller", from: "Fieldnotes Supply", initials: "FS", text: "Thanks for your order! Packing it today.", time: "Yesterday", online: false },
  { id: "support", from: "Store Support", initials: "SS", text: "Your return label is ready to print.", time: "Mon", online: true },
];

const PICKS = [
  { id: "ink", name: "Iron gall ink, 50 ml", price: 16, fresh: true },
  { id: "folio", name: "Leather folio", price: 64, fresh: false },
  { id: "clips", name: "Brass binder clips", price: 9, fresh: false },
];

const SHORTCUTS = [
  { id: "search", label: "Search Orders", keys: "cmd-f" },
  { id: "compose", label: "New Message", keys: "cmd-n" },
  { id: "archive", label: "Archive Message", keys: "cmd-backspace" },
  { id: "refresh", label: "Refresh", keys: "cmd-r" },
];

/** @param {number} n */
const money = (n) => `$${n.toFixed(2)}`;

export default class StatusPage extends View {
  /** @param {unknown} _props @param {Context} cx */
  init(_props, cx) {
    this.state = 1;
    this.delayDismissed = false;
    this.unread = new Set(["courier", "seller"]);
    this.archived = false;
    this.loading = false;
    /** @type {Task | null} */
    this.task = null;
    this.load(cx);
  }

  release() {
    this.task?.cancel();
  }

  /** Stands in for fetching recommendations. @param {Context} cx */
  load(cx) {
    this.loading = true;
    this.task?.cancel();
    this.task = cx.timer.after(1200, (cx) => {
      this.loading = false;
      cx.notify();
    });
    cx.notify();
  }

  /** @param {Context} cx */
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

  /** The order's header row: number, status tag and the banner for its state. @param {Context} cx */
  orderStatus(cx) {
    const t = cx.theme();
    const tag = [
      Tag.new().variant("success").rounded_full().child("Shipped"),
      Tag.new().variant("warning").rounded_full().child("Delayed"),
      Tag.new().variant("danger").rounded_full().child("On Hold"),
    ][this.state];
    const banner = [
      SuccessAlert.new("on-time", "Arrives Thursday, October 16, by 8 PM.").title("On Its Way"),
      WarningAlert.new("delayed", "A storm held your parcel in Memphis. New estimate: Saturday, October 18.")
        .title("Running Late")
        .visible(!this.delayDismissed)
        .on_close((_e, cx) => {
          this.delayDismissed = true;
          cx.notify();
        }),
      ErrorAlert.new("declined", "Your card ending 4242 was declined. Update it by Friday to keep your order.").title(
        "Payment Declined",
      ),
    ][this.state];
    return v_flex()
      .gap_3()
      .px_4()
      .py_3()
      .child(
        h_flex()
          .gap_3()
          .child(
            v_flex()
              .flex_1()
              .min_w_0()
              .child(label(cx, "headline", t.foreground, "Order #4821").role("header"))
              .child(label(cx, "subheadline", t.muted_foreground, "Placed October 9 · 4 items")),
          )
          .child(tag.test_id("order-tag")),
      )
      .child(banner.test_id("order-alert"));
  }

  /** @param {Context} cx */
  summary(cx) {
    const t = cx.theme();
    const subtotal = ITEMS.reduce((sum, i) => sum + i.qty * i.price, 0);
    /** @param {string} text @param {string} value @param {boolean} [strong] */
    const line = (text, value, strong = false) =>
      h_flex()
        .justify_between()
        .child(label(cx, strong ? "headline" : "body", strong ? t.foreground : t.muted_foreground, text))
        .child(label(cx, strong ? "headline" : "body", t.foreground, value).when(strong, (el) => el.test_id("order-total")));
    return v_flex()
      .gap_2()
      .px_4()
      .py_3()
      .child(line("Subtotal", money(subtotal)))
      .child(line("Shipping", "Free"))
      .child(Separator.new().my_1())
      .child(line("Total", money(subtotal), true));
  }

  /** @param {Context} cx */
  tabBar(cx) {
    const t = cx.theme();
    /** @param {string} id @param {"mail" | "bell" | "user"} name @param {string} text @param {import("gpui-component").BadgeElement} badge */
    const item = (id, name, text, badge) => {
      const color = id === "inbox" ? t.primary : t.muted_foreground;
      return v_flex()
        .test_id(`tab-${id}`)
        .flex_1()
        .items_center()
        .gap_0p5()
        .pt_2()
        .pb_1p5()
        .child(badge.child(icon(name, color, 26)))
        .child(textStyle(div(), cx, "caption2").font_medium().text_color(color).child(text));
    };
    return h_flex()
      .child(item("inbox", "mail", "Inbox", Badge.new().count(this.unread.size).test_id("inbox-badge")))
      .child(item("updates", "bell", "Updates", Badge.new().count(128).test_id("updates-badge")))
      .child(item("account", "user", "Account", this.state === 2 ? Badge.new().dot().test_id("account-badge") : Badge.new()));
  }

  /** @param {Context} cx */
  inboxRows(cx) {
    const t = cx.theme();
    if (this.archived) {
      return [
        Empty.new()
          .test_id("inbox-empty")
          .header(
            EmptyHeader.new()
              .media(EmptyMedia.new().variant("icon").child(icon("mail", t.muted_foreground, 32)))
              .title(EmptyTitle.new().child("No Messages"))
              .description(EmptyDescription.new().child("Messages from sellers and couriers about your orders appear here.")),
          )
          .content(
            EmptyContent.new().child(
              textButton(cx, "contact-support", "Contact Support", {
                on_click: (_e, cx) => cx.push_toast({ title: "Support will reply within a day", level: "info", timeout: 2000 }),
              }),
            ),
          ),
      ];
    }
    const messages = MESSAGES.map((m) => {
      const unread = this.unread.has(m.id);
      const avatar = h_flex()
        .size_10()
        .justify_center()
        .rounded_full()
        .bg(t.secondary)
        .child(textStyle(div(), cx, "subheadline").font_semibold().text_color(t.secondary_foreground).child(m.initials));
      return h_flex()
        .test_id(`message-${m.id}`)
        .gap_3()
        .px_4()
        .py_2p5()
        .accessibility_label(`${unread ? "Unread, " : ""}${m.from}, ${m.text}, ${m.time}`)
        .child(m.online ? Badge.new().dot().color("success").child(avatar) : avatar)
        .child(
          v_flex()
            .flex_1()
            .min_w_0()
            .child(
              h_flex()
                .gap_2()
                .child(label(cx, "headline", t.foreground, m.from).flex_1().truncate())
                .child(label(cx, "subheadline", t.muted_foreground, m.time)),
            )
            .child(label(cx, "subheadline", t.muted_foreground, m.text)),
        )
        .child(div().size_2p5().flex_none().rounded_full().when(unread, (el) => el.bg(t.primary)));
    });
    return [
      ...messages,
      h_flex()
        .px_2()
        .child(
          textButton(cx, "mark-read", "Mark All as Read", {
            disabled: this.unread.size === 0,
            on_click: (_e, cx) => {
              this.unread.clear();
              cx.haptic("selection");
              cx.notify();
            },
          }),
        )
        .child(div().flex_1())
        .child(textButton(cx, "archive-all", "Archive All", { destructive: true, on_click: (_e, cx) => this.archiveAll(cx) })),
    ];
  }

  /** @param {Context} cx */
  pickRows(cx) {
    const t = cx.theme();
    if (this.loading) {
      const placeholder = () =>
        h_flex()
          .gap_3()
          .px_4()
          .py_3()
          .child(Skeleton.new().size_12().rounded(t.radius.md))
          .child(v_flex().flex_1().gap_2().child(Skeleton.new().w_3_4()).child(Skeleton.new().secondary().w_1_3()));
      return [
        div()
          .test_id("picks-loading")
          .px_4()
          .py_3()
          .child(textStyle(ShimmerText.new("Finding picks for you…"), cx, "subheadline")),
        placeholder(),
        placeholder(),
      ];
    }
    return PICKS.map((p) =>
      h_flex()
        .test_id(`pick-${p.id}`)
        .gap_3()
        .px_4()
        .py_3()
        .child(h_flex().size_12().justify_center().rounded(t.radius.md).bg(t.secondary).child(icon("package", t.muted_foreground, 22)))
        .child(
          v_flex()
            .flex_1()
            .min_w_0()
            .child(label(cx, "body", t.foreground, p.name))
            .child(label(cx, "subheadline", t.muted_foreground, money(p.price))),
        )
        .when(p.fresh, (el) => el.child(Tag.new().variant("info").size("small").rounded_full().child("New"))),
    );
  }

  /** @param {Context} cx */
  render(cx) {
    const t = cx.theme();
    return page(cx, {
      title: "Status",
      intro: "A shop’s order screen. Each delivery state gets a tag and a banner in its color; counts sit on the tabs, and lists show still placeholders while they load.",
      children: [
        v_flex()
          .gap_2()
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
          .child(label(cx, "footnote", t.muted_foreground, "Preview how the order reads in each state.").px_4()),
        v_flex()
          .gap_4()
          .child(
            section(cx, {
              id: "order",
              rows: [
                this.orderStatus(cx),
                ...ITEMS.map((item, i) =>
                  h_flex()
                    .test_id(`item-${i}`)
                    .gap_3()
                    .px_4()
                    .py_2p5()
                    .min_h_11()
                    .child(textStyle(Label.new(item.name).secondary(`× ${item.qty}`), cx, "body").flex_1().min_w_0().text_color(t.foreground))
                    .child(label(cx, "body", t.muted_foreground, money(item.qty * item.price)).flex_none()),
                ),
                this.summary(cx),
              ],
            }),
          )
          .child(
            this.state === 2
              ? filledButton(cx, "update-payment", "Update Payment Method", {
                  on_click: (_e, cx) => {
                    this.state = 0;
                    cx.haptic("success");
                    cx.push_toast({ title: "Payment updated", level: "success", timeout: 2000 });
                    cx.notify();
                  },
                })
              : textStyle(
                  Notification.new("notify-delivery", "Notify Me on Delivery", (message, cx) =>
                    cx.push_toast({ title: "Couldn’t turn on alerts", description: message, level: "error" }),
                  ),
                  cx,
                  "headline",
                )
                  .test_id("notify-delivery")
                  .min_h("3.125rem")
                  .title("Delivery alerts on")
                  .message("We’ll tell you when order #4821 arrives.")
                  .type("success"),
          ),
        section(cx, {
          header: "Messages",
          footer: "The Inbox tab counts unread messages; a green dot marks who is online.",
          inset: this.archived ? 0 : 68,
          rows: [...this.inboxRows(cx), this.tabBar(cx)],
        }),
        section(cx, {
          header: "Recommended for You",
          inset: this.loading ? 16 : 76,
          rows: [
            ...this.pickRows(cx),
            h_flex()
              .px_2()
              .child(
                textButton(cx, "refresh-picks", this.loading ? "Refreshing…" : "Refresh", {
                  disabled: this.loading,
                  on_click: (_e, cx) => this.load(cx),
                }),
              ),
          ],
        }),
        section(cx, {
          header: "Keyboard Shortcuts",
          footer: "With a hardware keyboard connected.",
          rows: SHORTCUTS.map((s) => row(cx, { id: `shortcut-${s.id}`, label: s.label, trailing: Kbd.new(s.keys) })),
        }),
      ],
    });
  }
}
