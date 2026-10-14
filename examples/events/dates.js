// Dates as the program reads them: days from today, shown relative near today.

const MONTHS = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const WEEKDAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

/** Local midnight `offset` days from today. */
export function dayDate(offset) {
  const d = new Date();
  d.setHours(0, 0, 0, 0);
  d.setDate(d.getDate() + offset);
  return d;
}

/** `YYYY-MM-DD` of a local date. */
export function isoDay(date) {
  const pad = (n) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

/** Days from today to a local date: negative when it has passed. */
export function offsetOf(date) {
  return Math.round((startOfDay(date).getTime() - dayDate(0).getTime()) / 86400000);
}

function startOfDay(date) {
  const d = new Date(date.getTime());
  d.setHours(0, 0, 0, 0);
  return d;
}

/** `Oct 17`. */
export function shortDate(date) {
  return `${MONTHS[date.getMonth()].slice(0, 3)} ${date.getDate()}`;
}

/** `Saturday, October 17`. */
export function longDate(date) {
  return `${WEEKDAYS[date.getDay()]}, ${MONTHS[date.getMonth()]} ${date.getDate()}`;
}

/** A day as a list section reads it: `Today`, `Tomorrow`, `Sat, Oct 17`. */
export function relativeDay(offset) {
  if (offset === 0) return "Today";
  if (offset === 1) return "Tomorrow";
  const d = dayDate(offset);
  return `${WEEKDAYS[d.getDay()].slice(0, 3)}, ${shortDate(d)}`;
}

/** `19:30` → `7:30 PM`. */
export function formatTime(hhmm) {
  const [h, m] = hhmm.split(":").map(Number);
  return `${((h + 11) % 12) + 1}:${String(m).padStart(2, "0")} ${h < 12 ? "AM" : "PM"}`;
}

/** A start time and duration as a range: `7:30 – 10:00 PM`. */
export function timeRange(hhmm, minutes) {
  const [h, m] = hhmm.split(":").map(Number);
  const end = h * 60 + m + minutes;
  const endText = formatTime(`${Math.floor(end / 60) % 24}:${end % 60}`);
  const start = formatTime(hhmm);
  // Drop the first meridiem when both ends share it, as a printed program does.
  return start.slice(-2) === endText.slice(-2) ? `${start.slice(0, -3)} – ${endText}` : `${start} – ${endText}`;
}

/** Whole currency, or `Free`. */
export function formatPrice(amount) {
  return amount === 0 ? "Free" : `$${amount}`;
}
