// The instruments a user can follow, and the list they chose, kept on the
// device. Names and prices are fictional; the host app's `demo/quote` module
// supplies the numbers.

export const CATALOG = [
  { symbol: "HALO.US", name: "Halo Systems" },
  { symbol: "MAPL.US", name: "Maple Foods" },
  { symbol: "KITE.US", name: "Kite Logistics" },
  { symbol: "ORCA.US", name: "Orca Robotics" },
  { symbol: "FERN.US", name: "Fern Renewables" },
  { symbol: "PINE.HK", name: "Pine Harbour Holdings" },
  { symbol: "TIDE.HK", name: "Tide Power" },
  { symbol: "LUMA.US", name: "Luma Optics" },
];

const DEFAULT = ["HALO.US", "MAPL.US", "KITE.US", "ORCA.US", "FERN.US", "PINE.HK"];
const KEY = "watchlist.symbols";

export function nameOf(symbol) {
  return CATALOG.find((c) => c.symbol === symbol)?.name ?? symbol;
}

export function loadSymbols() {
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) ?? "null");
    if (Array.isArray(saved)) return saved.filter((s) => CATALOG.some((c) => c.symbol === s));
  } catch {
    // A corrupt entry falls back to the default list.
  }
  return [...DEFAULT];
}

export function saveSymbols(symbols) {
  try {
    localStorage.setItem(KEY, JSON.stringify(symbols));
  } catch {
    // The list still works for this session.
  }
}

export const SORTS = ["Symbol", "Change", "Price"];

/** Orders symbols by the chosen column; unknown quotes sort last. */
export function sorted(symbols, quotes, sort) {
  const value = (s) => {
    const q = quotes.get(s);
    if (sort === 1) return q?.change_percent ?? -Infinity;
    if (sort === 2) return q?.last ?? -Infinity;
    return 0;
  };
  if (sort === 0) return [...symbols].sort((a, b) => a.localeCompare(b));
  return [...symbols].sort((a, b) => value(b) - value(a));
}

export function formatPrice(n) {
  return n.toFixed(2);
}

export function formatPercent(n) {
  return `${n >= 0 ? "+" : "−"}${Math.abs(n).toFixed(2)}%`;
}
