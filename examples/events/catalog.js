// The city's events: venues, categories and the program for the coming weeks.
//
// Dates are days from today, so the program is always current; times are the
// venue's local time. Everything here is invented.

/** @typedef {{ id: string, name: string, detail: string, price: number, left: number }} Tier */
/** @typedef {{ id: string, name: string, address: string, district: string }} Venue */
/**
 * @typedef {{
 *   id: string, title: string, category: string, venue: Venue, day: number, time: string, minutes: number,
 *   summary: string, about: string, tiers: Tier[], featured?: boolean,
 * }} Event
 */

export const CITY = "Harborview";

/** @import { ThemeColor } from "native-shell" */

/**
 * Categories in the order the filter shows them. `colors` paint the event's poster.
 * @type {{ id: string, label: string, glyph: string, colors: [ThemeColor, ThemeColor] }[]}
 */
export const CATEGORIES = [
  { id: "music", label: "Music", glyph: "music", colors: ["primary", "destructive"] },
  { id: "theater", label: "Theater", glyph: "mask", colors: ["destructive", "warning"] },
  { id: "food", label: "Food & Drink", glyph: "cup", colors: ["warning", "success"] },
  { id: "art", label: "Art", glyph: "palette", colors: ["info", "success"] },
  { id: "talks", label: "Talks", glyph: "mic", colors: ["success", "primary"] },
  { id: "family", label: "Family", glyph: "sun", colors: ["warning", "destructive"] },
];

/** @param {string} id */
export function category(id) {
  return CATEGORIES.find((c) => c.id === id) ?? CATEGORIES[0];
}

/** @type {Record<string, Venue>} */
const VENUES = {
  lantern: { id: "lantern", name: "The Lantern Room", address: "14 Cooper Lane", district: "Old Town" },
  amphi: { id: "amphi", name: "Riverside Amphitheater", address: "2 Embankment Walk", district: "Riverside" },
  mill: { id: "mill", name: "Old Mill Market Hall", address: "88 Millrace Road", district: "Mill District" },
  northgate: { id: "northgate", name: "Northgate Gallery", address: "31 Northgate Street", district: "Northgate" },
  pier: { id: "pier", name: "Pier 9 Warehouse", address: "Pier 9, Harbor Road", district: "Docklands" },
  playhouse: { id: "playhouse", name: "Elm Street Playhouse", address: "120 Elm Street", district: "Old Town" },
  glasshouse: { id: "glasshouse", name: "Botanic Glasshouse", address: "Botanic Gardens, Park Gate", district: "Parkside" },
  library: { id: "library", name: "Central Library Auditorium", address: "1 Library Square", district: "Civic Center" },
};

const ga = (price, left) => ({ id: "ga", name: "General admission", detail: "Standing, first come first served", price, left });
const free = (left) => ({ id: "free", name: "Free entry", detail: "Reserve a spot so the venue can plan", price: 0, left });

/** @type {Event[]} */
export const EVENTS = [
  {
    id: "night-market",
    title: "Night Market on the Pier",
    category: "food",
    venue: VENUES.pier,
    day: 0,
    time: "18:00",
    minutes: 240,
    summary: "Forty food stalls, a natural wine bar and a brass band.",
    about:
      "The pier fills with forty stalls from the city’s best kitchens, from hand-pulled noodles to wood-fired flatbreads. A natural wine bar runs until close and the Docklands Brass Band plays two sets. Entry includes a tasting token.",
    tiers: [ga(12, 140), { id: "tasting", name: "Tasting pass", detail: "Entry and six tasting tokens", price: 35, left: 6 }],
    featured: true,
  },
  {
    id: "quiet-hours",
    title: "Quiet Hours: Late Jazz",
    category: "music",
    venue: VENUES.lantern,
    day: 0,
    time: "21:30",
    minutes: 120,
    summary: "A piano trio in a forty-seat room, lights low.",
    about:
      "The Mara Ellis Trio plays standards and originals in the Lantern Room’s forty seats. Phones stay in pockets; the bar serves between sets.",
    tiers: [{ id: "seat", name: "Seat", detail: "Unreserved table seating", price: 24, left: 4 }],
  },
  {
    id: "harbor-lights",
    title: "Harbor Lights Orchestra",
    category: "music",
    venue: VENUES.amphi,
    day: 2,
    time: "19:30",
    minutes: 150,
    summary: "Film scores under the open sky, with fireworks at the finale.",
    about:
      "The Harbor Lights Orchestra plays a program of film scores on the river stage, ending with fireworks over the water. Bring a blanket for the lawn, or reserve a seat in the bowl. The concert goes ahead in light rain.",
    tiers: [
      { id: "lawn", name: "Lawn", detail: "Bring a blanket; no chairs over 30 cm", price: 28, left: 320 },
      { id: "reserved", name: "Reserved seat", detail: "Numbered seats in the bowl", price: 54, left: 38 },
      { id: "box", name: "Riverside box", detail: "Four seats, table and a hamper", price: 120, left: 2 },
    ],
    featured: true,
  },
  {
    id: "glass-garden",
    title: "Glass Garden After Dark",
    category: "art",
    venue: VENUES.glasshouse,
    day: 3,
    time: "20:00",
    minutes: 180,
    summary: "A light installation through the palm house and the fern grotto.",
    about:
      "Twelve artists light the Botanic Glasshouse after closing. The walk takes about an hour and runs through the palm house, the fern grotto and the water lily pond. Entry is timed every fifteen minutes.",
    tiers: [ga(18, 64), { id: "member", name: "Garden member", detail: "Show your membership card at the gate", price: 12, left: 30 }],
    featured: true,
  },
  {
    id: "puppet-parade",
    title: "Saturday Puppet Parade",
    category: "family",
    venue: VENUES.mill,
    day: 3,
    time: "10:30",
    minutes: 90,
    summary: "A giant puppet workshop, then a parade through the market.",
    about:
      "Children build puppets with the Paper Moon company, then parade them through the market hall at noon. Materials are included. Suitable for ages 4 to 11; adults come free with a child.",
    tiers: [{ id: "child", name: "Child", detail: "Includes materials; one adult free", price: 8, left: 22 }],
  },
  {
    id: "cities-water",
    title: "Cities and Water",
    category: "talks",
    venue: VENUES.library,
    day: 5,
    time: "18:30",
    minutes: 75,
    summary: "How harbor towns are planning for higher tides.",
    about:
      "An urban planner, a hydrologist and the city’s harbor master on how Harborview is preparing for higher tides, followed by questions from the audience. Captions are shown on screen.",
    tiers: [free(110)],
  },
  {
    id: "the-understudy",
    title: "The Understudy",
    category: "theater",
    venue: VENUES.playhouse,
    day: 6,
    time: "19:00",
    minutes: 140,
    summary: "A backstage comedy about the night everything goes wrong.",
    about:
      "When the lead walks out ten minutes before curtain, the understudy has one evening to save the show. The Elm Street company’s new comedy, with one interval. Recommended for ages 12 and up.",
    tiers: [
      { id: "stalls", name: "Stalls", detail: "Rows A–M", price: 42, left: 0 },
      { id: "circle", name: "Circle", detail: "Rows A–F, raked", price: 34, left: 0 },
    ],
  },
  {
    id: "print-fair",
    title: "Risograph Print Fair",
    category: "art",
    venue: VENUES.northgate,
    day: 7,
    time: "11:00",
    minutes: 360,
    summary: "Thirty small presses, zines and live printing.",
    about:
      "Thirty independent presses sell zines, posters and books. A two-drum risograph runs all day: bring a drawing and print it in the afternoon workshop.",
    tiers: [free(400)],
  },
  {
    id: "river-run",
    title: "Riverside Sunset Run",
    category: "family",
    venue: VENUES.amphi,
    day: 9,
    time: "18:00",
    minutes: 90,
    summary: "A flat 5 km loop along the embankment, walkers welcome.",
    about:
      "A relaxed 5 km loop along both banks of the river, starting and finishing at the amphitheater. Strollers and walkers are welcome; every finisher gets a lemonade.",
    tiers: [ga(10, 240)],
  },
  {
    id: "dumpling-class",
    title: "Dumpling Making Class",
    category: "food",
    venue: VENUES.mill,
    day: 11,
    time: "17:00",
    minutes: 120,
    summary: "Fold three kinds of dumplings, then eat them.",
    about:
      "Chef Lin Okafor teaches three folds: crescent, pleated purse and wonton. You eat what you make, with a pot of tea. Vegetarian fillings available.",
    tiers: [{ id: "place", name: "Place at the table", detail: "Apron and ingredients included", price: 45, left: 8 }],
  },
  {
    id: "brass-season",
    title: "Docklands Brass: Season Opener",
    category: "music",
    venue: VENUES.pier,
    day: 13,
    time: "20:00",
    minutes: 120,
    summary: "Twenty-piece brass band, a warehouse and a dance floor.",
    about:
      "The Docklands Brass Band opens its season with a standing show in the Pier 9 warehouse. Doors at 19:00, the band at 20:00. The dance floor is the whole room.",
    tiers: [ga(22, 180)],
  },
  {
    id: "hamlet-reading",
    title: "Hamlet: A Staged Reading",
    category: "theater",
    venue: VENUES.library,
    day: 16,
    time: "19:30",
    minutes: 160,
    summary: "Eight actors, eight music stands, the whole play.",
    about: "Eight actors read the whole of Hamlet from music stands, with live cello. Pay what you can at the door, or reserve a seat here.",
    tiers: [free(60), { id: "support", name: "Supporter seat", detail: "Front rows; helps fund the series", price: 20, left: 12 }],
  },
  {
    id: "night-sky",
    title: "Night Sky Talk and Telescopes",
    category: "talks",
    venue: VENUES.glasshouse,
    day: 19,
    time: "21:00",
    minutes: 90,
    summary: "What’s up this month, then telescopes on the lawn.",
    about:
      "The Harborview Astronomical Society explains this month’s sky, then sets up six telescopes on the garden lawn. If it’s cloudy, the talk runs longer.",
    tiers: [ga(6, 80)],
  },
  {
    id: "autumn-salon",
    title: "Autumn Salon: New Painters",
    category: "art",
    venue: VENUES.northgate,
    day: 24,
    time: "18:00",
    minutes: 180,
    summary: "Opening night for twelve painters in their first show.",
    about: "Northgate Gallery’s annual show of painters exhibiting for the first time. Opening night with the artists; the show runs for six weeks.",
    tiers: [free(150)],
  },
];

/** @param {string} id */
export function eventById(id) {
  return EVENTS.find((e) => e.id === id) ?? null;
}

/** When so few seats are left that the event says so. */
export const FEW_SEATS = 10;
