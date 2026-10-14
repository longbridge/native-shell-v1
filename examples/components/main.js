// Components: every control the shell renders, each shown where it belongs.
//
// Not a dump of widgets: each page is a small, believable scenario (a post
// composer, an address form, checkout preferences, an upload, a booking, a
// shared document) that uses its components for their meaning and shows their
// states—rest, pressed, disabled, loading, error. Pages push natively and
// return with the back button or the platform's back gesture.

import { View, v_flex } from "native-shell";
import ButtonsPage from "./pages/buttons.js";
import FieldsPage from "./pages/fields.js";
import SelectionPage from "./pages/selection.js";
import ProgressPage from "./pages/progress.js";
import PickersPage from "./pages/pickers.js";
import FormsPage from "./pages/forms.js";
import OverlaysPage from "./pages/overlays.js";
import { largeTitle, row, section, statusBar } from "./ui.js";

const SECTIONS = [
  {
    title: "Actions",
    pages: [{ id: "buttons", icon: "pointer", title: "Buttons", detail: "A post composer", page: ButtonsPage }],
  },
  {
    title: "Input",
    pages: [
      { id: "fields", icon: "text", title: "Text fields", detail: "A shipping address form", page: FieldsPage },
      { id: "selection", icon: "toggle", title: "Selection controls", detail: "Checkout preferences", page: SelectionPage },
      { id: "progress", icon: "sliders", title: "Sliders and progress", detail: "Slideshow settings and an upload", page: ProgressPage },
      { id: "pickers", icon: "calendar", title: "Pickers", detail: "A table booking", page: PickersPage },
      { id: "forms", icon: "file", title: "Forms", detail: "A flower delivery checkout", page: FormsPage },
    ],
  },
  {
    title: "Feedback",
    pages: [{ id: "overlays", icon: "layers", title: "Overlays", detail: "Sheet, dialog, toast and alert", page: OverlaysPage }],
  },
];

export default class Components extends View {
  render(cx) {
    const t = cx.theme();
    return v_flex()
      .size_full()
      .bg(t.background)
      .line_height(1.375)
      .child(statusBar(cx))
      .child(
        v_flex()
          .test_id("index")
          .flex_1()
          .overflow_y_scroll()
          .child(largeTitle(cx, "Components", "Each control in a real task, with its states"))
          .child(
            v_flex()
              .px_4()
              .pt_4()
              .pb(cx.window().safe_area.bottom + 32)
              .gap_6()
              .children(
                SECTIONS.map((s) =>
                  section(cx, {
                    title: s.title,
                    rows: s.pages.map((p) =>
                      row(cx, {
                        id: `open-${p.id}`,
                        icon: p.icon,
                        label: p.title,
                        detail: p.detail,
                        chevron: true,
                        on_click: (_e, cx) => cx.navigator.push(p.page, {}, { title: p.title }),
                      }),
                    ),
                  }),
                ),
              ),
          ),
      );
  }
}
