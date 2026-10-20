# egui-accesskit-web

Screen readers for egui in a browser.

An eframe app on the web draws on a `<canvas>`: a screen reader finds an
empty picture there. egui builds an [AccessKit](https://accesskit.dev) tree
of every frame (each widget's role, name, value and states), and on the
desktop AccessKit's adapters hand it to Windows, macOS and Linux; on the
web, eframe leaves it unused, and AccessKit has no adapter for browsers.
This repository is that adapter, and the plugin wiring it into eframe.

It mirrors the tree into elements laid over the canvas, invisible but
present: each node an element with its ARIA role, name, states and place.
The browser's own accessibility then gives the app to screen readers like
any page: NVDA, JAWS, Narrator, VoiceOver, TalkBack, Orca; browse mode,
headings, landmarks, forms.

> **Status: early, private.** Built for [SmartBridge](https://github.com/QCalentier/SmartBridge-Framework)'s
> web builds, to be shared with egui's users once it is right.

## Two crates

| Crate | What it is |
|---|---|
| [`accesskit_web`](crates/accesskit_web) | The adapter: AccessKit's tree in, elements out; the browser's focus and a screen reader's actions back as AccessKit's action requests. No egui in it: any toolkit on AccessKit can use it in a browser. |
| [`egui_accesskit_web`](crates/egui_accesskit_web) | The egui glue: a `Plugin` adding it to any eframe web app in one line. |

```rust
eframe::WebRunner::new()
    .start(canvas.clone(), web_options, Box::new(move |cc| {
        cc.egui_ctx.add_plugin(egui_accesskit_web::AccessKitWeb::new(&canvas));
        Ok(Box::new(MyApp::default()))
    }))
    .await?;
```

## How it works

- **The tree in.** Each frame, the plugin takes the tree update egui
  built (it turns AccessKit on) and gives it to the adapter, which keeps
  a mirror with AccessKit's own consumer (the crate AccessKit's adapters
  use): it says which nodes were added, changed or removed, and only their
  elements are touched.
- **The elements out.** Each node is a `<div>` with its ARIA role (most
  of AccessKit's roles are ARIA's), its name as its text (text, buttons,
  headings) or `aria-label`, its states (`aria-checked`, `aria-selected`,
  `aria-expanded`, `aria-disabled`, `aria-valuenow`, `aria-level`, ...),
  its description, placed absolutely over what it stands for: a screen
  reader's highlight lands on the widget, and touch exploration on a phone
  finds widgets where they are drawn. Transparent and letting the pointer
  through, they change nothing on screen.
- **Messages.** Statuses, alerts and other live nodes are said through two
  live regions the adapter keeps (polite and assertive), which works for
  a message that has just appeared; a log (a chat) is a live region
  itself, its new lines said as they come.
- **Focus, both ways.** egui keeps its focus: Tab and the arrows move it,
  and the page's focus follows onto the focused widget's element, so a
  screen reader says what was reached. It follows from the elements, from
  nowhere, and from the canvas after a key; never after a pointer press,
  so a mouse user's focus stays on the canvas (and egui's own text input
  works as before). A screen reader moving to an element focuses its
  widget; activating it (Enter, Space, a double tap) clicks it.
- **Keys.** eframe only listens to its canvas: while an element has the
  focus, the plugin hands its keys to egui itself (and keeps from the
  browser those egui uses, as eframe does: Tab, the arrows, Space; never
  the browser's shortcuts).
- **The page.** `<html lang>` follows the language the tree's root says,
  when it says one.

## Try it

```sh
cargo xtask web                                  # dist/: the demo built for a browser
python -m http.server 8765 --directory dist      # then open http://localhost:8765
```

Then a screen reader: NVDA (free) or Narrator on Windows, VoiceOver on a
Mac or an iPhone, TalkBack on Android, Orca on Linux. The demo is egui's
own widgets: a button, a checkbox, a slider, radio buttons, a combo box, a
text field, a collapsing section.

`cargo test --workspace` tests the mirror without a browser (a DOM in
memory). In a browser, headless Chrome or Edge driven through the DevTools
protocol reads the page's accessibility tree
(`Accessibility.getFullAXTree`) as a screen reader gets it.

## Not yet

- **Text fields** are a `div` with the textbox role: named, and typing
  works (keys go to egui), but a screen reader can't move a caret in it,
  and a phone's keyboard doesn't open from it. A real `<input>` is next.
- **egui's own widgets** say what egui says: `ui.heading()` is text, not a
  heading (no AccessKit role for it); a collapsing header doesn't say
  whether it is open. Toolkits built on egui can say more (SmartBridge's
  components do).
- **Tab stays in the app**, wrapping from the last widget to the first, as
  in eframe on the canvas: the browser's own keys (F6, Ctrl+L) leave it.

## Licence

MIT OR Apache-2.0, at your option, like egui and AccessKit
([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)).
