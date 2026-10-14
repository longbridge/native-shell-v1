// How modification times read: relative near today, as a date further back.

const MONTHS = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const WEEKDAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const DAY = 86400_000;

function startOfDay(ms) {
  const d = new Date(ms);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

/** Whole days between `ms` and today: 0 today, 1 yesterday. */
function daysAgo(ms, now) {
  return Math.round((startOfDay(now) - startOfDay(ms)) / DAY);
}

export function clock(ms) {
  const d = new Date(ms);
  const h = d.getHours();
  return `${h % 12 === 0 ? 12 : h % 12}:${String(d.getMinutes()).padStart(2, "0")} ${h < 12 ? "AM" : "PM"}`;
}

/** The date in a list row: the time today, then "Yesterday", the weekday, a short date. */
export function rowDate(ms, now = Date.now()) {
  const days = daysAgo(ms, now);
  const d = new Date(ms);
  if (days <= 0) return clock(ms);
  if (days === 1) return "Yesterday";
  if (days < 7) return WEEKDAYS[d.getDay()];
  const short = `${MONTHS[d.getMonth()].slice(0, 3)} ${d.getDate()}`;
  return d.getFullYear() === new Date(now).getFullYear() ? short : `${short}, ${d.getFullYear()}`;
}

/** The editor's header: the full date and time of the last edit. */
export function longDate(ms) {
  const d = new Date(ms);
  return `${MONTHS[d.getMonth()]} ${d.getDate()}, ${d.getFullYear()} at ${clock(ms)}`;
}

/** The list section a note falls in, by when it was last edited. */
export function period(ms, now = Date.now()) {
  const days = daysAgo(ms, now);
  if (days <= 0) return "Today";
  if (days === 1) return "Yesterday";
  if (days < 7) return "Previous 7 Days";
  if (days < 30) return "Previous 30 Days";
  const d = new Date(ms);
  return d.getFullYear() === new Date(now).getFullYear() ? MONTHS[d.getMonth()] : String(d.getFullYear());
}
