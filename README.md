# egui-accesskit-web

Screen readers for [egui](https://github.com/emilk/egui) apps in the
browser.

egui describes every widget it draws through [AccessKit](https://accesskit.dev):
its role, name, value and state. On the desktop, AccessKit hands that to
the system's screen reader. This crate does the same on the web: it lays
invisible elements over your app's canvas, one per widget, so screen
readers read and use your eframe app like any web page. One line to add,
and nothing changes on screen.

## Quick start

Add the plugin to your web build:

```toml
[target.'cfg(target_arch = "wasm32")'.dependencies]
egui_accesskit_web = { git = "https://github.com/QCalentier/egui-accesskit-web" }
```

and add it to egui's context when the app starts, with the canvas eframe
draws on (two lines in eframe's usual web start, as in
[eframe_template](https://github.com/emilk/eframe_template)):

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
    // ... as before
});
```

That's it. Works with egui and eframe 0.36, Rust 1.95 or later.

## What your users get

- **Every widget, named**: buttons, checkboxes, radio buttons, sliders
  with their value, combo boxes with their choice, links, images with
  their alternative text (`Image::alt_text`), text fields with their
  label. Text reads in order, in the screen reader's browse mode.
- **Text fields like any web form**: the screen reader reads the text,
  follows the caret and the selection, and typing works, input methods
  included.
- **The keyboard**: Tab, Shift+Tab and the arrows move through the app,
  and the screen reader follows; moving the screen reader to a widget
  focuses it, and activating it clicks it.
- **Structure**: headings, landmarks (main, navigation, banner,
  regions), lists, tabs, trees and dialogs, as your app describes them.
- **Messages**: statuses and alerts are spoken when they appear; a chat
  log reads its new lines.
- **The page's language** (`<html lang>`), from AccessKit's root node.

## Make your app say more

egui's widgets describe themselves. Your own widgets, and the structure
of your app, can say more through egui's AccessKit hooks
(`Response::widget_info`, `Context::accesskit_node_builder`). For
example, a title screen readers can jump to:

```rust
let title = ui.heading("Settings");
ui.ctx().accesskit_node_builder(title.id, |node| {
    node.set_role(egui::accesskit::Role::Heading);
    node.set_level(1);
    node.set_label("Settings");
});
```

Whatever your app puts in the tree, screen readers get.

## Try the demo

```sh
cargo xtask web                                  # the demo built for a browser, in dist/
python -m http.server 8765 --directory dist      # then open http://localhost:8765
```

The demo is a page of egui's own widgets. Turn a screen reader on (NVDA
is free on Windows; Narrator, VoiceOver and Orca come with their systems)
and press Tab. `cargo xtask web` needs only the `wasm32-unknown-unknown`
target (`rustup target add wasm32-unknown-unknown`).

## How it works

Each frame, the plugin hands the tree egui built to the adapter, which
keeps a mirror of it with AccessKit's own consumer and touches only what
changed: about 0.3 ms a frame for a page of a hundred widgets. Each
widget becomes an element with its ARIA role, name and state, placed
over the widget and transparent to the pointer. The page's focus follows
egui's, and keys pressed there reach egui; a text field is a real
`<input>`, kept in step with egui's text and caret.

| Crate | What it is |
|---|---|
| [`egui_accesskit_web`](crates/egui_accesskit_web) | The egui plugin (`AccessKitWeb`): the one you add. |
| [`accesskit_web`](crates/accesskit_web) | The adapter itself, with no egui in it: any toolkit drawing on a canvas with AccessKit can use it. |

## Good to know

- A word being composed with an input method (Chinese, Japanese, Korean)
  appears in the app once it is chosen.
- Tab goes round the app, from the last widget back to the first, as in
  eframe; the browser's own keys (F6, Ctrl+L) leave it.
- One plugin per canvas. Versions follow egui's: egui 0.36 today.

## Found a problem?

Please [open an issue](https://github.com/QCalentier/egui-accesskit-web/issues): something a screen reader misses,
reads twice or reads wrong, a key that doesn't do what it should, a
widget out of place, a browser or a screen reader where it doesn't work.
We take care of every issue. What helps most:

- the screen reader and the browser, with their versions (and the
  system: Windows, macOS, Linux, a phone);
- the widgets involved (a few lines of egui code, or the demo);
- what you did, what you heard, and what you expected to hear.

Pull requests are welcome too.

## Licence

MIT OR Apache-2.0, at your option, like egui and AccessKit
([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)).
