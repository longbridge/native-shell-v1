// Preferences in localStorage. Unknown or corrupt values fall back to the
// defaults one key at a time, so one bad entry does not reset everything.

const KEY = "settings.v1";

export const DEFAULTS = Object.freeze({
  account: { name: "Ada Lovelace", email: "ada@example.org" },
  notifications: true,
  summary: true,
  summaryTime: "08:30",
  theme: "system",
  usageData: false,
});

export function load() {
  let saved = {};
  try {
    saved = JSON.parse(localStorage.getItem(KEY) ?? "{}") ?? {};
  } catch {
    saved = {};
  }
  const pick = (key, ok) => (key in saved && ok(saved[key]) ? saved[key] : DEFAULTS[key]);
  const isAccount = (a) => a === null || (a && typeof a.name === "string" && typeof a.email === "string");
  return {
    account: pick("account", isAccount),
    notifications: pick("notifications", (v) => typeof v === "boolean"),
    summary: pick("summary", (v) => typeof v === "boolean"),
    summaryTime: pick("summaryTime", (v) => typeof v === "string" && /^\d\d:\d\d$/.test(v)),
    theme: pick("theme", (v) => ["system", "light", "dark"].includes(v)),
    usageData: pick("usageData", (v) => typeof v === "boolean"),
  };
}

export function save(settings) {
  try {
    localStorage.setItem(KEY, JSON.stringify(settings));
    return true;
  } catch {
    return false;
  }
}

/** `08:30` → `8:30 AM`. */
export function formatTime(hhmm) {
  const [h, m] = hhmm.split(":").map(Number);
  return `${((h + 11) % 12) + 1}:${String(m).padStart(2, "0")} ${h < 12 ? "AM" : "PM"}`;
}

/** Bytes as people read them on a storage screen. */
export function formatBytes(n) {
  if (n === 0) return "Zero KB";
  if (n < 1e6) return `${Math.max(1, Math.round(n / 1e3))} KB`;
  return `${(n / 1e6).toFixed(n < 1e7 ? 1 : 0)} MB`;
}
