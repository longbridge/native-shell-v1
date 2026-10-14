// Notes: write things down, find them again.
//
// The first screen is the library: folders with how many notes each holds,
// the tags in use, and one search field across everything. A folder opens its
// notes; a note opens rendered, ready to read or tick off, and Edit turns it
// back into text. Folders are made from the toolbar and renamed or deleted
// from their menu (a long press, or Edit). See README.md for the design notes.

import { View, div, h_flex, v_flex, Button, InputState, Spinner } from "native-shell";
import { Tag } from "gpui-component";
import { library, ALL_NOTES, DEFAULT_FOLDER } from "./store.js";
import { actionSheet, barButton, emptyState, font, icon, largeTitle, navBar, saveError, searchField, section, tintedButton, toolbar } from "./ui.js";
import { folderDialog } from "./dialogs.js";
import { noteRow } from "./rows.js";
import { bindShortcuts, shortcuts } from "./keys.js";
import NotesPage from "./notes.js";
import EditorPage from "./editor.js";
/** @import { Folder } from "./store.js" */

const COLLAPSE_AT = 44;

export default class Notes extends View {
  init(_props, cx) {
    this.collapsed = false;
    this.editing = false;
    this.search = InputState.new({ placeholder: "Search all notes", return_key: "search", auto_capitalize: "none" });
    this.search.on("change", (_e, cx) => cx.notify());
    library.watch(cx.entity());
    bindShortcuts(cx);
    library.load(cx);
  }

  openFolder(id, cx) {
    this.search.blur();
    cx.navigator.push(NotesPage, { folder: id }, { title: id === ALL_NOTES ? "All Notes" : library.folder(id)?.name });
  }

  openTag(tag, cx) {
    cx.navigator.push(NotesPage, { tag }, { title: `#${tag}` });
  }

  compose(cx) {
    cx.navigator.push(EditorPage, { folder: DEFAULT_FOLDER, back: "Folders" }, { title: "New Note" });
  }

  newFolder(cx) {
    folderDialog(cx, {
      title: "New Folder",
      message: "Enter a name for this folder.",
      save: (name, cx) => {
        library.addFolder(name, cx);
        cx.haptic("success");
      },
    });
  }

  /** @param {Folder} folder */
  folderMenu(folder, cx) {
    const count = library.count(folder.id);
    actionSheet(cx, {
      id: "folder-menu",
      title: folder.name,
      message: count === 1 ? "1 note" : `${count} notes`,
      actions: [
        {
          id: "folder-rename",
          label: "Rename",
          icon: "pencil",
          run: (cx) =>
            folderDialog(cx, {
              title: "Rename Folder",
              message: `Enter a new name for “${folder.name}”.`,
              folder,
              save: (name, cx) => library.renameFolder(folder.id, name, cx),
            }),
        },
        { id: "folder-delete", label: "Delete Folder", icon: "trash", destructive: true, run: (cx) => this.confirmDelete(folder, cx) },
      ],
    });
  }

  /** Deleting a folder takes its notes with it, so it asks first and says how many. */
  confirmDelete(folder, cx) {
    const count = library.count(folder.id);
    cx.alert({
      title: `Delete “${folder.name}”?`,
      message: count === 0 ? "The folder is empty." : `${count === 1 ? "Its note" : `All ${count} notes in it`} will be deleted too. This can’t be undone.`,
      actions: [
        { label: "Cancel", style: "cancel" },
        {
          label: "Delete",
          style: "destructive",
          on_press: (cx) => {
            library.deleteFolder(folder.id, cx);
            cx.haptic("success");
          },
        },
      ],
    });
  }

  onScroll(e, cx) {
    const collapsed = e.y > COLLAPSE_AT;
    if (collapsed === this.collapsed) return;
    this.collapsed = collapsed;
    cx.notify();
  }

  render(cx) {
    const ready = library.status === "ready";
    return v_flex()
      .test_id("folders-page")
      .size_full()
      .bg(cx.theme().grouped_background)
      .key_context("Folders")
      .map(
        shortcuts({
          new_note: (cx) => ready && this.compose(cx),
          new_folder: (cx) => ready && this.newFolder(cx),
          search: () => this.search.focus(),
        }),
      )
      .child(
        navBar(cx, {
          id: "folders",
          title: "Folders",
          collapsed: this.collapsed,
          trailing: [
            barButton(cx, {
              id: "edit",
              label: this.editing ? "Done" : "Edit",
              emphasized: this.editing,
              disabled: !ready || library.folders.length < 2,
              on_click: (_e, cx) => {
                this.editing = !this.editing;
                cx.notify();
              },
            }),
          ],
        }),
      )
      .when(library.saveFailed, (el) => el.child(saveError(cx)))
      .child(
        v_flex()
          .id("folders-scroll")
          .test_id("folders-scroll")
          .flex_1()
          .overflow_y_scroll()
          .on_scroll((e, cx) => this.onScroll(e, cx))
          .child(largeTitle(cx, "Folders"))
          .when(ready, (el) => el.child(searchField(cx, { id: "folders-search", state: this.search, label: "Search all notes" })))
          .child(v_flex().pt_2().pb_8().gap_6().children(this.content(cx))),
      )
      .child(
        toolbar(cx, {
          id: "folders",
          leading: barButton(cx, {
            id: "new-folder",
            label: "New Folder",
            icon: "folder_plus",
            tooltip: "New Folder (⇧⌘N)",
            disabled: !ready,
            on_click: (_e, cx) => this.newFolder(cx),
          }),
          status: ready ? (library.notes.length === 1 ? "1 Note" : `${library.notes.length} Notes`) : "",
          trailing: barButton(cx, { id: "folders-compose", label: "New Note", icon: "compose", tooltip: "New Note (⌘N)", disabled: !ready, on_click: (_e, cx) => this.compose(cx) }),
        }),
      );
  }

  content(cx) {
    if (library.status === "loading") return [this.loading(cx)];
    if (library.status === "error") return [this.loadError(cx)];
    const query = this.search.value().trim();
    return query !== "" ? this.results(cx, query) : [this.folders(cx), this.tags(cx)];
  }

  loading(cx) {
    const t = cx.theme();
    return v_flex()
      .test_id("loading")
      .items_center()
      .gap_3()
      .py_16()
      .child(Spinner.new().size_6())
      .child(div().map(font(cx, "subheadline")).text_color(t.muted_foreground).child("Loading notes…"));
  }

  loadError(cx) {
    return emptyState(cx, {
      id: "load-error",
      icon: "alert",
      title: "Couldn’t Open Your Notes",
      body: "The notes file on this device couldn’t be read. Nothing has been changed or deleted.",
      action: tintedButton(cx, { id: "retry", label: "Try Again", on_click: (_e, cx) => library.load(cx) }),
    });
  }

  folders(cx) {
    const t = cx.theme();
    const row = (/** @type {{ id: string, name: string, icon: "folder" | "stack", editable: boolean }} */ f) => {
      const count = library.count(f.id);
      const menu = this.editing && f.editable;
      return Button.new(`folder-${f.id}`)
        .test_id(`folder-${f.id}`)
        .accessibility_label(`${f.name}, ${countLabel(count)}`)
        .accessibility_hint(f.editable ? "Opens the folder. Long press to rename or delete." : "Opens the folder.")
        .flex()
        .items_center()
        .gap_3()
        .pl_4()
        .pr_3()
        .py_2p5()
        .min_h_11()
        .active((s) => s.bg(t.muted))
        .on_click((_e, cx) => (menu ? this.folderMenu(/** @type {Folder} */ (library.folder(f.id)), cx) : this.openFolder(f.id, cx)))
        .when(f.editable, (el) => el.on_long_press((_e, cx) => this.folderMenu(/** @type {Folder} */ (library.folder(f.id)), cx)))
        .child(icon(f.icon, t.primary))
        .child(div().flex_1().min_w_0().map(font(cx, "body")).text_color(t.foreground).truncate().child(f.name))
        .child(div().test_id(`count-${f.id}`).map(font(cx, "body")).text_color(t.muted_foreground).child(String(count)))
        .child(
          menu
            ? icon("more", t.primary)
            : this.editing
              ? div().w(22)
              : icon("chevron_right", t.muted_foreground, 18),
        );
    };
    const rows = [
      { id: ALL_NOTES, name: "All Notes", icon: /** @type {const} */ ("stack"), editable: false },
      ...library.folders.map((f) => ({ id: f.id, name: f.name, icon: /** @type {const} */ ("folder"), editable: f.id !== DEFAULT_FOLDER })),
    ].map(row);
    return section(cx, {
      id: "folders",
      title: "On This Device",
      footer: this.editing ? "Tap a folder to rename or delete it." : undefined,
      rows,
      inset: 50,
    });
  }

  tags(cx) {
    const t = cx.theme();
    const tags = library.tags();
    if (tags.length === 0) return null;
    return v_flex()
      .px_4()
      .gap_2()
      .child(div().role("header").px_4().map(font(cx, "footnote")).text_color(t.muted_foreground).child("TAGS"))
      .child(
        h_flex()
          .test_id("tags")
          .flex_wrap()
          .gap_2()
          .children(
            tags.map(({ tag, count }) =>
              Button.new(`tag-${tag}`)
                .test_id(`tag-${tag}`)
                .accessibility_label(`Tag ${tag}, ${countLabel(count)}`)
                .hit_slop(6)
                .rounded_full()
                .active((s) => s.opacity(0.6))
                .on_click((_e, cx) => this.openTag(tag, cx))
                .child(
                  Tag.new()
                    .rounded_full()
                    .gap_1()
                    .px_3()
                    .py_1p5()
                    .bg(t.surface)
                    .child(div().map(font(cx, "subheadline")).text_color(t.foreground).child(`#${tag}`))
                    .child(div().map(font(cx, "footnote")).text_color(t.muted_foreground).child(String(count))),
                ),
            ),
          ),
      );
  }

  results(cx, query) {
    const found = library.search(query);
    if (found.length === 0) {
      return [
        emptyState(cx, {
          id: "no-results",
          icon: "search",
          title: "No Results",
          body: `No notes match “${query}”. Check the spelling or try another word.`,
        }),
      ];
    }
    return [
      section(cx, {
        id: "results",
        title: found.length === 1 ? "1 Note Found" : `${found.length} Notes Found`,
        rows: found.map((note) =>
          noteRow(cx, note, {
            query,
            showFolder: true,
            open: (cx) => {
              this.search.blur();
              cx.navigator.push(EditorPage, { id: note.id, back: "Folders" }, { title: "" });
            },
          }),
        ),
      }),
    ];
  }
}

function countLabel(count) {
  return count === 1 ? "1 note" : `${count} notes`;
}
