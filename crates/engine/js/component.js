// "gpui-component": the parts of gpui-component's catalog that exist on a
// phone. Each area lives in its own module under gpui_component/; this one
// re-exports them. A title bar is the screen's header row, under the status bar.

import { windowInfo } from "nsh:core";
import { Element } from "nsh:elements";

export * from "nsh:component/feedback";
export * from "nsh:component/forms";
export * from "nsh:component/data";
export * from "nsh:component/charts";
export * from "nsh:component/chat";

export class TitleBar extends Element {
  constructor() {
    super("view");
    const top = windowInfo().safe_area.top;
    this._preset([["flex"], ["flex_row"], ["items_center"], ["w_full"], ["h", 44 + top], ["pt", top], ["px_3"], ["gap_2"]]);
    this._P().role = "header";
  }
}
