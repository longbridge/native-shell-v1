// Components: every control the shell renders, each shown where it belongs.
//
// Not a dump of widgets: each page is a small, believable scenario (a post
// composer, an address form, checkout preferences, an upload, a booking, a
// shared document) that uses its components for their meaning and shows their
// states: rest, pressed, disabled, loading, error. The index is an inset
// grouped list under a collapsing large title; pages push natively and return
// with the back button or the edge swipe.

import { View } from "native-shell";
import ButtonsPage from "./pages/buttons.js";
import FieldsPage from "./pages/fields.js";
import SelectionPage from "./pages/selection.js";
import ProgressPage from "./pages/progress.js";
import PickersPage from "./pages/pickers.js";
import FormsPage from "./pages/forms.js";
import OverlaysPage from "./pages/overlays.js";
import StatusPage from "./pages/status.js";
import ChartsPage from "./pages/charts.js";
import OrderPage from "./pages/order.js";
import FilesPage from "./pages/files.js";
import BookingPage from "./pages/booking.js";
import PhotosPage from "./pages/photos.js";
import NotificationsPage from "./pages/notifications.js";
import { row, scrollScreen, section, tile } from "./ui.js";
/** @import { Color } from "native-shell" */

/** @typedef {{ id: string, icon: "pointer" | "text" | "toggle" | "sliders" | "calendar" | "file" | "layers" | "status" | "chart" | "receipt" | "folder" | "photo" | "bell", tint: "primary" | "success" | "warning" | "info" | "destructive", title: string, detail: string, page: import("native-shell").ViewClass }} Entry */

/** @type {{ header: string, pages: Entry[] }[]} */
const SECTIONS = [
  {
    header: "Actions",
    pages: [{ id: "buttons", icon: "pointer", tint: "primary", title: "Buttons", detail: "A post composer", page: ButtonsPage }],
  },
  {
    header: "Input",
    pages: [
      { id: "fields", icon: "text", tint: "info", title: "Text Fields", detail: "A shipping address form", page: FieldsPage },
      { id: "selection", icon: "toggle", tint: "success", title: "Selection", detail: "Checkout preferences", page: SelectionPage },
      {
        id: "progress",
        icon: "sliders",
        tint: "warning",
        title: "Sliders and Progress",
        detail: "A slideshow and an upload",
        page: ProgressPage,
      },
      { id: "pickers", icon: "calendar", tint: "destructive", title: "Pickers", detail: "A table booking", page: PickersPage },
      { id: "forms", icon: "file", tint: "primary", title: "Forms", detail: "A flower delivery checkout", page: FormsPage },
    ],
  },
  {
    header: "Data",
    pages: [
      { id: "charts", icon: "chart", tint: "info", title: "Charts", detail: "A health and money dashboard", page: ChartsPage },
      { id: "order", icon: "receipt", tint: "success", title: "Description List", detail: "An order's details", page: OrderPage },
      { id: "files", icon: "folder", tint: "primary", title: "Tree", detail: "A project's files", page: FilesPage },
      { id: "booking", icon: "calendar", tint: "destructive", title: "Calendar", detail: "A cabin booking", page: BookingPage },
      { id: "photos", icon: "photo", tint: "warning", title: "Carousel", detail: "A trip's photo album", page: PhotosPage },
      { id: "notifications", icon: "bell", tint: "destructive", title: "Settings", detail: "Notification preferences", page: NotificationsPage },
    ],
  },
  {
    header: "Feedback",
    pages: [
      { id: "overlays", icon: "layers", tint: "primary", title: "Overlays", detail: "Sheet, dialog, toast and alert", page: OverlaysPage },
      { id: "status", icon: "status", tint: "warning", title: "Status", detail: "Order banners, badges and empty states", page: StatusPage },
    ],
  },
];

export default class Components extends View {
  init() {
    this.collapsed = false;
  }

  render(cx) {
    const t = cx.theme();
    return scrollScreen(cx, {
      id: "index",
      title: "Components",
      subtitle: "Each control in a real task, with its states",
      background: t.grouped_background,
      collapsed: this.collapsed,
      on_collapse: (collapsed, cx) => {
        this.collapsed = collapsed;
        cx.notify();
      },
      children: SECTIONS.map((s) =>
        section(cx, {
          header: s.header,
          inset: 60,
          rows: s.pages.map((p) =>
            row(cx, {
              id: `open-${p.id}`,
              leading: tile(cx, p.icon, /** @type {Color} */ (t[p.tint])),
              label: p.title,
              detail: p.detail,
              chevron: true,
              on_click: (_e, cx) => cx.navigator.push(p.page, {}, { title: p.title }),
            }),
          ),
        }),
      ),
    });
  }
}
