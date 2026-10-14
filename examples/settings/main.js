// Settings: the preferences screen of a typical app.
//
// Rows are grouped by what they affect, each group says in one line what its
// switches do, and every change applies at once (no Save on this page).
// Dependent settings stay visible but disabled, with the reason beneath them,
// rather than disappearing. The account opens as a pushed page; clearing the
// cache asks first because the files have to be downloaded again. See
// README.md for the design notes.

import { View, div, v_flex, Button, Switch, Select, DatePicker } from "native-shell";
import AccountPage from "./account.js";
import { load, save, formatBytes, formatTime } from "./store.js";
import { avatar, icon, largeTitle, row, section, statusBar } from "./ui.js";

const THEMES = [
  { label: "System", value: "system" },
  { label: "Light", value: "light" },
  { label: "Dark", value: "dark" },
];
const VERSION = "2.4.0 (318)";
const HELP_URL = "https://example.org/help";

export default class Settings extends View {
  init(_props, cx) {
    this.settings = load();
    this.applyTheme(cx);
    /** Bytes in the cache, or null while it is being measured. */
    this.cacheBytes = null;
    this.measureCache(cx);
  }

  set(patch, cx) {
    this.settings = { ...this.settings, ...patch };
    save(this.settings);
    cx.notify();
  }

  applyTheme(cx) {
    const theme = this.settings.theme;
    // "System" hands the appearance back to the device.
    cx.set_theme(theme === "system" ? {} : { appearance: theme });
  }

  // Measuring a cache directory is real I/O in an app; here a short task
  // stands in for it, so the row shows its measuring state honestly.
  measureCache(cx) {
    cx.spawn(async (cx) => {
      await cx.sleep(600);
      this.cacheBytes = 184_300_000;
      cx.notify();
    });
  }

  confirmClearCache(cx) {
    const size = formatBytes(this.cacheBytes);
    cx.alert({
      title: `Clear ${size} of cached files?`,
      message: "Photos and attachments will download again the next time you open them.",
      actions: [
        { label: "Cancel", style: "cancel" },
        {
          label: "Clear cache",
          style: "destructive",
          on_press: (cx) => {
            this.cacheBytes = 0;
            cx.notify();
          },
        },
      ],
    });
  }

  openAccount(cx) {
    cx.navigator.push(
      AccountPage,
      {
        account: this.settings.account,
        // The page's `cx` notifies the page, so the settings below it are
        // redrawn explicitly to show the new account when it pops.
        save: (account, cx) => {
          this.set({ account }, cx);
          window.refresh();
        },
        signOut: (cx) => {
          this.set({ account: null }, cx);
          window.refresh();
        },
      },
      { title: this.settings.account ? "Account" : "Sign in" },
    );
  }

  render(cx) {
    const t = cx.theme();
    const s = this.settings;
    const summaryAvailable = s.notifications;
    return v_flex()
      .size_full()
      .bg(t.background)
      .line_height(1.375)
      .child(statusBar(cx))
      .child(
        v_flex()
          .test_id("settings")
          .id("settings")
          .flex_1()
          .overflow_y_scroll()
          .child(largeTitle(cx, "Settings"))
          .child(
            v_flex()
              .px_4()
              .pt_2()
              .pb(cx.window().safe_area.bottom + 32)
              .gap_6()
              .child(this.accountCard(cx))
              .child(
                section(cx, {
                  title: "Notifications",
                  footer: summaryAvailable
                    ? "The summary lists what changed since the day before."
                    : "Allow notifications to receive the daily summary.",
                  rows: [
                    row(cx, {
                      id: "notifications-row",
                      icon: "bell",
                      label: "Allow notifications",
                      trailing: Switch.new("notifications")
                        .test_id("notifications")
                        .accessibility_label("Allow notifications")
                        .checked(s.notifications)
                        .on_change((on, cx) => this.set({ notifications: on }, cx)),
                    }),
                    row(cx, {
                      id: "summary-row",
                      icon: "clock",
                      label: "Daily summary",
                      disabled: !summaryAvailable,
                      trailing: Switch.new("summary")
                        .test_id("summary")
                        .accessibility_label("Daily summary")
                        .checked(summaryAvailable && s.summary)
                        .disabled(!summaryAvailable)
                        .on_change((on, cx) => this.set({ summary: on }, cx)),
                    }),
                    summaryAvailable && s.summary
                      ? row(cx, {
                          id: "summary-time-row",
                          label: "Delivered at",
                          trailing: DatePicker.new("summary-time")
                            .test_id("summary-time")
                            .accessibility_label(`Delivered at ${formatTime(s.summaryTime)}`)
                            .min_w_32()
                            .mode("time")
                            .value(s.summaryTime)
                            // Pickers report `HH:mm`, or a full timestamp; the time is the last five characters either way.
                            .on_change((value, cx) => this.set({ summaryTime: value.slice(-5) }, cx)),
                        })
                      : null,
                  ],
                }),
              )
              .child(
                section(cx, {
                  title: "Appearance",
                  rows: [
                    row(cx, {
                      id: "theme-row",
                      icon: "moon",
                      label: "Theme",
                      trailing: Select.new("theme")
                        .test_id("theme")
                        .accessibility_label("Theme")
                        .min_w_32()
                        .items(THEMES)
                        .value(s.theme)
                        .on_change((value, cx) => {
                          this.set({ theme: value }, cx);
                          this.applyTheme(cx);
                        }),
                    }),
                  ],
                }),
              )
              .child(
                section(cx, {
                  title: "Privacy",
                  footer: "Anonymous counts of which features are used. Your notes and contacts are never included.",
                  rows: [
                    row(cx, {
                      id: "usage-row",
                      icon: "chart",
                      label: "Share usage data",
                      trailing: Switch.new("usage")
                        .test_id("usage")
                        .accessibility_label("Share usage data")
                        .checked(s.usageData)
                        .on_change((on, cx) => this.set({ usageData: on }, cx)),
                    }),
                  ],
                }),
              )
              .child(this.storageSection(cx))
              .child(
                section(cx, {
                  title: "About",
                  rows: [
                    row(cx, { id: "version", icon: "info", label: "Version", value: VERSION }),
                    row(cx, {
                      id: "help",
                      icon: "help",
                      label: "Help center",
                      trailing: icon("external", t.muted_foreground, 18),
                      on_click: (_e, cx) => cx.open_url(HELP_URL),
                    }),
                  ],
                }),
              ),
          ),
      );
  }

  accountCard(cx) {
    const t = cx.theme();
    const account = this.settings.account;
    return Button.new("account")
      .test_id("account")
      .accessibility_label(account ? `Account, ${account.name}, ${account.email}` : "Sign in")
      .flex()
      .items_center()
      .gap_3()
      .p_4()
      .rounded_xl()
      .border_1()
      .border_color(t.border)
      .bg(t.surface)
      .active((s) => s.bg(t.muted))
      .on_click((_e, cx) => this.openAccount(cx))
      .child(
        account
          ? avatar(cx, account.name)
          : div()
              .size_12()
              .flex_none()
              .rounded_full()
              .bg(t.muted)
              .flex()
              .items_center()
              .justify_center()
              .child(icon("user", t.muted_foreground, 24)),
      )
      .child(
        v_flex()
          .flex_1()
          .min_w_0()
          .gap_0p5()
          .child(
            div()
              .test_id("account-name")
              .text_lg()
              .font_semibold()
              .text_color(t.foreground)
              .truncate()
              .child(account ? account.name : "Sign in"),
          )
          .child(
            div()
              .text_sm()
              .text_color(t.muted_foreground)
              .truncate()
              .child(account ? account.email : "Keep your settings in sync across devices"),
          ),
      )
      .child(icon("chevron_right", t.muted_foreground, 18));
  }

  storageSection(cx) {
    const t = cx.theme();
    const measuring = this.cacheBytes === null;
    const empty = this.cacheBytes === 0;
    return section(cx, {
      title: "Storage",
      footer: "Cached files make photos and attachments open faster. Your data is not affected.",
      rows: [
        row(cx, {
          id: "cache",
          icon: "database",
          label: "Cached files",
          value: measuring ? "Measuring…" : formatBytes(this.cacheBytes),
        }),
        Button.new("clear-cache")
          .test_id("clear-cache")
          .disabled(measuring || empty)
          .pl(48)
          .pr_4()
          .py_3()
          .min_h_12()
          .flex()
          .items_center()
          .text_base()
          .text_color(measuring || empty ? t.muted_foreground : t.destructive)
          .when(!measuring && !empty, (el) => el.active((s) => s.bg(t.muted)).on_click((_e, cx) => this.confirmClearCache(cx)))
          .child("Clear cache…"),
      ],
    });
  }
}
