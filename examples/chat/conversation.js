// The conversation: messages, the history that loads on request, and how
// times read. Everything is local; `fetchEarlier` stands in for a network
// page request and takes a moment, as one would.

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/**
 * A photo or a file sent with a message; `progress` (0–100) while it uploads.
 * @typedef {{ kind: "image" | "file", name: string, detail: string, src?: string, progress?: number }} Attachment
 */

/**
 * @typedef {{
 *   id: string,
 *   from: "me" | "them",
 *   text?: string,
 *   attachment?: Attachment,
 *   at: number,
 *   status?: "sending" | "sent" | "read" | "failed",
 *   unread?: boolean,
 * }} Message
 */

export const CONTACT = { name: "Maya Chen", status: "Active 5 min ago" };

/** Today at `h:m`, local time. */
function today(h, m) {
  const d = new Date();
  d.setHours(h, m, 0, 0);
  return d.getTime();
}

/** @returns {Message[]} */
export function recentMessages() {
  const t = today(9, 12);
  const yesterday = t - DAY - 3 * HOUR;
  return [
    { id: "m1", from: "them", text: "Are we still on for the hike on Saturday?", at: yesterday },
    {
      id: "m2",
      from: "me",
      text: "Yes! I was thinking the ridge trail, the one with the lake at the top.",
      at: yesterday + 4 * MINUTE,
    },
    {
      id: "m3",
      from: "me",
      attachment: { kind: "file", name: "ridge-trail.gpx", detail: "GPX · 84 KB" },
      at: yesterday + 4 * MINUTE,
      status: "failed",
    },
    { id: "m4", from: "them", text: "Perfect. How long is it?", at: yesterday + 6 * MINUTE },
    { id: "m5", from: "me", text: "About 14 km round trip, so roughly five hours with a lunch stop.", at: yesterday + 7 * MINUTE },
    { id: "m6", from: "them", text: "Forecast says sunny and 18° in the morning.", at: t },
    { id: "m7", from: "them", text: "Should we take the 8:15 train? It gets us to the trailhead just before nine.", at: t + MINUTE },
    { id: "m8", from: "me", text: "8:15 works. I’ll bring sandwiches and the first-aid kit.", at: t + 9 * MINUTE, status: "read" },
    {
      id: "m9",
      from: "me",
      text: "Should I book a table at the lodge for after? They fill up on weekends.",
      at: t + 10 * MINUTE,
      status: "failed",
    },
    {
      id: "m10",
      from: "them",
      text: "Called the ranger station: the upper trail reopened after last week’s storm, but the bridge past the lake is still closed, so we turn around at the top.",
      at: t + 31 * MINUTE,
      unread: true,
    },
    {
      id: "m11",
      from: "them",
      attachment: { kind: "image", name: "ridge-route.jpg", detail: "JPEG · 28 KB", src: "assets/ridge-route.jpg" },
      at: t + 32 * MINUTE,
      unread: true,
    },
    {
      id: "m12",
      from: "them",
      attachment: { kind: "file", name: "trail-conditions.pdf", detail: "PDF · 380 KB" },
      at: t + 32 * MINUTE,
      unread: true,
    },
  ];
}

/** Older history, newest page first. @type {{ from: Message["from"], text: string, days: number, h: number, m: number }[][]} */
const EARLIER = [
  [
    { from: "them", text: "Finished the book you lent me. That ending!", days: 6, h: 20, m: 4 },
    { from: "me", text: "Right? I had to read the last chapter twice.", days: 6, h: 20, m: 11 },
    { from: "them", text: "I’ll bring it back on Thursday, if you’re around.", days: 6, h: 20, m: 12 },
    { from: "me", text: "Thursday is good. Come by after six.", days: 6, h: 20, m: 30 },
    { from: "them", text: "Thanks again for the book. Next one is on me.", days: 4, h: 18, m: 45 },
  ],
  [
    { from: "me", text: "Happy birthday! Hope the day is as good as you are.", days: 12, h: 8, m: 2 },
    { from: "them", text: "Thank you!! Dinner with my sister tonight, the Thai place on Elm Street.", days: 12, h: 9, m: 40 },
    { from: "me", text: "Order the green curry. Trust me.", days: 12, h: 9, m: 41 },
    { from: "them", text: "Did you get home all right? The rain was unreal.", days: 9, h: 22, m: 15 },
    { from: "me", text: "Soaked, but yes. The umbrella gave up halfway.", days: 9, h: 22, m: 20 },
    { from: "them", text: "Want to borrow a book for the train next week?", days: 8, h: 7, m: 55 },
    { from: "me", text: "Yes please!", days: 8, h: 8, m: 3 },
  ],
];

let pagesServed = 0;

/**
 * One page of older messages, oldest first, and whether more exist.
 * @param {import("native-shell").AsyncContext} cx
 * @returns {Promise<{ messages: Message[], more: boolean }>}
 */
export async function fetchEarlier(cx) {
  await cx.sleep(900);
  const index = pagesServed++;
  const base = today(0, 0);
  /** @type {Message[]} */
  const messages = (EARLIER[index] ?? []).map((e, i) => ({
    id: `e${index}-${i}`,
    from: e.from,
    text: e.text,
    at: base - e.days * DAY + e.h * HOUR + e.m * MINUTE,
  }));
  return { messages, more: pagesServed < EARLIER.length };
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const WEEKDAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

/** @param {number} at */
export function clock(at) {
  const d = new Date(at);
  const h = d.getHours();
  return `${((h + 11) % 12) + 1}:${String(d.getMinutes()).padStart(2, "0")} ${h < 12 ? "AM" : "PM"}`;
}

/**
 * The label of the day a message belongs to: "Today", "Yesterday", a weekday, then a date.
 * @param {number} at
 */
export function dayLabel(at) {
  const days = Math.round((today(0, 0) - new Date(at).setHours(0, 0, 0, 0)) / DAY);
  const d = new Date(at);
  if (days === 0) return "Today";
  if (days === 1) return "Yesterday";
  if (days < 7) return WEEKDAYS[d.getDay()];
  return `${MONTHS[d.getMonth()]} ${d.getDate()}`;
}

/**
 * Whether two adjacent messages read as one burst: same sender, a few minutes apart.
 * @param {Message | undefined} a
 * @param {Message | undefined} b
 */
export function sameBurst(a, b) {
  return a !== undefined && b !== undefined && a.from === b.from && Math.abs(b.at - a.at) < 5 * MINUTE;
}

/**
 * How a message reads in its actions sheet and to VoiceOver.
 * @param {Message} m
 */
export function summary(m) {
  if (m.attachment === undefined) return m.text ?? "";
  return `${m.attachment.kind === "image" ? "Photo" : "File"}, ${m.attachment.name}`;
}
