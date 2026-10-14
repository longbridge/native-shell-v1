// The conversation: messages, the history that loads on request, and how
// times read. Everything is local; `fetchEarlier` stands in for a network
// page request and takes a moment, as one would.

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** @typedef {{ id: string, from: "me" | "them", text: string, at: number, status?: "sending" | "sent" | "read" | "failed" }} Message */

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
  return [
    { id: "m1", from: "them", text: "Are we still on for the hike on Saturday?", at: t - DAY - 3 * HOUR },
    {
      id: "m2",
      from: "me",
      text: "Yes! I was thinking the ridge trail, the one with the lake at the top.",
      at: t - DAY - 3 * HOUR + 4 * MINUTE,
    },
    { id: "m3", from: "them", text: "Perfect. How long is it?", at: t - DAY - 3 * HOUR + 6 * MINUTE },
    { id: "m4", from: "me", text: "About 14 km round trip, so roughly five hours with a lunch stop.", at: t - DAY - 3 * HOUR + 7 * MINUTE },
    { id: "m5", from: "them", text: "Forecast says sunny and 18° in the morning.", at: t },
    { id: "m6", from: "them", text: "Should we take the 8:15 train? It gets us to the trailhead just before nine.", at: t + MINUTE },
    { id: "m7", from: "me", text: "8:15 works. I’ll bring sandwiches and the first-aid kit.", at: t + 9 * MINUTE, status: "read" },
    {
      id: "m8",
      from: "me",
      text: "Can you check whether the upper trail reopened after last week’s storm?",
      at: t + 10 * MINUTE,
      status: "failed",
    },
  ];
}

/** @type {{ from: Message["from"], text: string, days: number, h: number, m: number }[]} */
const EARLIER = [
  { from: "them", text: "Finished the book you lent me. That ending!", days: 6, h: 20, m: 4 },
  { from: "me", text: "Right? I had to read the last chapter twice.", days: 6, h: 20, m: 11 },
  { from: "them", text: "I’ll bring it back on Thursday, if you’re around.", days: 6, h: 20, m: 12 },
  { from: "me", text: "Thursday is good. Come by after six.", days: 6, h: 20, m: 30 },
  { from: "them", text: "Thanks again for the book. Next one is on me.", days: 4, h: 18, m: 45 },
];

let earlierServed = false;

/**
 * One page of older messages, oldest first, and whether more exist.
 * @param {import("native-shell").AsyncContext} cx
 * @returns {Promise<{ messages: Message[], more: boolean }>}
 */
export async function fetchEarlier(cx) {
  await cx.sleep(900);
  if (earlierServed) return { messages: [], more: false };
  earlierServed = true;
  const base = today(0, 0);
  /** @type {Message[]} */
  const messages = EARLIER.map((e, i) => ({
    id: `e${i}`,
    from: e.from,
    text: e.text,
    at: base - e.days * DAY + e.h * HOUR + e.m * MINUTE,
  }));
  return { messages, more: false };
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const WEEKDAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

export function clock(at) {
  const d = new Date(at);
  const h = d.getHours();
  return `${((h + 11) % 12) + 1}:${String(d.getMinutes()).padStart(2, "0")} ${h < 12 ? "AM" : "PM"}`;
}

/** The label of the day a message belongs to: "Today", "Yesterday", a weekday, then a date. */
export function dayLabel(at) {
  const days = Math.round((today(0, 0) - new Date(at).setHours(0, 0, 0, 0)) / DAY);
  const d = new Date(at);
  if (days === 0) return "Today";
  if (days === 1) return "Yesterday";
  if (days < 7) return WEEKDAYS[d.getDay()];
  return `${MONTHS[d.getMonth()]} ${d.getDate()}`;
}

/** Whether two adjacent messages read as one burst: same sender, a few minutes apart. */
export function sameBurst(a, b) {
  return a !== undefined && b !== undefined && a.from === b.from && Math.abs(b.at - a.at) < 5 * MINUTE;
}
