// The simulated market: fictional instruments and their prices.
//
// Nothing is fetched. Each instrument's history is a random walk seeded by its
// symbol, counted in trading days back from today (0 is today, 1 yesterday),
// so every device and every test run draws the same charts and totals; only
// the date labels follow the calendar. Today's session is frozen at the same
// moment, and live updates continue from there one deterministic step at a
// time.

export const INSTRUMENTS = [
  {
    symbol: "ACME",
    name: "Acme Industrial",
    sector: "Industrials",
    close: 142.18,
    drift: 0.14,
    vol: 0.015,
  },
  {
    symbol: "NOVA",
    name: "Nova Semiconductors",
    sector: "Technology",
    close: 318.42,
    drift: 0.38,
    vol: 0.026,
  },
  {
    symbol: "WAVE",
    name: "Wave Total Market Fund",
    sector: "Funds",
    close: 112.6,
    drift: 0.09,
    vol: 0.009,
  },
  {
    symbol: "VRDE",
    name: "Verde Energy",
    sector: "Energy",
    close: 38.74,
    drift: -0.06,
    vol: 0.022,
  },
  {
    symbol: "HRBR",
    name: "Harbor Health",
    sector: "Health care",
    close: 96.05,
    drift: 0.07,
    vol: 0.013,
  },
  {
    symbol: "QUIL",
    name: "Quill Media",
    sector: "Communication",
    close: 24.31,
    drift: 0.21,
    vol: 0.031,
  },
  {
    symbol: "ORBT",
    name: "Orbit Aerospace",
    sector: "Industrials",
    close: 211.9,
    drift: 0.18,
    vol: 0.021,
  },
  {
    symbol: "GRAN",
    name: "Granary Foods",
    sector: "Consumer staples",
    close: 57.33,
    drift: 0.04,
    vol: 0.01,
  },
];

export const RANGES = ["1D", "1W", "1M", "1Y"];

/** Trading days of history: a year and a little more. */
const DAYS = 270;
/** Five-minute slots from 9:30 to 16:00. */
const SLOTS = 79;
/** Where today's simulated session stands: 1:40 PM. */
const NOW_SLOT = 50;
/** Live updates move by a fraction of a five-minute step. */
const TICK_VOL = 0.0007;

export function instrument(symbol) {
  return INSTRUMENTS.find((i) => i.symbol === symbol);
}

/** A small, fast, seedable generator (mulberry32), so the walks repeat exactly. */
function generator(seed) {
  let a = seed >>> 0;
  const uniform = () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
  // Box–Muller: normal steps make walks that look like prices.
  return () => Math.sqrt(-2 * Math.log(uniform() || 1e-12)) * Math.cos(2 * Math.PI * uniform());
}

function hash(text) {
  let h = 2166136261;
  for (const c of text) h = Math.imul(h ^ c.charCodeAt(0), 16777619);
  return h >>> 0;
}

/** Closes from today (index 0) back `DAYS` trading days, ending at the instrument's previous close. */
function dailyCloses(inst) {
  const normal = generator(hash(inst.symbol));
  const closes = new Array(DAYS + 1);
  // Index 1 is yesterday's close, the anchor; today's close is one step on.
  closes[1] = inst.close;
  closes[0] = inst.close * Math.exp(inst.drift / 252 + inst.vol * normal());
  for (let d = 2; d <= DAYS; d++) {
    closes[d] = closes[d - 1] / Math.exp(inst.drift / 252 + inst.vol * normal());
  }
  return closes;
}

/** One day's five-minute prices: a bridge from near the previous close to the day's close. */
function session(inst, closes, day) {
  const normal = generator(hash(`${inst.symbol}/${day}`));
  const from = closes[day + 1] * (1 + inst.vol * 0.2 * normal());
  const to = closes[day];
  const step = (inst.vol / Math.sqrt(SLOTS)) * 0.9;
  const walk = [0];
  for (let i = 1; i < SLOTS; i++) walk.push(walk[i - 1] + step * normal());
  const last = walk[SLOTS - 1];
  // Pinning the walk's end to the close keeps sessions and daily closes in agreement.
  return walk.map((w, i) => from * Math.pow(to / from, i / (SLOTS - 1)) * Math.exp(w - (last * i) / (SLOTS - 1)));
}

/** The trading date `day` trading days before today (weekends skipped). */
export function tradingDate(day, today = new Date()) {
  const d = new Date(today.getFullYear(), today.getMonth(), today.getDate());
  // On a weekend, "today's" session is Friday's.
  while (d.getDay() === 0 || d.getDay() === 6) d.setDate(d.getDate() - 1);
  for (let left = day; left > 0;) {
    d.setDate(d.getDate() - 1);
    if (d.getDay() !== 0 && d.getDay() !== 6) left -= 1;
  }
  return d;
}

/** How many trading days before today an ISO date (`YYYY-MM-DD`) falls; weekends count as the Friday before. */
export function tradingDayOf(iso, today = new Date()) {
  const [y, m, dd] = iso.split("-").map(Number);
  const target = new Date(y, m - 1, dd);
  while (target.getDay() === 0 || target.getDay() === 6) target.setDate(target.getDate() - 1);
  const d = tradingDate(0, today);
  let day = 0;
  while (day < DAYS && d > target) {
    d.setDate(d.getDate() - 1);
    if (d.getDay() !== 0 && d.getDay() !== 6) day += 1;
  }
  return day;
}

export function isoDate(date) {
  const p = (n) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${p(date.getMonth() + 1)}-${p(date.getDate())}`;
}

export const OLDEST_DAY = DAYS - 1;

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

export function slotTime(slot) {
  const minutes = 9 * 60 + 30 + slot * 5;
  const h = Math.floor(minutes / 60);
  return `${((h + 11) % 12) + 1}:${String(minutes % 60).padStart(2, "0")} ${h < 12 ? "AM" : "PM"}`;
}

export function shortDate(date, withYear = false) {
  return `${MONTHS[date.getMonth()]} ${date.getDate()}${withYear ? `, ${date.getFullYear()}` : ""}`;
}

/**
 * The points a range draws: `{ label, day, slot }`, where `slot` is a
 * five-minute slot of that day's session or `null` for its close. The last
 * point is always "now".
 */
export function timeline(range, today = new Date()) {
  const points = [];
  const intraday = (day, every) => {
    const date = tradingDate(day, today);
    const end = day === 0 ? NOW_SLOT : SLOTS - 1;
    for (let slot = 0; slot <= end; slot += every) {
      points.push({
        label: day === 0 && every === 1 ? slotTime(slot) : `${WEEKDAYS[date.getDay()]} ${slotTime(slot)}`,
        day,
        slot,
      });
    }
  };
  if (range === "1D") {
    // The day is measured from the previous close, as the day's change is.
    points.push({ label: "Previous close", day: 1, slot: null });
    intraday(0, 1);
  }
  if (range === "1W") for (let day = 4; day >= 0; day--) intraday(day, 6);
  if (range === "1M" || range === "1Y") {
    const [span, every] = range === "1M" ? [21, 1] : [255, 5];
    for (let day = span; day >= 1; day -= every)
      points.push({
        label: shortDate(tradingDate(day, today), range === "1Y"),
        day,
        slot: null,
      });
  }
  const last = points[points.length - 1];
  if (!last || last.day !== 0 || last.slot !== NOW_SLOT) points.push({ label: slotTime(NOW_SLOT), day: 0, slot: NOW_SLOT });
  return points;
}

/**
 * Prices for every instrument: history on demand, and a live price that
 * `tick()` moves one step at a time.
 */
export class Market {
  constructor() {
    this.closes = new Map(INSTRUMENTS.map((i) => [i.symbol, dailyCloses(i)]));
    this.sessions = new Map();
    this.live = new Map(INSTRUMENTS.map((i) => [i.symbol, this.sessionOf(i.symbol, 0)[NOW_SLOT]]));
    this.ticks = 0;
    this.normal = generator(hash("live"));
  }

  sessionOf(symbol, day) {
    const key = `${symbol}/${day}`;
    let prices = this.sessions.get(key);
    if (!prices) {
      prices = session(instrument(symbol), this.closes.get(symbol), day);
      this.sessions.set(key, prices);
    }
    return prices;
  }

  /** The latest price. */
  price(symbol) {
    return this.live.get(symbol);
  }

  previousClose(symbol) {
    return this.closes.get(symbol)[1];
  }

  /** The close `day` trading days back; today's is the live price. */
  close(symbol, day) {
    return day === 0 ? this.price(symbol) : this.closes.get(symbol)[Math.min(day, DAYS)];
  }

  /** The price at one timeline point. */
  at(symbol, point) {
    if (point.day === 0 && point.slot === NOW_SLOT) return this.price(symbol);
    return point.slot === null ? this.close(symbol, point.day) : this.sessionOf(symbol, point.day)[point.slot];
  }

  /** Today's low and high so far. */
  dayRange(symbol) {
    const today = [...this.sessionOf(symbol, 0).slice(0, NOW_SLOT), this.price(symbol)];
    return [Math.min(...today), Math.max(...today)];
  }

  /** The lowest and highest close over the last 52 weeks. */
  yearRange(symbol) {
    const year = [...this.closes.get(symbol).slice(1, 253), this.price(symbol)];
    return [Math.min(...year), Math.max(...year)];
  }

  /** One live update: every price moves a small, repeatable step. */
  tick() {
    this.ticks += 1;
    for (const inst of INSTRUMENTS) {
      this.live.set(inst.symbol, this.live.get(inst.symbol) * Math.exp(TICK_VOL * (inst.vol / 0.015) * this.normal()));
    }
  }
}
