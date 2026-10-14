// Settings, as an app's notification preferences: two pages (messages and
// calendar) on a segmented control, each a few inset grouped sections whose
// footers say what the switches do. Turning notifications off leaves the
// rows that depend on it visible but disabled, with the reason.

import { View, Select, Switch } from "native-shell";
import { Settings, SettingGroup, SettingItem, SettingPage } from "gpui-component";
import { page } from "../ui.js";

const PREVIEWS = [
  { label: "Always", value: "always" },
  { label: "When Unlocked", value: "unlocked" },
  { label: "Never", value: "never" },
];
const ALERTS = [
  { label: "At Time of Event", value: "0" },
  { label: "5 Minutes Before", value: "5" },
  { label: "15 Minutes Before", value: "15" },
  { label: "1 Hour Before", value: "60" },
];

export default class NotificationsPage extends View {
  init() {
    this.prefs = { allow: true, sounds: true, previews: "unlocked", summary: false, invitations: true, alert: "15", travel: false };
  }

  /** @param {Partial<typeof this.prefs>} patch @param {import("native-shell").Context} cx */
  set(patch, cx) {
    this.prefs = { ...this.prefs, ...patch };
    cx.notify();
  }

  /** @param {string} id @param {keyof typeof this.prefs} key */
  toggle(id, key) {
    return Switch.new(id)
      .test_id(id)
      .checked(Boolean(this.prefs[key]))
      .on_change((on, cx) => this.set({ [key]: on }, cx));
  }

  render(cx) {
    const p = this.prefs;
    const needsAllow = p.allow ? undefined : "Turn on notifications to use this.";
    const messages = new SettingPage("Messages")
      .child(
        new SettingGroup()
          .title("Alerts")
          .description("Previews show the first line of a message on the Lock Screen.")
          .child(new SettingItem("Allow Notifications").content(this.toggle("allow", "allow")))
          .child(
            new SettingItem("Sounds")
              .description(needsAllow ?? "A short chime for each new message.")
              .disabled(!p.allow)
              .content(this.toggle("sounds", "sounds")),
          )
          .child(
            new SettingItem("Show Previews")
              .disabled(!p.allow)
              .content(
                Select.new("previews")
                  .test_id("previews")
                  .items(PREVIEWS)
                  .value(p.previews)
                  .on_change((v, cx) => this.set({ previews: String(v) }, cx)),
              ),
          ),
      )
      .child(
        new SettingGroup()
          .title("Summary")
          .description("Messages from people you don't know wait for the summary instead of alerting you.")
          .child(
            new SettingItem("Scheduled Summary")
              .description(needsAllow ?? "Every morning at 8:00 AM.")
              .disabled(!p.allow)
              .content(this.toggle("summary", "summary")),
          ),
      );
    const calendar = new SettingPage("Calendar")
      .child(
        new SettingGroup()
          .title("Events")
          .child(new SettingItem("Invitations").content(this.toggle("invitations", "invitations")))
          .child(
            new SettingItem("Default Alert").content(
              Select.new("alert")
                .test_id("alert")
                .items(ALERTS)
                .value(p.alert)
                .on_change((v, cx) => this.set({ alert: String(v) }, cx)),
            ),
          ),
      )
      .child(
        new SettingGroup()
          .title("Travel")
          .description("Alerts move earlier by the time it takes to get there, from the event's location.")
          .child(new SettingItem("Travel Time").content(this.toggle("travel", "travel"))),
      );
    return page(cx, {
      title: "Settings",
      intro: "Notification settings for messages and the calendar. Changes apply at once.",
      children: [new Settings("notifications").test_id("settings").child(messages).child(calendar)],
    });
  }
}
