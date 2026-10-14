// A tree, as a project's files: folders open in place with their contents
// indented beneath, a tap selects a file and shows its details, and the path
// above goes back to any folder on the way. The toolbar under the tree sorts
// it, counts what is in it and adds a folder.

import { View, div, v_flex, Button } from "native-shell";
import { Breadcrumb, DescriptionList, DescriptionItem, DropdownMenu, GroupBox, Icon, Toolbar, Tree, TreeItem } from "gpui-component";
import { label, page } from "../ui.js";

/** @typedef {{ id: string, name: string, kind: string, size?: number, modified: string, children?: Node[] }} Node */

/** @type {Node[]} */
const PROJECT = [
  {
    id: "design",
    name: "Design",
    kind: "Folder",
    modified: "2026-10-09",
    children: [
      { id: "layout", name: "Bed layout.pdf", kind: "PDF document", size: 2_400_000, modified: "2026-10-09" },
      { id: "calendar", name: "Planting calendar.pdf", kind: "PDF document", size: 860_000, modified: "2026-09-28" },
    ],
  },
  {
    id: "notes",
    name: "Notes",
    kind: "Folder",
    modified: "2026-10-13",
    children: [
      { id: "soil", name: "Soil test results.txt", kind: "Plain text", size: 4_200, modified: "2026-10-02" },
      { id: "seeds", name: "Seed order.md", kind: "Markdown", size: 1_900, modified: "2026-10-13" },
    ],
  },
  {
    id: "photos",
    name: "Photos",
    kind: "Folder",
    modified: "2026-10-12",
    children: [
      {
        id: "october",
        name: "October",
        kind: "Folder",
        modified: "2026-10-12",
        children: [{ id: "harvest", name: "Harvest.jpg", kind: "JPEG image", size: 3_100_000, modified: "2026-10-12" }],
      },
      { id: "before", name: "Before.jpg", kind: "JPEG image", size: 2_800_000, modified: "2026-04-18" },
      { id: "after", name: "After.jpg", kind: "JPEG image", size: 2_950_000, modified: "2026-08-30" },
    ],
  },
  { id: "budget", name: "Budget.csv", kind: "CSV document", size: 12_000, modified: "2026-10-01" },
];

const SORTS = {
  name: { label: "Name", compare: (/** @type {Node} */ a, /** @type {Node} */ b) => a.name.localeCompare(b.name) },
  modified: { label: "Date Modified", compare: (/** @type {Node} */ a, /** @type {Node} */ b) => b.modified.localeCompare(a.modified) },
  size: { label: "Size", compare: (/** @type {Node} */ a, /** @type {Node} */ b) => total(b) - total(a) },
};

/** @param {Node} node @returns {number} */
function total(node) {
  return node.children ? node.children.reduce((sum, c) => sum + total(c), 0) : (node.size ?? 0);
}

/** @param {number} bytes */
function formatSize(bytes) {
  if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`;
  if (bytes >= 1_000) return `${Math.round(bytes / 1_000)} KB`;
  return bytes === 0 ? "Zero bytes" : `${bytes} bytes`;
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** @param {string} iso */
function formatDate(iso) {
  const [y, m, d] = iso.split("-").map(Number);
  return `${MONTHS[m - 1]} ${d}, ${y}`;
}

/** The nodes from the root down to `id`, or null. @param {Node[]} nodes @param {string} id @returns {Node[] | null} */
function pathTo(nodes, id) {
  for (const node of nodes) {
    if (node.id === id) return [node];
    const below = node.children ? pathTo(node.children, id) : null;
    if (below) return [node, ...below];
  }
  return null;
}

/** @param {Node[]} nodes @returns {[number, number]} folders and files */
function count(nodes) {
  return nodes.reduce(
    ([folders, files], n) => {
      const [f, d] = n.children ? count(n.children) : [0, 0];
      return n.children ? [folders + 1 + f, files + d] : [folders, files + 1];
    },
    [0, 0],
  );
}

export default class FilesPage extends View {
  init() {
    /** @type {Node[]} */
    this.project = PROJECT;
    /** @type {string | null} */
    this.selected = "harvest";
    /** @type {keyof typeof SORTS} */
    this.sort = "name";
    this.added = 0;
  }

  /** @param {string | null} id @param {import("native-shell").Context} cx */
  select(id, cx) {
    this.selected = id;
    cx.notify();
  }

  /** @param {import("native-shell").Context} cx */
  newFolder(cx) {
    this.added += 1;
    const id = `new-${this.added}`;
    const name = this.added === 1 ? "Untitled folder" : `Untitled folder ${this.added}`;
    const now = new Date();
    const modified = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(now.getDate()).padStart(2, "0")}`;
    this.project = [...this.project, { id, name, kind: "Folder", modified, children: [] }];
    this.selected = id;
    cx.haptic("light");
    cx.notify();
  }

  /** @param {Node[]} nodes @returns {import("gpui-component").TreeItemElement[]} */
  items(nodes) {
    const sorted = [...nodes].sort((a, b) => (a.children ? 0 : 1) - (b.children ? 0 : 1) || SORTS[this.sort].compare(a, b));
    return sorted.map((n) =>
      new TreeItem(n.id, n.name)
        .icon(n.children ? "icons/folder.svg" : n.kind === "JPEG image" ? "icons/file.svg" : "icons/file-text.svg")
        .expanded(n.id === "photos")
        .children(n.children ? this.items(n.children) : []),
    );
  }

  render(cx) {
    const t = cx.theme();
    const path = this.selected ? (pathTo(this.project, this.selected) ?? []) : [];
    const node = path.at(-1);
    const [folders, files] = count(this.project);
    return page(cx, {
      title: "Files",
      intro: "A garden project's files. Tap a folder to open it, a file to see its details.",
      children: [
        v_flex()
          .gap_1p5()
          .child(
            new Breadcrumb(["Garden Project", ...path.map((n) => n.name)])
              .test_id("path")
              .px_3()
              .on_item_click((i, cx) => this.select(i === 0 ? null : path[i - 1].id, cx)),
          )
          .child(
            v_flex()
              .rounded(t.radius.md)
              .bg(t.surface)
              .overflow_hidden()
              .child(
                new Tree("project")
                  .test_id("tree")
                  .p_1p5()
                  .selected_item(this.selected)
                  .on_select((id, cx) => this.select(id, cx))
                  .children(this.items(this.project)),
              )
              .child(div().h(0.5).bg(t.border))
              .child(
                new Toolbar("files-toolbar")
                  .child(
                    new DropdownMenu("sort", `Sort by ${SORTS[this.sort].label}`)
                      .test_id("sort")
                      .item("Name", (cx) => this.sortBy("name", cx))
                      .item("Date Modified", (cx) => this.sortBy("modified", cx))
                      .item("Size", (cx) => this.sortBy("size", cx)),
                  )
                  .child(label(cx, "footnote", t.muted_foreground, `${folders} folders, ${files} files`).test_id("count"))
                  .child(
                    Button.new("new-folder")
                      .test_id("new-folder")
                      .accessibility_label("New Folder")
                      .size_11()
                      .flex()
                      .items_center()
                      .justify_center()
                      .rounded_full()
                      .active((s) => s.bg(t.secondary))
                      .on_click((_e, cx) => this.newFolder(cx))
                      .child(new Icon("icons/plus.svg").color(t.primary)),
                  ),
              ),
          ),
        node
          ? new GroupBox().title(node.children ? "Folder" : "File").child(
              new DescriptionList()
                .test_id("details")
                .child(new DescriptionItem("Name").value(node.name))
                .child(new DescriptionItem("Kind").value(node.kind))
                .child(new DescriptionItem("Size").value(node.children && node.children.length === 0 ? "Empty" : formatSize(total(node))))
                .child(new DescriptionItem("Modified").value(formatDate(node.modified))),
            )
          : label(cx, "subheadline", t.muted_foreground, "Select a file to see its details.").test_id("no-selection").px_4(),
      ],
    });
  }

  /** @param {keyof typeof SORTS} sort @param {import("native-shell").Context} cx */
  sortBy(sort, cx) {
    this.sort = sort;
    cx.notify();
  }
}
