// A stand-in for the city's box office API: the same calls an app would make,
// answered from the bundled program after a network-like delay.
//
// Real networks fail, and the screens that handle failure matter as much as
// the happy path, so the stand-in fails on a short script that makes every
// state reachable in one session: the first refresh of the program finds no
// connection, and the first payment times out. Every later call succeeds.

import { EVENTS } from "./catalog.js";
/** @import { AsyncContext } from "native-shell" */
/** @import { Event } from "./catalog.js" */

const SCRIPT = { refresh: ["offline"], pay: ["timeout"] };

export class ServiceError extends Error {
  /** @param {"offline" | "timeout"} kind @param {string} message */
  constructor(kind, message) {
    super(message);
    this.kind = kind;
  }
}

/** The next scripted outcome of a call: a failure, or nothing for success. */
function outcome(call) {
  const next = SCRIPT[call].shift();
  if (next === "offline") throw new ServiceError("offline", "You’re offline.");
  if (next === "timeout") throw new ServiceError("timeout", "The box office didn’t answer in time.");
}

/**
 * The program of upcoming events. `refresh` marks a pull to refresh rather than the first load.
 * @param {AsyncContext} cx
 * @returns {Promise<{ events: Event[], at: number }>}
 */
export async function fetchProgram(cx, { refresh = false } = {}) {
  await cx.sleep(refresh ? 700 : 1000);
  if (refresh) outcome("refresh");
  return { events: EVENTS, at: Date.now() };
}

/**
 * Charges the order and issues its tickets.
 * @param {AsyncContext} cx
 * @param {{ event: Event, tierId: string, quantity: number }} order
 */
export async function pay(cx, order) {
  await cx.sleep(1200);
  outcome("pay");
  const tier = order.event.tiers.find((t) => t.id === order.tierId);
  return { total: (tier?.price ?? 0) * order.quantity };
}
