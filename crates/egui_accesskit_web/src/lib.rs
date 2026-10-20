//! egui in a browser, for screen readers: an egui [`Plugin`] wiring
//! [`accesskit_web`] into any eframe web app, so the AccessKit tree egui
//! builds (which eframe's web runner leaves unused) reaches screen readers
//! as the page's own elements.
//!
//! ```ignore
//! eframe::WebRunner::new()
//!     .start(canvas.clone(), web_options, Box::new(move |cc| {
//!         cc.egui_ctx.add_plugin(egui_accesskit_web::AccessKitWeb::new(&canvas));
//!         Ok(Box::new(MyApp::default()))
//!     }))
//!     .await?;
//! ```
//!
//! What it does each frame: turns AccessKit on (once), hands the frame's
//! tree to the adapter, and brings back what screen readers did (focusing
//! or activating an element) as egui's AccessKit action requests. While
//! one of the adapter's elements has the page's focus, eframe (which only
//! listens to its canvas) doesn't see the keyboard: the plugin takes the
//! keys from the elements and gives them to egui itself, so Tab, the
//! arrows, Space and Enter work as on the canvas.

use std::cell::RefCell;
use std::rc::Rc;

use egui::{Context, Event, FullOutput, Key, Modifiers, Plugin, RawInput};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

/// The plugin: add it to the context of an app drawing on `canvas`
/// (`ctx.add_plugin(AccessKitWeb::new(&canvas))`).
///
/// Everything it holds lives on the page's thread (the browser's objects
/// can't leave it): the plugin itself only names its place there.
pub struct AccessKitWeb {
    slot: usize,
}

/// What a plugin keeps on the page's thread.
struct Slot {
    canvas: web_sys::Element,
    running: Option<Running>,
}

/// A started plugin: its adapter, what it brings back for egui, and its
/// key listeners.
struct Running {
    adapter: accesskit_web::Adapter,
    queue: Rc<RefCell<Queue>>,
    _keys: Vec<Closure<dyn FnMut(web_sys::KeyboardEvent)>>,
}

/// What happened on the page for egui's next frame.
#[derive(Default)]
struct Queue {
    events: Vec<Event>,
    modifiers: Modifiers,
}

thread_local! {
    static SLOTS: RefCell<Vec<Slot>> = const { RefCell::new(Vec::new()) };
}

impl AccessKitWeb {
    /// The plugin for an app drawing on `canvas`.
    pub fn new(canvas: &web_sys::HtmlCanvasElement) -> Self {
        let slot = SLOTS.with(|slots| {
            let mut slots = slots.borrow_mut();
            slots.push(Slot {
                canvas: canvas.clone().into(),
                running: None,
            });
            slots.len() - 1
        });
        Self { slot }
    }

    fn with<R>(&self, f: impl FnOnce(&mut Slot) -> R) -> Option<R> {
        SLOTS.with(|slots| slots.borrow_mut().get_mut(self.slot).map(f))
    }
}

impl Plugin for AccessKitWeb {
    fn debug_name(&self) -> &'static str {
        "egui_accesskit_web"
    }

    fn setup(&mut self, ctx: &Context) {
        ctx.enable_accesskit();
        let started = self.with(|slot| start(&slot.canvas, ctx).map(|r| slot.running = Some(r)));
        if let Some(Err(e)) = started {
            log::warn!("egui_accesskit_web: no accessibility in this page: {e:?}");
        }
    }

    fn input_hook(&mut self, _ctx: &Context, input: &mut RawInput) {
        self.with(|slot| {
            let Some(running) = &slot.running else {
                return;
            };
            let mut queue = running.queue.borrow_mut();
            input.events.append(&mut queue.events);
            if running.adapter.has_focus() {
                // The keyboard is the elements': egui has it, though the
                // canvas doesn't.
                input.focused = true;
                input
                    .events
                    .retain(|event| !matches!(event, Event::WindowFocused(false)));
            }
        });
    }

    fn output_hook(&mut self, _ctx: &Context, output: &mut FullOutput) {
        let Some(update) = output.platform_output.accesskit_update.take() else {
            return;
        };
        self.with(|slot| {
            if let Some(running) = &slot.running {
                running.adapter.update(update);
            }
        });
    }
}

/// The adapter over `canvas`, what screen readers do and the keys on its
/// elements queued for `ctx`'s next frame.
fn start(canvas: &web_sys::Element, ctx: &Context) -> Result<Running, JsValue> {
    let queue = Rc::new(RefCell::new(Queue::default()));
    let adapter = accesskit_web::Adapter::new(canvas, {
        let queue = queue.clone();
        let ctx = ctx.clone();
        move |request| {
            queue
                .borrow_mut()
                .events
                .push(Event::AccessKitActionRequest(request));
            ctx.request_repaint();
        }
    })?;
    let mut keys = Vec::new();
    for (name, pressed) in [("keydown", true), ("keyup", false)] {
        let queue = queue.clone();
        let ctx = ctx.clone();
        let closure = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(
            move |event: web_sys::KeyboardEvent| {
                if key_event(&event, pressed, &mut queue.borrow_mut()) {
                    event.prevent_default();
                }
                ctx.request_repaint();
            },
        );
        adapter
            .container()
            .add_event_listener_with_callback(name, closure.as_ref().unchecked_ref())?;
        keys.push(closure);
    }
    Ok(Running {
        adapter,
        queue,
        _keys: keys,
    })
}

/// Queues a key pressed (or released) on the adapter's elements as egui's
/// events, as eframe does on its canvas; answers whether the browser
/// should leave it alone (egui moves its own focus with Tab, and the
/// arrows, Space and the like would scroll the page).
fn key_event(event: &web_sys::KeyboardEvent, pressed: bool, queue: &mut Queue) -> bool {
    let modifiers = Modifiers {
        alt: event.alt_key(),
        ctrl: event.ctrl_key(),
        shift: event.shift_key(),
        mac_cmd: event.meta_key(),
        command: event.ctrl_key() || event.meta_key(),
    };
    if queue.modifiers != modifiers {
        queue.modifiers = modifiers;
        queue.events.push(Event::ModifiersChanged(modifiers));
    }
    let name = event.key();
    if pressed
        && !modifiers.ctrl
        && !modifiers.command
        && let Some(text) = text_of(&name)
    {
        queue.events.push(Event::Text(text));
    }
    let physical_key = Key::from_name(&event.code());
    let Some(key) = Key::from_name(&name).or(physical_key) else {
        return false;
    };
    queue.events.push(Event::Key {
        key,
        physical_key,
        pressed,
        repeat: false, // egui works it out
        modifiers,
    });
    pressed && taken(key, modifiers)
}

/// The text a key with this `key` name types: a single character only
/// (the rest are names: "Enter", "ArrowLeft", "Dead").
fn text_of(name: &str) -> Option<String> {
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if !c.is_control() => Some(name.to_owned()),
        _ => None,
    }
}

/// Whether egui takes `key` from the browser: the keys moving its focus,
/// activating and scrolling, as eframe takes them on its canvas; never
/// the browser's own shortcuts (reload, copy, paste, Ctrl+Tab).
fn taken(key: Key, modifiers: Modifiers) -> bool {
    if modifiers.command {
        return false;
    }
    matches!(
        key,
        Key::Tab
            | Key::Space
            | Key::Backspace
            | Key::ArrowDown
            | Key::ArrowLeft
            | Key::ArrowRight
            | Key::ArrowUp
            | Key::Home
            | Key::End
            | Key::PageUp
            | Key::PageDown
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_single_characters_are_text() {
        assert_eq!(text_of("a").as_deref(), Some("a"));
        assert_eq!(text_of(" ").as_deref(), Some(" "));
        assert_eq!(text_of("é").as_deref(), Some("é"));
        assert_eq!(text_of("Enter"), None);
        assert_eq!(text_of("Dead"), None);
        assert_eq!(text_of(""), None);
    }

    #[test]
    fn the_browser_keeps_its_shortcuts() {
        let none = Modifiers::NONE;
        assert!(taken(Key::Tab, none));
        assert!(taken(Key::Tab, Modifiers::SHIFT));
        assert!(taken(Key::ArrowDown, none));
        assert!(!taken(Key::R, Modifiers::COMMAND));
        assert!(!taken(Key::Tab, Modifiers::COMMAND));
        assert!(!taken(Key::V, Modifiers::COMMAND));
        assert!(!taken(Key::Escape, none));
        assert!(!taken(Key::F5, none));
    }
}
