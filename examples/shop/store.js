// The shopper's state: the cart, past orders and the demo switch, kept in
// localStorage so a cart survives closing the app. One store serves every
// screen; a screen that changes it calls `changed()`, which saves and redraws
// every view (the tab bar's badge, the product page's button, the cart).
//
// The network is simulated: loading the catalog and placing an order take a
// moment, and fail while "Simulate network errors" is on in Account, so the
// loading and error states can be seen and tested.

import { PRODUCTS, productById } from "./catalog.js";

const KEY = "shop.v1";
export const MAX_QUANTITY = 10;
export const FREE_DELIVERY = 5000;
export const TAX_RATE = 0.08;

/**
 * @typedef {{ product: string, color: string, size: string | null, quantity: number }} Line
 * @typedef {{ code: string, label: string }} Promo
 * @typedef {{ name: string, street: string, apartment: string, city: string, postal: string, phone: string }} Address
 * @typedef {{ id: string, placed: number, lines: Line[], totals: Totals, delivery: string, arrives: string, address: Address, status: string }} Order
 * @typedef {{ subtotal: number, discount: number, delivery: number, tax: number, total: number }} Totals
 */

export const ACCOUNT = { name: "Ada Lovelace", email: "ada@example.org" };

/** Codes the store knows; the shopper sees why one doesn't apply. */
const PROMOS = {
  WELCOME10: { label: "10% off your order", percent: 10 },
  SAVE15: { label: "$15 off orders over $100", amount: 1500, minimum: 10000 },
  SUMMER24: { label: "Summer sale", expired: "Sep 30" },
};

export const DELIVERY = [
  { id: "standard", label: "Standard", detail: "3–5 business days", price: 500, days: 5 },
  { id: "express", label: "Express", detail: "1–2 business days", price: 1200, days: 2 },
];

class Store {
  constructor() {
    /** @type {Line[]} */
    this.cart = [];
    /** @type {Promo | null} */
    this.promo = null;
    /** @type {Order[]} */
    this.orders = [];
    this.failNetwork = false;
    this.saveFailed = false;
    this.load();
  }

  load() {
    let saved = null;
    try {
      saved = JSON.parse(localStorage.getItem(KEY) ?? "null");
    } catch {
      saved = null;
    }
    if (saved === null || typeof saved !== "object") {
      this.orders = sampleOrders();
      return;
    }
    this.cart = Array.isArray(saved.cart) ? saved.cart.filter(validLine) : [];
    this.promo = saved.promo && PROMOS[saved.promo.code] ? saved.promo : null;
    this.orders = Array.isArray(saved.orders) ? saved.orders.filter((o) => o && typeof o.id === "string" && Array.isArray(o.lines)) : [];
    this.failNetwork = saved.failNetwork === true;
  }

  /** Saves, then redraws every view. A full or unavailable store is reported on the cart, not thrown. */
  changed() {
    try {
      const { cart, promo, orders, failNetwork } = this;
      localStorage.setItem(KEY, JSON.stringify({ cart, promo, orders, failNetwork }));
      this.saveFailed = false;
    } catch {
      this.saveFailed = true;
    }
    window.refresh();
  }

  get count() {
    return this.cart.reduce((n, l) => n + l.quantity, 0);
  }

  /** How many of exactly this variant are in the cart. */
  quantityOf(product, color, size) {
    return this.cart.find((l) => sameVariant(l, { product, color, size }))?.quantity ?? 0;
  }

  /** Adds to the cart (merging with the same variant), and answers how to take it back. */
  add(line) {
    const before = this.cart;
    const existing = this.cart.find((l) => sameVariant(l, line));
    this.cart = existing
      ? this.cart.map((l) => (l === existing ? { ...l, quantity: Math.min(MAX_QUANTITY, l.quantity + line.quantity) } : l))
      : [...this.cart, line];
    this.changed();
    return () => {
      this.cart = before;
      this.changed();
    };
  }

  setQuantity(index, quantity) {
    this.cart = this.cart.map((l, i) => (i === index ? { ...l, quantity: Math.max(1, Math.min(MAX_QUANTITY, quantity)) } : l));
    this.changed();
  }

  /** Removes one line, and answers how to put it back where it was. */
  remove(index) {
    const line = this.cart[index];
    this.cart = this.cart.filter((_, i) => i !== index);
    this.changed();
    return () => {
      this.cart = [...this.cart.slice(0, index), line, ...this.cart.slice(index)];
      this.changed();
    };
  }

  /** Applies a promo code, or answers why it can't be applied. */
  applyPromo(input) {
    const code = input.trim().toUpperCase();
    const promo = PROMOS[code];
    if (!promo) return `“${code}” isn’t a valid code. Check the spelling and try again.`;
    if (promo.expired) return `${code} expired on ${promo.expired}.`;
    if (promo.minimum && this.subtotal() < promo.minimum) {
      return `${code} needs an order of ${money(promo.minimum)} or more.`;
    }
    this.promo = { code, label: promo.label };
    this.changed();
    return null;
  }

  removePromo() {
    this.promo = null;
    this.changed();
  }

  subtotal(lines = this.cart) {
    return lines.reduce((sum, l) => sum + (productById(l.product)?.price ?? 0) * l.quantity, 0);
  }

  /** The discount the applied code gives on `subtotal`; a code whose minimum is no longer met gives nothing. */
  discount(subtotal = this.subtotal()) {
    const promo = this.promo && PROMOS[this.promo.code];
    if (!promo || (promo.minimum && subtotal < promo.minimum)) return 0;
    if (promo.percent) return Math.round((subtotal * promo.percent) / 100);
    return Math.min(subtotal, promo.amount ?? 0);
  }

  /** @returns {Totals} */
  totals(deliveryId = null) {
    const subtotal = this.subtotal();
    const discount = this.discount(subtotal);
    const option = DELIVERY.find((d) => d.id === deliveryId);
    const delivery = option ? deliveryPrice(option, subtotal - discount) : 0;
    const tax = Math.round((subtotal - discount) * TAX_RATE);
    return { subtotal, discount, delivery, tax, total: subtotal - discount + delivery + tax };
  }

  /**
   * Places the order: a request that takes a moment and fails while network
   * errors are simulated. On success the cart empties and the order leads the
   * history.
   * @param {import("native-shell").AsyncContext} cx
   * @param {{ address: Address, delivery: string }} details
   * @returns {Promise<Order>}
   */
  async placeOrder(cx, { address, delivery }) {
    await cx.sleep(1200);
    if (this.failNetwork) throw new Error("The connection timed out.");
    const option = DELIVERY.find((d) => d.id === delivery) ?? DELIVERY[0];
    const order = {
      id: `LS-${Math.max(10480, ...this.orders.map((o) => Number(o.id.slice(3)) || 0)) + 1}`,
      placed: Date.now(),
      lines: this.cart,
      totals: this.totals(option.id),
      delivery: option.label,
      arrives: arrivalDate(option.days),
      address,
      status: "Processing",
    };
    this.orders = [order, ...this.orders];
    this.cart = [];
    this.promo = null;
    this.changed();
    return order;
  }

  /** Stands in for fetching the catalog: a moment's wait, then the products or an error. */
  async fetchCatalog(cx) {
    await cx.sleep(700);
    if (this.failNetwork) throw new Error("The connection timed out.");
    return PRODUCTS;
  }
}

/** Delivery is free on standard shipping once the order reaches the threshold. */
export function deliveryPrice(option, amount) {
  return option.id === "standard" && amount >= FREE_DELIVERY ? 0 : option.price;
}

function sameVariant(a, b) {
  return a.product === b.product && a.color === b.color && (a.size ?? null) === (b.size ?? null);
}

function validLine(l) {
  const p = l && productById(l.product);
  return (
    p !== undefined &&
    p.colors.some((c) => c.id === l.color) &&
    (l.size === null || p.sizes?.some((s) => s.id === l.size)) &&
    Number.isInteger(l.quantity) &&
    l.quantity >= 1 &&
    l.quantity <= MAX_QUANTITY
  );
}

/** Cents as dollars: `$1,234.50`. */
export function money(cents) {
  const sign = cents < 0 ? "−" : "";
  const abs = Math.abs(cents);
  const dollars = Math.floor(abs / 100).toString().replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  return `${sign}$${dollars}.${String(abs % 100).padStart(2, "0")}`;
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/** `Thu, Oct 17` for a timestamp. */
export function shortDate(ms) {
  const d = new Date(ms);
  return `${WEEKDAYS[d.getDay()]}, ${MONTHS[d.getMonth()]} ${d.getDate()}`;
}

/** The day an order placed now arrives, counting business days. */
export function arrivalDate(businessDays) {
  const d = new Date();
  let left = businessDays;
  while (left > 0) {
    d.setDate(d.getDate() + 1);
    if (d.getDay() !== 0 && d.getDay() !== 6) left--;
  }
  return shortDate(d.getTime());
}

/** A first run shows two past orders, so the history explains itself. */
function sampleOrders() {
  const day = 86400000;
  const address = { name: ACCOUNT.name, street: "12 Harbour Street", apartment: "", city: "Springfield", postal: "12345", phone: "" };
  const order = (id, daysAgo, lines, status) => {
    const subtotal = lines.reduce((sum, l) => sum + (productById(l.product)?.price ?? 0) * l.quantity, 0);
    const delivery = subtotal >= FREE_DELIVERY ? 0 : 500;
    const tax = Math.round(subtotal * TAX_RATE);
    return {
      id,
      placed: Date.now() - daysAgo * day,
      lines,
      totals: { subtotal, discount: 0, delivery, tax, total: subtotal + delivery + tax },
      delivery: "Standard",
      arrives: shortDate(Date.now() - (daysAgo - 4) * day),
      address,
      status,
    };
  };
  return [
    order("LS-10480", 9, [{ product: "kettle", color: "black", size: null, quantity: 1 }, { product: "mug", color: "sand", size: null, quantity: 2 }], "Delivered"),
    order("LS-10477", 41, [{ product: "notebook", color: "forest", size: "a5", quantity: 3 }], "Delivered"),
  ];
}

// Last, so the constants above are initialized when the first load reads them.
export const store = new Store();
