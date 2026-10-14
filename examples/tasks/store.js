// Tasks in localStorage, and the dates they are due.
//
// A first run starts with a few sample tasks so the list shows what it is for;
// once the user has saved anything, even an empty list, that is what loads.
// A corrupt entry is dropped rather than failing the program.

const KEY = "tasks.v1";

/** @typedef {{ id: number, title: string, done: boolean, due: string | null }} Task */

/** @returns {Task[]} */
export function load() {
  const raw = localStorage.getItem(KEY);
  if (raw === null) return samples();
  try {
    const items = JSON.parse(raw);
    if (!Array.isArray(items)) return [];
    return items
      .filter((i) => i && typeof i.id === "number" && typeof i.title === "string")
      .map((i) => ({ id: i.id, title: i.title, done: !!i.done, due: typeof i.due === "string" ? i.due : null }));
  } catch {
    return [];
  }
}

/** Answers whether the list was written; a full or unavailable store is reported, not thrown. */
export function save(items) {
  try {
    localStorage.setItem(KEY, JSON.stringify(items));
    return true;
  } catch {
    return false;
  }
}

function samples() {
  return [
    { id: 1, title: "Renew passport", done: false, due: isoDate(-2) },
    { id: 2, title: "Call the plumber about the kitchen sink", done: false, due: isoDate(0) },
    { id: 3, title: "Book a dentist appointment", done: false, due: isoDate(1) },
    { id: 4, title: "Return the library books", done: false, due: null },
    {
      id: 5,
      title: "Plan the weekend trip: train tickets, a room near the old town, and a dinner reservation for Saturday",
      done: false,
      due: isoDate(5),
    },
    { id: 6, title: "Pay the electricity bill", done: true, due: null },
  ];
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/** `YYYY-MM-DD` for today plus `offset` days, in local time. */
export function isoDate(offset = 0) {
  const d = new Date();
  d.setDate(d.getDate() + offset);
  const pad = (n) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

function parse(iso) {
  const [y, m, d] = iso.split("-").map(Number);
  return new Date(y, m - 1, d);
}

/** Days from today to `iso`: negative when it has passed. */
function daysUntil(iso) {
  const today = parse(isoDate(0));
  return Math.round((parse(iso).getTime() - today.getTime()) / 86400000);
}

/** How a due date reads in a row: relative near today, a short date otherwise. */
export function describeDue(iso) {
  const days = daysUntil(iso);
  const d = parse(iso);
  const short = `${MONTHS[d.getMonth()]} ${d.getDate()}`;
  if (days < 0) return { text: `Overdue · ${short}`, overdue: true };
  if (days === 0) return { text: "Today", overdue: false };
  if (days === 1) return { text: "Tomorrow", overdue: false };
  if (days < 7) return { text: `${WEEKDAYS[d.getDay()]}, ${short}`, overdue: false };
  return { text: short, overdue: false };
}
