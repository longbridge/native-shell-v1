// "gpui-shell": gpui-shell registers this specifier with no exports (its
// names live in "gpui-kit" / "gpui-base"), so importing a name from it fails at
// link time there and here alike; a bare or empty import loads.

export {};
