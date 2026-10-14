// The store's catalog: a small range of everyday goods from an invented
// brand. Prices are in cents. Each color has its own photo, and every product
// has a close-up, so the carousel on the product page shows what a buyer
// would ask about (the color in hand, the material up close).

/**
 * @typedef {{ id: string, label: string }} Option
 * @typedef {{
 *   id: string,
 *   name: string,
 *   category: string,
 *   price: number,
 *   was?: number,
 *   rating: number,
 *   reviews: number,
 *   summary: string,
 *   details: string[],
 *   colors: Option[],
 *   sizes?: Option[],
 *   isNew?: boolean,
 * }} Product
 */

export const BRAND = "Larch Supply";

export const CATEGORIES = [
  { id: "all", label: "All" },
  { id: "kitchen", label: "Kitchen" },
  { id: "home", label: "Home" },
  { id: "bags", label: "Bags" },
  { id: "desk", label: "Desk" },
];

/** @type {Product[]} */
export const PRODUCTS = [
  {
    id: "mug",
    name: "Stoneware Mug",
    category: "kitchen",
    price: 1800,
    rating: 4.8,
    reviews: 412,
    summary: "A heavy, hand-glazed mug that keeps coffee warm through the first email of the day.",
    details: ["Holds 350 ml (12 oz)", "Dishwasher and microwave safe", "Each glaze varies slightly"],
    colors: [
      { id: "sand", label: "Sand" },
      { id: "sage", label: "Sage" },
      { id: "charcoal", label: "Charcoal" },
    ],
  },
  {
    id: "kettle",
    name: "Pour-Over Kettle",
    category: "kitchen",
    price: 6400,
    rating: 4.6,
    reviews: 128,
    summary: "A gooseneck spout for a slow, even pour, and a handle that stays cool.",
    details: ["1 liter capacity", "Works on gas, electric and induction", "Brushed stainless steel body"],
    colors: [
      { id: "black", label: "Matte Black" },
      { id: "steel", label: "Steel" },
    ],
  },
  {
    id: "towels",
    name: "Linen Tea Towels, Set of 2",
    category: "kitchen",
    price: 2200,
    rating: 4.5,
    reviews: 86,
    summary: "Washed linen that dries glasses without lint and softens with every wash.",
    details: ["100% European flax linen", "50 × 70 cm each", "Hanging loop in one corner"],
    colors: [{ id: "natural", label: "Natural" }],
  },
  {
    id: "lamp",
    name: "Ceramic Table Lamp",
    category: "home",
    price: 8900,
    rating: 4.7,
    reviews: 64,
    summary: "A warm, dimmable glow for a bedside or a reading corner.",
    details: ["Linen shade, ceramic base", "Dimmable LED bulb included", "42 cm tall, 1.8 m fabric cord"],
    colors: [
      { id: "white", label: "Chalk" },
      { id: "terracotta", label: "Terracotta" },
    ],
    isNew: true,
  },
  {
    id: "throw",
    name: "Wool Throw Blanket",
    category: "home",
    price: 9600,
    was: 12000,
    rating: 4.9,
    reviews: 233,
    summary: "A soft lambswool throw, woven with a subtle herringbone and fringed edges.",
    details: ["100% lambswool", "130 × 180 cm", "Dry clean or hand wash cold"],
    colors: [
      { id: "oat", label: "Oat" },
      { id: "rust", label: "Rust" },
    ],
  },
  {
    id: "planter",
    name: "Terracotta Planter",
    category: "home",
    price: 2800,
    rating: 4.4,
    reviews: 51,
    summary: "Unglazed clay that lets roots breathe, with a matching saucer to catch the water.",
    details: ["Drainage hole and saucer", "Frost resistant", "Small 12 cm · Medium 16 cm · Large 22 cm"],
    colors: [{ id: "clay", label: "Clay" }],
    sizes: [
      { id: "s", label: "Small" },
      { id: "m", label: "Medium" },
      { id: "l", label: "Large" },
    ],
  },
  {
    id: "daypack",
    name: "Canvas Daypack",
    category: "bags",
    price: 7800,
    rating: 4.6,
    reviews: 197,
    summary: "Waxed canvas with a padded sleeve for a 14-inch laptop and a quick-access front pocket.",
    details: ["20 liters", "Water-resistant waxed canvas", "Padded laptop sleeve fits up to 14 inches"],
    colors: [
      { id: "olive", label: "Olive" },
      { id: "navy", label: "Navy" },
    ],
  },
  {
    id: "tote",
    name: "Market Tote",
    category: "bags",
    price: 5400,
    was: 6800,
    rating: 4.3,
    reviews: 72,
    summary: "A roomy tote with leather handles that holds a week of groceries.",
    details: ["Heavy cotton canvas", "Leather handles, 25 cm drop", "Inside zip pocket"],
    colors: [{ id: "tan", label: "Tan" }],
  },
  {
    id: "notebook",
    name: "Dot-Grid Notebook",
    category: "desk",
    price: 1400,
    rating: 4.8,
    reviews: 520,
    summary: "Lay-flat binding and fountain-pen-friendly paper for lists, sketches and plans.",
    details: ["160 pages of 100 g/m² paper", "Numbered pages and an index", "Lay-flat stitched binding"],
    colors: [{ id: "forest", label: "Forest" }],
    sizes: [
      { id: "a5", label: "A5" },
      { id: "a6", label: "A6" },
    ],
  },
  {
    id: "pen",
    name: "Brass Pen",
    category: "desk",
    price: 3600,
    rating: 4.5,
    reviews: 143,
    summary: "Solid brass that darkens with use, taking standard gel refills.",
    details: ["Solid brass barrel", "Takes standard 0.5 mm gel refills", "Black ink refill included"],
    colors: [{ id: "brass", label: "Brass" }],
    isNew: true,
  },
];

/** @returns {Product | undefined} */
export function productById(id) {
  return PRODUCTS.find((p) => p.id === id);
}

/** The photo of one color. */
export function photo(product, colorId = product.colors[0].id) {
  return `images/${product.id}-${colorId}.png`;
}

/** The carousel's slides: one per color, then the close-up. */
export function photos(product) {
  return [...product.colors.map((c) => ({ src: photo(product, c.id), label: `${product.name} in ${c.label}` })), {
    src: `images/${product.id}-detail.png`,
    label: `${product.name}, close-up`,
  }];
}

/** Products whose name, category or description mention every word of `query`. */
export function search(query) {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return [];
  return PRODUCTS.filter((p) => {
    const category = CATEGORIES.find((c) => c.id === p.category)?.label ?? "";
    const haystack = `${p.name} ${category} ${p.summary} ${p.colors.map((c) => c.label).join(" ")}`.toLowerCase();
    return words.every((w) => haystack.includes(w));
  });
}
