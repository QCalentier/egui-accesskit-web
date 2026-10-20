# egui-accesskit-web

Screen readers for [egui](https://github.com/emilk/egui) apps in a browser.

An eframe app on the web draws on a `<canvas>`. To a screen reader, that
is an empty picture: nothing to read, nothing to press. egui already
knows what each widget is: it builds an [AccessKit](https://accesskit.dev)
tree of every frame (each widget's role, name, value and states). On the
desktop, AccessKit's adapters hand that tree to Windows, macOS and Linux.
On the web, eframe leaves it unused, and AccessKit has no adapter for
browsers.

This repository is that adapter, and an egui plugin wiring it into any
eframe web app in one line. It mirrors the tree into elements laid over
the canvas, invisible but present: one element per widget, with its ARIA
role, name, states and place. The browser's own accessibility then gives
the app to screen readers (NVDA, JAWS, Narrator, VoiceOver, TalkBack,
Orca) like any web page.

> **Status: early.** It works, and it has been checked through a
> browser's own accessibility tree (below), but not yet with every
> screen reader, and some things are missing (see
> [What it doesn't do yet](#what-it-doesnt-do-yet)). It isn't on
> crates.io yet; its names may change. Reports, above all from screen
> reader users, are very welcome.

## Use it in an eframe app

Add the plugin for the web build only:

```toml
[target.'cfg(target_arch = "wasm32")'.dependencies]
egui_accesskit_web = { git = "https://github.com/QCalentier/egui-accesskit-web" }
```

and add two lines to eframe's usual web start (as in
[eframe_template](https://github.com/emilk/eframe_template)): make the
plugin with the canvas eframe draws on, and add it to egui's context:

```rust
wasm_bindgen_futures::spawn_local(async {
    let canvas: web_sys::HtmlCanvasElement = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("the_canvas_id"))
        .and_then(|e| e.dyn_into().ok()) // wasm_bindgen::JsCast
        .expect("the page's canvas");
    let plugin = egui_accesskit_web::AccessKitWeb::new(&canvas); // here
    let started = eframe::WebRunner::new()
        .start(
            canvas,
            eframe::WebOptions::default(),
            Box::new(move |cc| {
                cc.egui_ctx.add_plugin(plugin); // and here
                Ok(Box::new(MyApp::new(cc)))
            }),
        )
        .await;
    // ... as before: say if it didn't start
});
```

That's all: the plugin turns AccessKit on and does the rest. Nothing
changes on screen, nor for mouse and touch users.

**Needs** egui and eframe 0.36 (AccessKit 0.24), Rust 1.95 or later.
Either renderer (glow or wgpu): the plugin doesn't draw.

## What a screen reader gets

- **Every widget egui describes**, with its role and name: buttons,
  checkboxes, radio buttons, sliders (with their value and range), combo
  boxes (with the option chosen), text fields (named by their label),
  links, images with their alternative text (`Image::alt_text`). Text is readable in the
  screen reader's browse mode, in order.
- **States**: checked, selected, expanded, disabled, read-only, busy, a
  heading's level, a value's text.
- **Structure**, when the app describes it: headings, landmarks (main,
  navigation, banner, regions), lists, tabs, trees, dialogs (modal).
- **Messages**: a node marked live (a status, an alert) is said when it
  appears or changes, politely or at once; a log (a chat) says its new
  lines.
- **Focus, both ways**: Tab, Shift+Tab and the arrows move egui's focus,
  and the screen reader follows it; a screen reader moving to a widget
  focuses it, and activating it (Enter, Space, a double tap) clicks it.
- **The page's language** (`<html lang>`), when the app gives AccessKit's
  root node one.

egui's own widgets say what egui says about them; an app (or a library
of widgets on egui) can say more through egui's AccessKit hooks
(`Response::widget_info`, `Context::accesskit_node_builder`). The
screen reader reads whatever the tree holds.

## How it works

- **The tree in.** Each frame, the plugin takes the tree update egui built
  and gives it to the adapter. The adapter keeps a mirror of the tree
  with AccessKit's own consumer (the crate AccessKit's other adapters
  use), which says which nodes were added, changed or removed: only their
  elements are touched.
- **Elements out.** Each node is a `<div>` with its ARIA role (most of
  AccessKit's roles are ARIA's), its name as its text (text, buttons,
  headings) or its `aria-label`, its states as `aria-*` attributes, its
  description, and its place: positioned over the widget it stands for,
  so a screen reader's highlight lands on the widget and touch exploration
  finds widgets where they are drawn. They are transparent and let the
  pointer through, so nothing changes on screen.
- **Messages** go through two live regions the adapter keeps (polite and
  assertive), which works for a message that has just appeared, unlike
  a live region added with it.
- **Focus.** egui keeps its own focus; the page's focus follows it onto the
  focused widget's element, so the screen reader says what was reached.
  It follows from the adapter's elements, from nowhere, and from the
  canvas (or eframe's text input) after a key, never after a pointer
  press: a mouse user's focus stays on the canvas, where eframe expects
  it.
- **Keys.** eframe only listens to its canvas, so while one of the
  adapter's elements has the focus, the plugin hands its keys to egui
  itself, and keeps from the browser the keys egui uses there (Tab, the
  arrows, Space), as eframe does, never the browser's own shortcuts.
- **Text fields.** On a field one types in, the page's focus goes to
  eframe's own hidden `<input>`, named after the field, so typing works
  as it always did, with input methods and a phone's keyboard.

The two crates:

| Crate | What it is |
|---|---|
| [`accesskit_web`](crates/accesskit_web) | The adapter: an AccessKit tree in, elements out, a screen reader's actions back as AccessKit's action requests. No egui in it: any toolkit drawing on a canvas with AccessKit can use it (`Adapter::new(canvas, on_action)`, then `update(tree_update)` each frame; keys pressed on its elements reach `Adapter::container()`). |
| [`egui_accesskit_web`](crates/egui_accesskit_web) | The egui plugin (`AccessKitWeb`), wiring the adapter into eframe. |

## What it doesn't do yet

Honestly, so you know before you rely on it:

- **Real screen readers in a browser are the next test.** So far it has
  been checked through the browser's own accessibility tree (headless
  Edge driven through the DevTools protocol, which is what screen readers
  read): roles, names, states, focus, keyboard, a screen reader's click,
  live messages. Not yet systematically with NVDA, JAWS, VoiceOver or
  TalkBack, nor in Firefox or Safari.
- **A text field's content.** Fields are named and typing works, but a
  screen reader can't read back what is in a field, nor move a caret
  through it: eframe's hidden input stays empty. Mirroring the field's
  text and caret into it is next.
- **egui's own gaps.** `ui.heading()` is text to AccessKit, not a
  heading; a collapsing header doesn't say whether it is open. These are
  egui's to describe (or an app's, through the hooks above).
- **Tab stays in the app**, wrapping from the last widget to the first,
  as it does in eframe on the canvas. The browser's own keys (F6, Ctrl+L)
  leave it.
- **Touch screen readers** (VoiceOver on iOS, TalkBack) get elements where
  the widgets are, but haven't been tried.
- **One canvas, one viewport**, as eframe has on the web; a page with
  several apps gives each its own plugin. It relies on one detail of
  eframe's: its hidden text input is the canvas's next sibling (if a
  future eframe moves it, fields lose their name, nothing else).
- **A canvas in a scrolling page** has its elements placed again at each
  egui frame; between frames, they can lag a scroll. A canvas filling its
  page (the usual eframe setup) doesn't move.
- **Versions** follow egui's: each egui release that changes AccessKit's
  version needs a release of this.

What it costs: about 0.3 ms a frame for a page of 40 to 120 widgets
(measured in headless Edge, rendering in software), and 0.2 MB of
WebAssembly.

## Try it

```sh
cargo xtask web                                  # the demo built for a browser, in dist/
python -m http.server 8765 --directory dist      # then open http://localhost:8765
```

The demo is egui's own widgets: a button, a checkbox, a slider, radio
buttons, a combo box, a text field, a collapsing section. Turn a screen
reader on (NVDA is free on Windows; Narrator, VoiceOver and Orca come with
their systems) and press Tab. `cargo xtask web` needs nothing installed
but the `wasm32-unknown-unknown` target (`rustup target add
wasm32-unknown-unknown`).

`cargo test --workspace` tests the mirror without a browser, on a DOM in
memory.

## Licence

MIT OR Apache-2.0, at your option, like egui and AccessKit
([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)).
