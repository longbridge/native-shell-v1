// A carousel, as a trip's photo album: swipe between photos (or use the
// arrows and dots), read where each was taken under it, and star the ones to
// keep. Favorites is the same carousel over the starred photos, with an empty
// state that says how to fill it.

import { View, div, h_flex, v_flex, image, Button } from "native-shell";
import {
  Carousel, CarouselContent, CarouselItem, CarouselNext, CarouselPagination, CarouselPaginationItem, CarouselPrevious,
  CarouselState, Icon, Tab, TabBar,
} from "gpui-component";
import { label, page, textButton } from "../ui.js";

const PHOTOS = [
  { id: "lake-dawn", title: "First light on the lake", detail: "Saturday, 6:52 AM" },
  { id: "canoe-morning", title: "Out in the canoe", detail: "Saturday, 9:15 AM" },
  { id: "forest-trail", title: "The ridge trail", detail: "Saturday, 1:40 PM" },
  { id: "cabin-dusk", title: "Back at the cabin", detail: "Saturday, 6:30 PM" },
  { id: "night-sky", title: "Stars over the water", detail: "Sunday, 11:58 PM" },
];

export default class PhotosPage extends View {
  init() {
    this.tab = 0;
    this.favorites = new Set(["cabin-dusk"]);
    this.all = CarouselState(PHOTOS.length);
    this.starred = CarouselState(this.favorites.size);
  }

  shown() {
    return this.tab === 0 ? PHOTOS : PHOTOS.filter((p) => this.favorites.has(p.id));
  }

  /** @param {string} id @param {import("native-shell").Context} cx */
  toggleFavorite(id, cx) {
    if (this.favorites.has(id)) this.favorites.delete(id);
    else this.favorites.add(id);
    this.starred.set_item_count(this.favorites.size);
    cx.haptic("selection");
    cx.notify();
  }

  render(cx) {
    const t = cx.theme();
    const photos = this.shown();
    const state = this.tab === 0 ? this.all : this.starred;
    const current = photos[state.selected_index() ?? 0];
    return page(cx, {
      title: "Photos",
      intro: "A weekend at Fernhill. Swipe between photos; star the ones to keep.",
      children: [
        new TabBar("album")
          .variant("pill")
          .test_id("album")
          .selected_index(this.tab)
          .on_change((i, cx) => {
            this.tab = i;
            cx.notify();
          })
          .child(new Tab().label(`All ${PHOTOS.length}`).test_id("album-all").accessibility_label(`All photos, ${PHOTOS.length}`))
          .child(new Tab().label(`Favorites ${this.favorites.size}`).test_id("album-favorites").accessibility_label(`Favorites, ${this.favorites.size}`)),
        current
          ? v_flex()
              .gap_3()
              .child(
                div()
                  .rounded(t.radius.lg)
                  .overflow_hidden()
                  .child(
                    new Carousel(this.tab === 0 ? "all" : "favorites", state)
                      .test_id("carousel")
                      .accessibility_label(this.tab === 0 ? "Album" : "Favorites")
                      .on_change((_i, cx) => cx.notify())
                      .child(
                        new CarouselContent(state).test_id("slides").children(
                          photos.map((p, i) =>
                            new CarouselItem(p.id, i, state)
                              .accessibility_label(p.title)
                              .child(image(`assets/photos/${p.id}.jpg`).w_full().aspect_ratio(4 / 3).object_fit("cover")),
                          ),
                        ),
                      )
                      .child(new CarouselPrevious(state).test_id("previous"))
                      .child(new CarouselNext(state).test_id("next"))
                      .child(
                        new CarouselPagination().children(photos.map((p, i) => new CarouselPaginationItem(`dot-${p.id}`, i, state))),
                      ),
                  ),
              )
              .child(
                h_flex()
                  .gap_3()
                  .px_1()
                  .child(
                    v_flex()
                      .flex_1()
                      .min_w_0()
                      .child(label(cx, "headline", t.foreground, current.title).test_id("caption"))
                      .child(label(cx, "subheadline", t.muted_foreground, current.detail)),
                  )
                  .child(this.star(cx, current.id)),
              )
          : v_flex()
              .test_id("no-favorites")
              .items_center()
              .gap_2()
              .py_12()
              .px_8()
              .child(new Icon("icons/star.svg").size(44).color(t.muted_foreground))
              .child(label(cx, "title3", t.foreground, "No Favorites Yet").font_semibold())
              .child(label(cx, "subheadline", t.muted_foreground, "Star a photo to keep it here.").text_center())
              .child(
                textButton(cx, "show-all", "Show All Photos", {
                  on_click: (_e, cx) => {
                    this.tab = 0;
                    cx.notify();
                  },
                }),
              ),
      ],
    });
  }

  /** @param {import("native-shell").Context} cx @param {string} id */
  star(cx, id) {
    const t = cx.theme();
    const on = this.favorites.has(id);
    return Button.new("favorite")
      .test_id("favorite")
      .accessibility_label(on ? "Remove from Favorites" : "Add to Favorites")
      .pressed(on)
      .size_11()
      .flex()
      .items_center()
      .justify_center()
      .rounded_full()
      .active((s) => s.bg(t.secondary))
      .on_click((_e, cx) => this.toggleFavorite(id, cx))
      .child(new Icon(on ? "icons/star-fill.svg" : "icons/star.svg").size(24).color(on ? t.warning : t.primary));
  }
}
