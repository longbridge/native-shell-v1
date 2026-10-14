// Settings: the preferences screen of a typical app, laid out as iOS Settings.
//
// A large title that collapses as the list scrolls, the account first, then
// inset grouped sections ordered by how often they are visited. Rows carry a
// colored icon tile, every switch and picker applies at once (no Save here),
// and a footer says in one sentence what a group's switches do. Dependent
// settings stay visible but disabled, with the reason beneath them. The
// account opens as a pushed page; clearing the cache asks first because the
// files have to be downloaded again. See README.md for the design notes.

import { View, div, Switch, Select, DatePicker } from "native-shell";
import AccountPage from "./account.js";
import { load, save, formatBytes, formatTime } from "./store.js";
import { avatar, icon, row, scrollScreen, section, tile } from "./ui.js";
/** @import { Context } from "native-shell" */
/** @import { Settings as Prefs } from "./store.js" */

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
    this.collapsed = false;
    /** Bytes in the cache, or null while it is being measured. @type {number | null} */
    this.cacheBytes = null;
    this.measureCache(cx);
  }

  /** @param {Partial<Prefs>} patch @param {Context} cx */
  set(patch, cx) {
    this.settings = { ...this.settings, ...patch };
    save(this.settings);
    cx.notify();
  }

  /** @param {Context} cx */
  applyTheme(cx) {
    const theme = this.settings.theme;
    // "System" hands the appearance back to the device.
    cx.set_theme(theme === "system" ? {} : { appearance: theme === "dark" ? "dark" : "light" });
  }

  // Measuring a cache directory is real I/O in an app; here a short task
  // stands in for it, so the row shows its measuring state honestly.
  /** @param {Context} cx */
  measureCache(cx) {
    cx.spawn(async (cx) => {
      await cx.sleep(600);
      this.cacheBytes = 184_300_000;
      cx.notify();
    });
  }

  /** @param {Context} cx */
  confirmClearCache(cx) {
    cx.alert({
      title: `Clear ${formatBytes(this.cacheBytes ?? 0)} of cached files?`,
      message: "Photos and attachments will download again the next time you open them.",
      actions: [
        { label: "Cancel", style: "cancel" },
        {
          label: "Clear Cache",
          style: "destructive",
          on_press: (cx) => {
            this.cacheBytes = 0;
            cx.haptic("success");
            cx.notify();
          },
        },
      ],
    });
  }

  /** @param {Context} cx */
  openAccount(cx) {
    cx.navigator.push(
      AccountPage,
      {
        account: this.settings.account,
        save: (/** @type {Prefs["account"]} */ account, /** @type {Context} */ cx) => this.set({ account }, cx),
        signOut: (/** @type {Context} */ cx) => this.set({ account: null }, cx),
      },
      { title: this.settings.account ? "Account" : "Sign In" },
    );
  }

  render(cx) {
    const t = cx.theme();
    const s = this.settings;
    const summaryAvailable = s.notifications;
    return scrollScreen(cx, {
      id: "settings",
      title: "Settings",
      background: t.grouped_background,
      collapsed: this.collapsed,
      on_collapse: (collapsed, cx) => {
        this.collapsed = collapsed;
        cx.notify();
      },
      children: [
        section(cx, { rows: [this.accountRow(cx)] }),
        section(cx, {
          inset: 60,
          footer: summaryAvailable
            ? "The summary lists what changed since the day before."
            : "Allow notifications to receive the daily summary.",
          rows: [
            row(cx, {
              id: "notifications-row",
              leading: tile(cx, "bell", t.destructive),
              label: "Notifications",
              trailing: Switch.new("notifications")
                .test_id("notifications")
                .accessibility_label("Notifications")
                .checked(s.notifications)
                .on_change((on, cx) => this.set({ notifications: on }, cx)),
            }),
            row(cx, {
              id: "summary-row",
              leading: tile(cx, "clock", t.warning),
              label: "Daily Summary",
              disabled: !summaryAvailable,
              trailing: Switch.new("summary")
                .test_id("summary")
                .accessibility_label("Daily summary")
                .checked(summaryAvailable && s.summary)
                .disabled(!summaryAvailable)
                .on_change((on, cx) => this.set({ summary: on }, cx)),
            }),
            summaryAvailable &&
              s.summary &&
              row(cx, {
                id: "summary-time-row",
                leading: div().size_7().flex_none(),
                label: "Delivered At",
                trailing: DatePicker.new("summary-time")
                  .test_id("summary-time")
                  .accessibility_label(`Delivered at ${formatTime(s.summaryTime)}`)
                  .mode("time")
                  .value(s.summaryTime)
                  .on_change((value, cx) => this.set({ summaryTime: value }, cx)),
              }),
          ],
        }),
        section(cx, {
          inset: 60,
          rows: [
            row(cx, {
              id: "theme-row",
              leading: tile(cx, "moon", t.primary),
              label: "Appearance",
              trailing: Select.new("theme")
                .test_id("theme")
                .accessibility_label("Appearance")
                .items(THEMES)
                .value(s.theme)
                .on_change((value, cx) => {
                  this.set({ theme: /** @type {Prefs["theme"]} */ (value) }, cx);
                  this.applyTheme(cx);
                }),
            }),
            row(cx, {
              id: "usage-row",
              leading: tile(cx, "hand", t.info),
              label: "Share Usage Data",
              trailing: Switch.new("usage")
                .test_id("usage")
                .accessibility_label("Share usage data")
                .checked(s.usageData)
                .on_change((on, cx) => this.set({ usageData: on }, cx)),
            }),
          ],
          footer: "Anonymous counts of which features are used. Your notes and contacts are never included.",
        }),
        this.storageSection(cx),
        section(cx, {
          inset: 60,
          rows: [
            row(cx, { id: "version", leading: tile(cx, "info", t.muted_foreground), label: "Version", value: VERSION }),
            row(cx, {
              id: "help",
              leading: tile(cx, "help", t.muted_foreground),
              label: "Help Center",
              trailing: icon("external", t.muted_foreground, 16),
              on_click: (_e, cx) => cx.open_url(HELP_URL),
            }),
          ],
        }),
      ],
    });
  }

  /** The account row leads the screen: a photo-sized avatar, the name and email, and a chevron. @param {Context} cx */
  accountRow(cx) {
    const t = cx.theme();
    const account = this.settings.account;
    return row(cx, {
      id: "account",
      leading: account ? avatar(cx, account.name, 60) : tile(cx, "user", t.muted_foreground),
      label: account ? account.name : "Sign In",
      detail: account ? account.email : "Keep your settings in sync across devices.",
      chevron: true,
      on_click: (_e, cx) => this.openAccount(cx),
    });
  }

  /** @param {Context} cx */
  storageSection(cx) {
    const t = cx.theme();
    const measuring = this.cacheBytes === null;
    const empty = this.cacheBytes === 0;
    return section(cx, {
      inset: 60,
      footer: "Cached files make photos and attachments open faster. Your data is not affected.",
      rows: [
        row(cx, {
          id: "cache",
          leading: tile(cx, "database", t.success),
          label: "Cached Files",
          value: measuring ? "Measuring…" : formatBytes(this.cacheBytes ?? 0),
        }),
        row(cx, {
          id: "clear-cache",
          leading: div().size_7().flex_none(),
          label: "Clear Cache…",
          destructive: true,
          disabled: measuring || empty,
          on_click: (_e, cx) => this.confirmClearCache(cx),
        }),
      ],
    });
  }
}
