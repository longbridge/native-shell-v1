// What the user has: booked tickets and events added to their calendar, kept
// on the device. Seats left are the catalog's minus what this device booked,
// so a booking shows up in the counts at once.
//
// The pages share one store; a change redraws every page (`window.refresh()`),
// since a booking made on a pushed page changes the tabs beneath it.

import { eventById } from "./catalog.js";
/** @import { Event, Tier } from "./catalog.js" */

const TICKETS = "events.tickets.v1";
const SAVED = "events.saved.v1";

/**
 * @typedef {{
 *   id: number, code: string, eventId: string, tierId: string, quantity: number, total: number,
 *   attendee: { name: string, email: string, phone: string },
 * }} Ticket
 */

/** @type {Ticket[]} */
let tickets = read(TICKETS).filter((t) => t && typeof t.id === "number" && eventById(t.eventId));
/** @type {string[]} */
let saved = read(SAVED).filter((id) => typeof id === "string" && eventById(id));
let saveFailed = false;

function read(key) {
  try {
    const value = JSON.parse(localStorage.getItem(key) ?? "[]");
    return Array.isArray(value) ? value : [];
  } catch {
    return [];
  }
}

function write() {
  try {
    localStorage.setItem(TICKETS, JSON.stringify(tickets));
    localStorage.setItem(SAVED, JSON.stringify(saved));
    saveFailed = false;
  } catch {
    saveFailed = true;
  }
  window.refresh();
}

/** Whether the last change could not be written; it is kept in memory until the app closes. */
export function failedToSave() {
  return saveFailed;
}

/** Tickets in date order, soonest first. */
export function allTickets() {
  return [...tickets].sort((a, b) => eventById(a.eventId).day - eventById(b.eventId).day || a.id - b.id);
}

/** @param {number} id */
export function ticketById(id) {
  return tickets.find((t) => t.id === id) ?? null;
}

/** @param {Event} event */
export function ticketsFor(event) {
  return tickets.filter((t) => t.eventId === event.id);
}

/** Seats still on sale in a tier. @param {Event} event @param {Tier} tier */
export function left(event, tier) {
  const booked = tickets.filter((t) => t.eventId === event.id && t.tierId === tier.id).reduce((n, t) => n + t.quantity, 0);
  return Math.max(0, tier.left - booked);
}

/** Seats left across an event's tiers. @param {Event} event */
export function seatsLeft(event) {
  return event.tiers.reduce((n, tier) => n + left(event, tier), 0);
}

/** The lowest price still on sale, or null when sold out. @param {Event} event */
export function fromPrice(event) {
  const open = event.tiers.filter((tier) => left(event, tier) > 0);
  return open.length === 0 ? null : Math.min(...open.map((tier) => tier.price));
}

/**
 * Records a paid booking and answers its ticket.
 * @param {Omit<Ticket, "id" | "code">} order
 */
export function addTicket(order) {
  const id = tickets.reduce((max, t) => Math.max(max, t.id), 0) + 1;
  const ticket = { ...order, id, code: ticketCode(order.eventId, id) };
  tickets = [...tickets, ticket];
  write();
  return ticket;
}

/** Removes a ticket; answers a function that puts it back, for Undo. @param {Ticket} ticket */
export function cancelTicket(ticket) {
  tickets = tickets.filter((t) => t.id !== ticket.id);
  write();
  return () => {
    if (!ticketById(ticket.id)) tickets = [...tickets, ticket];
    write();
  };
}

/** @param {Event} event */
export function isSaved(event) {
  return saved.includes(event.id);
}

/** Adds an event to the user's calendar, or takes it off. @param {Event} event */
export function setSaved(event, on) {
  saved = on ? [...saved.filter((id) => id !== event.id), event.id] : saved.filter((id) => id !== event.id);
  write();
}

/** A short code the door staff can read out: `HV-` and six letters and digits from the booking. */
function ticketCode(eventId, id) {
  const alphabet = "ACDEFGHJKLMNPQRTUVWXY34679";
  let h = 2166136261;
  for (const ch of `${eventId}:${id}`) h = Math.imul(h ^ ch.charCodeAt(0), 16777619) >>> 0;
  let code = "";
  for (let i = 0; i < 6; i++) {
    code += alphabet[h % alphabet.length];
    h = (Math.floor(h / alphabet.length) ^ Math.imul(h, 2654435761)) >>> 0;
  }
  return `HV-${code}`;
}
