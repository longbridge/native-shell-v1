// The library: folders and notes, kept in one JSON file through `fs/promises`.
//
// Every page reads the same library and redraws when it changes, so a note
// edited in the editor is already up to date in the list behind it. Writes
// are coalesced (typing saves a moment after the last keystroke) and go to a
// temporary file first, so a failed write never leaves half a library on
// disk. A first run starts with sample notes; a file that can't be read is
// reported, never overwritten.

import { readFile, writeFile, rename, exists } from "fs/promises";
/** @import { AsyncContext, Context, Entity } from "native-shell" */

const FILE = "notes.json";
const SAVE_DELAY_MS = 400;

/** The folder every library has; it can't be renamed or deleted. */
export const DEFAULT_FOLDER = "notes";
/** Not a folder: every note, wherever it is filed. */
export const ALL_NOTES = "all";

/** @typedef {{ id: string, name: string }} Folder */
/** @typedef {{ id: string, folder: string, body: string, pinned: boolean, updated: number }} Note */

class Library {
  constructor() {
    /** @type {"loading" | "ready" | "error"} */
    this.status = "loading";
    /** @type {Folder[]} */
    this.folders = [];
    /** @type {Note[]} */
    this.notes = [];
    this.nextId = 1;
    this.saveFailed = false;
    /** @type {Set<Entity>} */
    this.watchers = new Set();
    this.saveTimer = null;
    this.writing = Promise.resolve();
  }

  /** Redraws `entity` (a page) whenever the library changes. */
  watch(entity) {
    this.watchers.add(entity);
  }

  /** @param {AsyncContext} cx */
  async load(cx) {
    this.status = "loading";
    this.changed(cx, { save: false });
    try {
      if (await exists(FILE)) {
        const data = parse(await readFile(FILE, "utf8"));
        this.folders = data.folders;
        this.notes = data.notes;
        this.nextId = data.nextId;
      } else {
        const data = samples(Date.now());
        this.folders = data.folders;
        this.notes = data.notes;
        this.nextId = data.notes.length + 1;
      }
      this.status = "ready";
    } catch (e) {
      console.warn(`notes: could not read ${FILE}: ${e?.message ?? e}`);
      this.status = "error";
    }
    this.changed(cx, { save: false });
  }

  /** @param {Context} cx */
  changed(cx, { save = true } = {}) {
    for (const entity of this.watchers) cx.notify(entity);
    if (save) this.scheduleSave(cx);
  }

  /** @param {Context} cx */
  scheduleSave(cx) {
    this.saveTimer?.cancel();
    // Owned by no view: a note edited just before its page closes still saves.
    this.saveTimer = cx.timer.after(SAVE_DELAY_MS, (cx) => this.save(cx), { owner: null });
  }

  /** Writes now instead of after the delay (leaving the editor). */
  flush(cx) {
    if (!this.saveTimer || this.saveTimer.is_done()) return;
    this.saveTimer.cancel();
    this.save(cx);
  }

  save(cx) {
    this.saveTimer = null;
    const json = JSON.stringify({ version: 1, nextId: this.nextId, folders: this.folders, notes: this.notes });
    // One write at a time, in order, so an older library never lands last.
    const previous = this.writing;
    this.writing = new Promise((done) =>
      cx.spawn(
        async (cx) => {
          await previous;
          let failed = false;
          try {
            await writeFile(`${FILE}.tmp`, json);
            await rename(`${FILE}.tmp`, FILE);
          } catch (e) {
            console.warn(`notes: could not save: ${e?.message ?? e}`);
            failed = true;
          }
          if (failed !== this.saveFailed) {
            this.saveFailed = failed;
            this.changed(cx, { save: false });
          }
          done(undefined);
        },
        { owner: null },
      ),
    );
  }

  // Folders ------------------------------------------------------------------

  folder(id) {
    return this.folders.find((f) => f.id === id) ?? null;
  }

  /** The folder named `name`, ignoring case, other than `except`. */
  folderNamed(name, except = null) {
    const key = name.trim().toLocaleLowerCase();
    return this.folders.find((f) => f.id !== except && f.name.toLocaleLowerCase() === key) ?? null;
  }

  count(folderId) {
    return folderId === ALL_NOTES ? this.notes.length : this.notes.filter((n) => n.folder === folderId).length;
  }

  addFolder(name, cx) {
    const folder = { id: `f${this.nextId++}`, name: name.trim() };
    this.folders = [...this.folders, folder];
    this.changed(cx);
    return folder;
  }

  renameFolder(id, name, cx) {
    this.folders = this.folders.map((f) => (f.id === id ? { ...f, name: name.trim() } : f));
    this.changed(cx);
  }

  /** Deletes a folder and the notes in it. */
  deleteFolder(id, cx) {
    if (id === DEFAULT_FOLDER) return;
    this.folders = this.folders.filter((f) => f.id !== id);
    this.notes = this.notes.filter((n) => n.folder !== id);
    this.changed(cx);
  }

  // Notes --------------------------------------------------------------------

  note(id) {
    return this.notes.find((n) => n.id === id) ?? null;
  }

  /** A note that isn't in the library yet: it joins on its first character. */
  draft(folder) {
    return { id: `n${this.nextId++}`, folder: folder === ALL_NOTES ? DEFAULT_FOLDER : folder, body: "", pinned: false, updated: Date.now() };
  }

  /** Stores `note` (adding it when new) with a fresh modification time. */
  put(note, cx) {
    const next = { ...note, updated: Date.now() };
    this.notes = this.note(note.id) ? this.notes.map((n) => (n.id === note.id ? next : n)) : [next, ...this.notes];
    this.changed(cx);
    return next;
  }

  /** Changes a note's place or pin without touching its modification time. */
  patch(ids, change, cx) {
    this.notes = this.notes.map((n) => (ids.includes(n.id) ? { ...n, ...change } : n));
    this.changed(cx);
  }

  /** Removes notes and answers how to put them back. */
  remove(ids, cx) {
    const gone = this.notes.filter((n) => ids.includes(n.id));
    this.notes = this.notes.filter((n) => !ids.includes(n.id));
    this.changed(cx);
    return (cx) => {
      // A folder deleted in the meantime hands its notes back to the default one.
      const back = gone
        .filter((n) => !this.note(n.id))
        .map((n) => (this.folder(n.folder) ? n : { ...n, folder: DEFAULT_FOLDER }));
      this.notes = [...this.notes, ...back];
      this.changed(cx);
    };
  }

  /** Notes in a folder (or every note), most recently edited first. */
  notesIn(folderId) {
    const notes = folderId === ALL_NOTES ? this.notes : this.notes.filter((n) => n.folder === folderId);
    return [...notes].sort((a, b) => b.updated - a.updated);
  }

  /** Notes carrying `#tag`, most recently edited first. */
  tagged(tag) {
    return this.notesIn(ALL_NOTES).filter((n) => tagsOf(n.body).includes(tag));
  }

  /** Every tag in use, with how many notes carry it, most used first. */
  tags() {
    const counts = new Map();
    for (const n of this.notes) for (const tag of tagsOf(n.body)) counts.set(tag, (counts.get(tag) ?? 0) + 1);
    return [...counts].map(([tag, count]) => ({ tag, count })).sort((a, b) => b.count - a.count || a.tag.localeCompare(b.tag));
  }

  /** Notes whose text contains every word of `query`, most recently edited first. */
  search(query, notes = this.notesIn(ALL_NOTES)) {
    const words = terms(query);
    if (words.length === 0) return [];
    return notes.filter((n) => {
      const text = n.body.toLocaleLowerCase();
      return words.every((w) => text.includes(w));
    });
  }
}

export const library = new Library();

/** A library read from disk, checked field by field; anything else is an error. */
function parse(json) {
  const data = JSON.parse(json);
  if (!data || !Array.isArray(data.folders) || !Array.isArray(data.notes)) throw new Error("not a notes library");
  const folders = data.folders.filter((f) => f && typeof f.id === "string" && typeof f.name === "string");
  if (!folders.some((f) => f.id === DEFAULT_FOLDER)) folders.unshift({ id: DEFAULT_FOLDER, name: "Notes" });
  const notes = data.notes
    .filter((n) => n && typeof n.id === "string" && typeof n.body === "string")
    .map((n) => ({
      id: n.id,
      folder: folders.some((f) => f.id === n.folder) ? n.folder : DEFAULT_FOLDER,
      body: n.body,
      pinned: !!n.pinned,
      updated: Number.isFinite(n.updated) ? n.updated : 0,
    }));
  const nextId = Number.isInteger(data.nextId) ? data.nextId : folders.length + notes.length + 1;
  return { folders, notes, nextId };
}

// Reading a note -------------------------------------------------------------

/** The lowercase words of a search. */
export function terms(query) {
  return query.toLocaleLowerCase().split(/\s+/).filter(Boolean);
}

/** A line as it reads, without its Markdown markers. */
export function plain(line) {
  return line
    .replace(/^\s*#{1,6}\s+/, "")
    .replace(/^\s*[-*+]\s+\[[ xX]\]\s+/, "")
    .replace(/^\s*([-*+]|\d+\.)\s+/, "")
    .replace(/^\s*>\s?/, "")
    .replace(/\*\*|__|~~|`/g, "")
    .replace(/(^|[\s(])[*_]([^*_\s][^*_]*)[*_](?=$|[\s.,;:!?)])/g, "$1$2")
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
    .trim();
}

function lines(body) {
  return body.split("\n").map(plain).filter(Boolean);
}

/** A note's title: its first line, as the list shows it. */
export function titleOf(body) {
  return lines(body)[0] ?? "New Note";
}

/** The line under the title in the list. */
export function previewOf(body) {
  return lines(body)[1] ?? "No additional text";
}

/** The `#tags` in a note, lowercase, without the `#`. */
export function tagsOf(body) {
  const tags = new Set();
  for (const m of body.matchAll(/(?:^|\s)#([\p{L}\p{N}_-]+)/gu)) tags.add(m[1].toLocaleLowerCase());
  return [...tags];
}

/** A note's checklist: how many items, how many done. */
export function checklistOf(body) {
  const items = body.split("\n").filter((l) => CHECK_ITEM.test(l));
  return { total: items.length, done: items.filter((l) => /\[[xX]\]/.test(l)).length };
}

/** `- [ ] item` or `- [x] item`. */
export const CHECK_ITEM = /^\s*[-*+]\s+\[[ xX]\]\s/;

/** The body with the checklist item on `line` flipped. */
export function toggleItem(body, line) {
  const all = body.split("\n");
  const l = all[line];
  if (l === undefined || !CHECK_ITEM.test(l)) return body;
  all[line] = l.replace(/\[([ xX])\]/, (_m, c) => (c === " " ? "[x]" : "[ ]"));
  return all.join("\n");
}

// Sample notes ---------------------------------------------------------------

const HOUR = 3600_000;
const DAY = 24 * HOUR;

function samples(now) {
  const at = (ago) => now - ago;
  const folders = [
    { id: DEFAULT_FOLDER, name: "Notes" },
    { id: "f-recipes", name: "Recipes" },
    { id: "f-travel", name: "Travel" },
    { id: "f-work", name: "Work" },
  ];
  const notes = [
    {
      folder: DEFAULT_FOLDER,
      pinned: true,
      updated: at(2 * HOUR),
      body: "Groceries\n- [ ] Oat milk\n- [x] Sourdough loaf\n- [ ] Cherry tomatoes\n- [ ] Parmesan\n- [x] Coffee beans\n- [ ] Lemons\n\n#groceries",
    },
    {
      folder: "f-travel",
      pinned: true,
      updated: at(5 * HOUR),
      body:
        "# Lisbon in November\n" +
        "Flights booked, **TP 1347** on the 14th, back on the 19th.\n\n" +
        "## Places to stay\n" +
        "- Alfama: close to the castle, steep streets\n" +
        "- Príncipe Real: quieter, good cafés\n\n" +
        "## To try\n" +
        "1. Pastéis at Manteigaria\n" +
        "2. Tram 28 early in the morning, before the queues\n" +
        "3. Sunset at the Miradouro da Graça\n\n" +
        "#travel",
    },
    {
      folder: DEFAULT_FOLDER,
      pinned: false,
      updated: at(26 * HOUR),
      body:
        "Book club, October\n" +
        "We're reading *The Remains of the Day*. Meeting at Ana's on the 24th, 7 pm. I'm bringing the dessert.\n\n" +
        "Questions to raise:\n" +
        "- Is Stevens a reliable narrator?\n" +
        "- What does the journey change, if anything?\n\n" +
        "#books",
    },
    {
      folder: "f-recipes",
      pinned: false,
      updated: at(3 * DAY),
      body:
        "Sourdough starter\n" +
        "Feed 1:1:1 by weight: 50 g starter, 50 g flour, 50 g water.\n\n" +
        "1. Discard all but 50 g\n" +
        "2. Mix in the flour and lukewarm water\n" +
        "3. Mark the jar and leave it at room temperature\n" +
        "4. Ready when it has doubled, usually 4 to 6 hours\n\n" +
        "> If it smells of nail polish it's hungry: feed it twice a day for a while.\n\n" +
        "#baking",
    },
    {
      folder: "f-recipes",
      pinned: false,
      updated: at(9 * DAY),
      body:
        "Weeknight dal\n" +
        "Serves 4, about 35 minutes.\n\n" +
        "- [ ] 250 g red lentils, rinsed\n" +
        "- [ ] 1 onion, 3 garlic cloves, thumb of ginger\n" +
        "- [ ] 1 tsp each cumin, turmeric, garam masala\n" +
        "- [ ] 400 ml coconut milk\n\n" +
        "Simmer the lentils in 800 ml water until soft. Fry the aromatics and spices, stir them in with the coconut milk and finish with lime.\n\n" +
        "#dinner",
    },
    {
      folder: "f-work",
      pinned: false,
      updated: at(4 * HOUR),
      body: quarterly(),
    },
    {
      folder: "f-work",
      pinned: false,
      updated: at(2 * DAY),
      body:
        "1:1 with Priya\n" +
        "- Onboarding doc is out of date: offered to rewrite the setup section\n" +
        "- She wants to own the release checklist next quarter\n" +
        "- Follow up on the conference budget\n\n" +
        "#meetings",
    },
    {
      folder: "f-travel",
      pinned: false,
      updated: at(18 * DAY),
      body:
        "Packing list\n" +
        "- [x] Passport\n- [x] Chargers and adapter\n- [ ] Rain jacket\n- [ ] Walking shoes\n- [ ] Book for the flight\n\n#travel",
    },
    {
      folder: DEFAULT_FOLDER,
      pinned: false,
      updated: at(41 * DAY),
      body:
        "Gift ideas\n" +
        "Mum: the ceramics class she mentioned, or a good bread knife.\n" +
        "Tom: [the trail guide](https://example.org/trails) for the coast path.",
    },
    {
      folder: DEFAULT_FOLDER,
      pinned: false,
      updated: at(75 * DAY),
      body: "Bike maintenance\nLube the chain every 300 km or after rain. Tyres at 4.5 bar front, 5 bar rear. Brake pads were replaced in July.",
    },
  ];
  return { folders, notes: notes.map((n, i) => ({ id: `n${i + 1}`, ...n })) };
}

/** A long note: a planning document with sections, lists and a checklist. */
function quarterly() {
  const section = (title, items) => `## ${title}\n${items.map((i) => `- ${i}`).join("\n")}\n`;
  return [
    "Quarterly planning, Q4",
    "Notes from the planning session. Owners in brackets; anything without an owner is up for grabs.\n",
    section("Where we are", [
      "Sign-ups grew 18% on last quarter, mostly from the referral programme",
      "Support volume is flat, but the median first reply slipped from 3 to 5 hours",
      "Two of the five Q3 goals shipped; the export rewrite moved to this quarter",
      "The mobile app's crash-free rate is 99.2%, up from 98.6%",
    ]),
    section("Goals", [
      "Ship the new export, CSV and PDF first (Sam)",
      "Bring first replies back under 3 hours: rota changes and saved answers (Lena)",
      "Offline mode for the mobile app, read-only to start (Priya)",
      "Accessibility review of the sign-up flow with an external auditor",
      "Retire the legacy billing page",
    ]),
    section("Risks", [
      "Offline mode depends on the sync API changes landing in October",
      "The auditor is booked for the second half of November only",
      "Holiday cover in December is thin on the support rota",
    ]),
    "## Before the next session\n- [x] Share the Q3 numbers with the team\n- [ ] Draft the export spec\n- [ ] Book the auditor\n- [ ] Ask finance about the conference budget\n- [ ] Collect holiday plans for the rota\n",
    section("Parking lot", [
      "Dark mode for the marketing site",
      "A public roadmap page: who would maintain it?",
      "Moving the weekly sync to Thursdays",
      "Whether the referral rewards should change in January",
      "A shared glossary for support and engineering",
    ]),
    "#planning #meetings",
  ].join("\n");
}
