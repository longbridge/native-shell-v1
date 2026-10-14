// The portfolio: transactions kept on the device, and what they add up to.
//
// Transactions are the only record; positions, cost, returns and every chart
// are derived from them and the market's prices, so adding or undoing one
// changes everything consistently. Costs use the average-cost method.

import { INSTRUMENTS, OLDEST_DAY, isoDate, shortDate, timeline, tradingDate, tradingDayOf } from "./market.js";

const KEY = "portfolio.transactions.v1";
const SETTINGS_KEY = "portfolio.settings.v1";

/** A first run starts with a believable history, bought at the simulated prices of the day. */
const SAMPLE = [
  ["WAVE", "buy", 60, 250],
  ["NOVA", "buy", 25, 240],
  ["ACME", "buy", 40, 200],
  ["VRDE", "buy", 120, 180],
  ["HRBR", "buy", 30, 150],
  ["WAVE", "buy", 20, 90],
  ["ACME", "buy", 15, 60],
  ["QUIL", "buy", 80, 45],
  ["NOVA", "sell", 5, 30],
];

/** @typedef {{ id: number, symbol: string, type: "buy" | "sell", shares: number, price: number, date: string }} Transaction */

/** @returns {Transaction[]} */
function sample(market) {
  return SAMPLE.map(([symbol, type, shares, day], i) => ({
    id: i + 1,
    symbol,
    type,
    shares,
    price: Math.round(market.close(symbol, day) * 100) / 100,
    date: isoDate(tradingDate(day)),
  }));
}

/** @param {any} t */
function isTransaction(t) {
  return (
    t &&
    Number.isInteger(t.id) &&
    INSTRUMENTS.some((i) => i.symbol === t.symbol) &&
    (t.type === "buy" || t.type === "sell") &&
    Number.isFinite(t.shares) &&
    t.shares > 0 &&
    Number.isFinite(t.price) &&
    t.price > 0 &&
    /^\d{4}-\d\d-\d\d$/.test(t.date)
  );
}

/** @returns {Transaction[]} */
export function loadTransactions(market) {
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) ?? "null");
    if (Array.isArray(saved)) return saved.filter(isTransaction);
  } catch {
    // A corrupt entry starts over with the sample history.
  }
  return sample(market);
}

export function saveTransactions(transactions) {
  try {
    localStorage.setItem(KEY, JSON.stringify(transactions));
    return true;
  } catch {
    return false;
  }
}

const SETTINGS = Object.freeze({
  moveAlerts: true,
  threshold: 3,
  muted: [],
  summary: true,
  summaryTime: "17:00",
  offline: false,
});

export function loadSettings() {
  let saved = {};
  try {
    saved = JSON.parse(localStorage.getItem(SETTINGS_KEY) ?? "{}") ?? {};
  } catch {
    saved = {};
  }
  const pick = (key, ok) => (key in saved && ok(saved[key]) ? saved[key] : SETTINGS[key]);
  const bool = (v) => typeof v === "boolean";
  return {
    moveAlerts: pick("moveAlerts", bool),
    threshold: pick("threshold", (v) => [2, 3, 5, 10].includes(v)),
    muted: pick("muted", (v) => Array.isArray(v) && v.every((s) => typeof s === "string")),
    summary: pick("summary", bool),
    summaryTime: pick("summaryTime", (v) => typeof v === "string" && /^\d\d:\d\d$/.test(v)),
    offline: pick("offline", bool),
  };
}

export function saveSettings(settings) {
  try {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
  } catch {
    // Settings still apply for this session.
  }
}

const dayCache = new Map();
/** `tradingDayOf`, remembered: charts ask for the same few dates hundreds of times. */
function dayOf(iso) {
  let day = dayCache.get(iso);
  if (day === undefined) {
    day = tradingDayOf(iso);
    dayCache.set(iso, day);
  }
  return day;
}

/** Oldest first, so running totals read in order. */
function chronological(transactions) {
  return [...transactions].sort((a, b) => (a.date === b.date ? a.id - b.id : a.date < b.date ? -1 : 1));
}

/** Shares and cost basis per symbol after the transactions up to and including `day` trading days ago. */
function holdingsAt(transactions, day = 0) {
  const held = new Map();
  for (const t of chronological(transactions)) {
    if (dayOf(t.date) < day) break;
    const h = held.get(t.symbol) ?? { shares: 0, cost: 0 };
    if (t.type === "buy") {
      h.shares += t.shares;
      h.cost += t.shares * t.price;
    } else if (h.shares > 0) {
      const sold = Math.min(t.shares, h.shares);
      h.cost -= (h.cost / h.shares) * sold;
      h.shares -= sold;
    }
    held.set(t.symbol, h);
  }
  return held;
}

/** Shares of `symbol` held on `date`, for validating a sale. */
export function sharesOn(transactions, symbol, date) {
  return holdingsAt(transactions, dayOf(date)).get(symbol)?.shares ?? 0;
}

/**
 * Open positions with today's numbers.
 * @returns {{ symbol: string, name: string, sector: string, shares: number, cost: number, price: number, value: number, dayChange: number, dayPercent: number, gain: number, gainPercent: number }[]}
 */
export function positions(transactions, market) {
  const out = [];
  for (const [symbol, h] of holdingsAt(transactions)) {
    if (h.shares <= 1e-9) continue;
    const inst = INSTRUMENTS.find((i) => i.symbol === symbol);
    const price = market.price(symbol);
    const prev = market.previousClose(symbol);
    out.push({
      symbol,
      name: inst.name,
      sector: inst.sector,
      shares: h.shares,
      cost: h.cost,
      price,
      value: h.shares * price,
      dayChange: h.shares * (price - prev),
      dayPercent: ((price - prev) / prev) * 100,
      gain: h.shares * price - h.cost,
      gainPercent: h.cost > 0 ? ((h.shares * price - h.cost) / h.cost) * 100 : 0,
    });
  }
  return out;
}

export function totals(list) {
  const value = list.reduce((s, p) => s + p.value, 0);
  const dayChange = list.reduce((s, p) => s + p.dayChange, 0);
  const cost = list.reduce((s, p) => s + p.cost, 0);
  const before = value - dayChange;
  return {
    value,
    dayChange,
    dayPercent: before > 0 ? (dayChange / before) * 100 : 0,
    gain: value - cost,
    gainPercent: cost > 0 ? ((value - cost) / cost) * 100 : 0,
  };
}

/**
 * The portfolio's value at every point of a range, as chart rows, holding
 * what was held at each point (a purchase shows up on its day).
 * @param {string[] | null} only Limits the sum to these symbols (a holding's own chart).
 */
export function series(transactions, market, range, only = null) {
  const points = timeline(range);
  const byDay = new Map();
  const heldOn = (day) => {
    if (!byDay.has(day)) byDay.set(day, holdingsAt(transactions, day));
    return byDay.get(day);
  };
  return points.map((p) => {
    let value = 0;
    for (const [symbol, h] of heldOn(p.day)) {
      if (only && !only.includes(symbol)) continue;
      value += h.shares * market.at(symbol, p);
    }
    return { label: p.label, value: Math.round(value * 100) / 100 };
  });
}

/** One instrument's price over a range, as chart rows. */
export function priceSeries(market, symbol, range) {
  return timeline(range).map((p) => ({
    label: p.label,
    value: Math.round(market.at(symbol, p) * 100) / 100,
  }));
}

/** The earliest date a transaction may carry: the simulated history's first day. */
export function earliestDate() {
  return isoDate(tradingDate(OLDEST_DAY));
}

// Formatting ----------------------------------------------------------------

const MINUS = "\u2212";

function grouped(n, decimals) {
  const [int, frac] = Math.abs(n).toFixed(decimals).split(".");
  const withCommas = int.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  return frac ? `${withCommas}.${frac}` : withCommas;
}

/** `$48,213.07`, with a true minus sign for losses. */
export function money(n) {
  return `${n < -0.004 ? MINUS : ""}$${grouped(n, 2)}`;
}

/** `+$412.80` / `−$96.10`: a change always carries its sign, so it never depends on color. */
export function signedMoney(n) {
  return `${n < -0.004 ? MINUS : "+"}$${grouped(n, 2)}`;
}

export function signedPercent(n) {
  return `${n < -0.004 ? MINUS : "+"}${Math.abs(n).toFixed(2)}%`;
}

export function percent(n) {
  return `${n.toFixed(1)}%`;
}

/** Shares as people write them: no trailing zeros, up to four decimals. */
export function shares(n) {
  return grouped(n, 4).replace(/\.?0+$/, "");
}

/** `Oct 3, 2026` from `2026-10-03`. */
export function longDate(iso) {
  const [y, m, d] = iso.split("-").map(Number);
  return shortDate(new Date(y, m - 1, d), true);
}
