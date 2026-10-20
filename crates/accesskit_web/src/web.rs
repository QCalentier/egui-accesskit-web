//! The adapter in a page: the mirror's elements in a container laid over
//! the canvas, the focus kept in step both ways, and what screen readers
//! do with the elements sent back as AccessKit's action requests.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use accesskit::{Action, ActionRequest, Rect, TreeUpdate};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

use crate::dom::{Dom, Tag};
use crate::field;
use crate::mirror::{FieldState, Focus, Mirror};

/// The class of every container, which the adapter's style sheet styles.
const CLASS: &str = "accesskit-web";

/// The `id` of the style sheet, added to the page once.
const STYLE_ID: &str = "accesskit-web-style";

/// The style of the containers and their elements: invisible but there
/// for screen readers (not `display: none` nor `visibility: hidden`, which
/// hide them from screen readers too), letting the pointer through to the
/// canvas, each placed over what it stands for.
const STYLE: &str = "
.accesskit-web { position: fixed; overflow: hidden; pointer-events: none; }
.accesskit-web, .accesskit-web * {
  color: transparent; background: transparent; border: 0; margin: 0; padding: 0;
  outline: none; box-shadow: none; caret-color: transparent; user-select: none;
  font: 1px/1 sans-serif; white-space: nowrap; overflow: hidden; box-sizing: border-box;
}
.accesskit-web [data-placed] { position: absolute; }
.accesskit-web input, .accesskit-web textarea {
  font-size: 16px; resize: none; appearance: none; user-select: text;
}
";

/// An AccessKit adapter for a page: the tree a toolkit draws on a
/// `<canvas>` as elements a screen reader reads, follows and acts on.
///
/// Make it with the canvas, give it each [`TreeUpdate`] the toolkit
/// builds, and act on the [`ActionRequest`]s it sends back (a screen
/// reader focusing or clicking an element). The toolkit keeps its own
/// focus: Tab and the arrows move it, and the elements follow; while one
/// of them has the page's focus, keys go to it, so the toolkit should
/// take them from [`container`](Self::container) (the egui glue does).
///
/// A field one types in is a real `<input>` (or `<textarea>`) holding the
/// field's text and caret: the browser edits it (typing, pasting, input
/// methods, a phone's keyboard), and each edit comes back as a
/// `SetTextSelection` (when it wasn't at the selection) then a
/// `ReplaceSelectedText` request; keys that move the caret or delete
/// (arrows, Home, End, Backspace, Delete) are the toolkit's to handle, the
/// field following its caret. A toolkit with a text input of its own
/// (eframe's hidden `<input>`) gives it with
/// [`set_text_input`](Self::set_text_input).
pub struct Adapter {
    shared: Rc<Shared>,
    anchor: web_sys::Element,
    language: RefCell<Option<String>>,
    /// The toolkit's own text input, and what it last said: hidden from
    /// screen readers or not, its name.
    text_input: Option<web_sys::HtmlElement>,
    said: RefCell<Option<(bool, Option<String>)>>,
    _listeners: Vec<Closure<dyn FnMut(web_sys::Event)>>,
}

/// What the adapter and its event listeners share.
struct Shared {
    mirror: RefCell<Mirror<PageDom>>,
    container: web_sys::HtmlElement,
    on_action: RefCell<Box<dyn FnMut(ActionRequest)>>,
    /// Set while the adapter moves the focus itself, so its own focus
    /// events aren't taken for a screen reader's.
    moving: Cell<bool>,
    /// Whether the last thing done on the canvas was a key (else a pointer
    /// press): the focus then follows the toolkit's off the canvas.
    keyed: Cell<bool>,
    /// The field being typed in, as its element was last made to show it.
    field: RefCell<Option<FieldSync>>,
    /// Set when the field was edited or its caret moved in the page: its
    /// element is shown the toolkit's text again after the toolkit's next
    /// frame (which may have refused the edit), even if unchanged.
    touched: Cell<bool>,
}

/// A field's element, as last made to show the toolkit's field: its text
/// and selection, in UTF-16 code units.
struct FieldSync {
    key: u128,
    text: Vec<u16>,
    start: usize,
    end: usize,
    /// Edited before the toolkit focused it: the edit waits for its focus.
    waiting: bool,
}

impl Adapter {
    /// An adapter for the toolkit drawing on `canvas` (any element, the
    /// elements laid over it), sending what a screen reader does to
    /// `on_action`.
    ///
    /// Its container goes just before `canvas`, so it comes first in the
    /// page's reading order.
    pub fn new(
        canvas: &web_sys::Element,
        on_action: impl FnMut(ActionRequest) + 'static,
    ) -> Result<Self, JsValue> {
        let document = canvas
            .owner_document()
            .ok_or_else(|| JsValue::from_str("the canvas isn't in a document"))?;
        add_style(&document)?;
        let container: web_sys::HtmlElement = document.create_element("div")?.dyn_into()?;
        let prefix = format!("{CLASS}-{}", next_number());
        container.set_id(&prefix);
        container.set_class_name(CLASS);
        // The way into the toolkit with Tab: it hands the focus on to the
        // toolkit's focused element (or first one).
        container.set_tab_index(0);
        let polite = announcer(&document, "polite")?;
        let assertive = announcer(&document, "assertive")?;
        container.append_child(&polite)?;
        container.append_child(&assertive)?;
        match canvas.parent_node() {
            Some(parent) => parent.insert_before(&container, Some(canvas))?,
            None => document
                .body()
                .ok_or_else(|| JsValue::from_str("the page has no body"))?
                .append_child(&container)?,
        };

        let dom = PageDom {
            document,
            container: container.clone(),
            prefix,
            polite,
            assertive,
            said: [String::new(), String::new()],
        };
        let shared = Rc::new(Shared {
            mirror: RefCell::new(Mirror::new(dom)),
            container,
            on_action: RefCell::new(Box::new(on_action)),
            moving: Cell::new(false),
            keyed: Cell::new(false),
            field: RefCell::new(None),
            touched: Cell::new(false),
        });
        let mut adapter = Self {
            shared: shared.clone(),
            anchor: canvas.clone(),
            language: RefCell::new(None),
            text_input: None,
            said: RefCell::new(None),
            _listeners: Vec::new(),
        };
        adapter.listen("focusin", {
            let shared = shared.clone();
            move |event| shared.focused(&event)
        })?;
        adapter.listen("click", {
            let shared = shared.clone();
            move |event| shared.clicked(&event)
        })?;
        for event in ["input", "compositionend"] {
            adapter.listen(event, {
                let shared = shared.clone();
                move |event| shared.edited(&event)
            })?;
        }
        if let Some(document) = canvas.owner_document() {
            adapter.listen_on(&document, "selectionchange", {
                let shared = shared.clone();
                move |_| shared.caret_moved()
            })?;
        }
        adapter.listen_on(canvas, "keydown", {
            let shared = shared.clone();
            move |_| shared.keyed.set(true)
        })?;
        adapter.listen_on(canvas, "pointerdown", {
            let shared = shared.clone();
            move |_| shared.keyed.set(false)
        })?;
        Ok(adapter)
    }

    /// Says the toolkit types through `input`, an input of its own beside
    /// the canvas (eframe's hidden `<input>`, where a pointer user's typing
    /// goes): it is named after the focused field, and from it, after a
    /// key, the focus follows the toolkit's as from the canvas.
    pub fn set_text_input(&mut self, input: &web_sys::HtmlElement) -> Result<(), JsValue> {
        self.listen_on(input, "keydown", {
            let shared = self.shared.clone();
            move |_| shared.keyed.set(true)
        })?;
        self.listen_on(input, "pointerdown", {
            let shared = self.shared.clone();
            move |_| shared.keyed.set(false)
        })?;
        self.text_input = Some(input.clone());
        Ok(())
    }

    /// Mirrors `update` (what the toolkit built this frame) into the
    /// elements, and moves the page's focus where the toolkit's went
    /// while it is with the elements, nowhere, or on the canvas after a
    /// key (Tab): a pointer user's focus, on the canvas or a text field of
    /// the toolkit's, stays there.
    pub fn update(&self, update: TreeUpdate) {
        self.place();
        let scale = web_sys::window().map_or(1.0, |w| w.device_pixel_ratio());
        let moved = self.shared.mirror.borrow_mut().update(update, scale);
        if let Some(input) = &self.text_input {
            // Where a pointer user types: named after the field while it
            // has the focus, else hidden from screen readers (a second,
            // empty field otherwise, beside the field's own element).
            let hidden = !self.is_active(input);
            let name = self
                .shared
                .mirror
                .borrow()
                .focused_field()
                .filter(|name| !name.is_empty());
            let said = Some((hidden, name));
            if *self.said.borrow() != said {
                if let Some((hidden, name)) = &said {
                    match hidden {
                        true => input.set_attribute("aria-hidden", "true").ok(),
                        false => input.remove_attribute("aria-hidden").ok(),
                    };
                    match name {
                        Some(name) => input.set_attribute("aria-label", name).ok(),
                        None => input.remove_attribute("aria-label").ok(),
                    };
                }
                *self.said.borrow_mut() = said;
            }
        }
        if let Some(focus) = moved
            && self.follows_focus()
        {
            match focus {
                Focus::To(element) => self.shared.focus(&element),
                Focus::Root => self.shared.focus(&self.shared.container),
            }
        }
        // The field being typed in shows the toolkit's text and caret.
        let typing = self.shared.mirror.borrow().focused_field_state();
        if let Some(state) = typing
            && self.is_active(&state.element)
        {
            self.shared.show(state);
        }
        let language = self.shared.mirror.borrow().language();
        if *self.language.borrow() != language {
            if let (Some(language), Some(html)) = (
                &language,
                self.anchor
                    .owner_document()
                    .and_then(|d| d.document_element()),
            ) {
                html.set_attribute("lang", language).ok();
            }
            *self.language.borrow_mut() = language;
        }
    }

    /// The element holding the adapter's elements: keys pressed while one
    /// of them has the focus reach it.
    pub fn container(&self) -> &web_sys::HtmlElement {
        &self.shared.container
    }

    /// Whether the page's focus is on the adapter's elements (so keys go
    /// to them, not to the canvas).
    pub fn has_focus(&self) -> bool {
        let active = self
            .anchor
            .owner_document()
            .and_then(|d| d.active_element());
        active.is_some_and(|active| self.shared.container.contains(Some(&active)))
    }

    /// Whether `element` has the page's focus.
    fn is_active(&self, element: &web_sys::HtmlElement) -> bool {
        let active = self
            .anchor
            .owner_document()
            .and_then(|d| d.active_element());
        active.is_some_and(|active| active == *element.unchecked_ref::<web_sys::Element>())
    }

    /// Whether the page's focus should follow the toolkit's: it is on the
    /// elements already, nowhere, or on the canvas or the toolkit's text
    /// input after a key.
    fn follows_focus(&self) -> bool {
        let document = self.anchor.owner_document();
        let active = document.as_ref().and_then(|d| d.active_element());
        let body = document.as_ref().and_then(|d| d.body());
        match active {
            None => true,
            Some(active) => {
                body.is_some_and(|body| active == *body.unchecked_ref::<web_sys::Element>())
                    || self.shared.container.contains(Some(&active))
                    || ((active == self.anchor
                        || self
                            .text_input
                            .as_ref()
                            .is_some_and(|input| active == **input))
                        && self.shared.keyed.get())
            }
        }
    }

    /// Lays the container over the canvas, where it is now.
    fn place(&self) {
        let r = self.anchor.get_bounding_client_rect();
        let style = self.shared.container.style();
        style.set_css_text(&format!(
            "left:{}px;top:{}px;width:{}px;height:{}px",
            r.left(),
            r.top(),
            r.width(),
            r.height()
        ));
    }

    fn listen(
        &mut self,
        event: &str,
        handle: impl FnMut(web_sys::Event) + 'static,
    ) -> Result<(), JsValue> {
        let container = self.shared.container.clone();
        self.listen_on(&container, event, handle)
    }

    fn listen_on(
        &mut self,
        target: &web_sys::EventTarget,
        event: &str,
        handle: impl FnMut(web_sys::Event) + 'static,
    ) -> Result<(), JsValue> {
        let closure = Closure::<dyn FnMut(web_sys::Event)>::new(handle);
        target.add_event_listener_with_callback(event, closure.as_ref().unchecked_ref())?;
        self._listeners.push(closure);
        Ok(())
    }
}

impl Drop for Adapter {
    fn drop(&mut self) {
        self.shared.container.remove();
    }
}

impl Shared {
    /// Moves the page's focus to `element`, not scrolling the page.
    fn focus(&self, element: &web_sys::HtmlElement) {
        self.moving.set(true);
        let options = web_sys::FocusOptions::new();
        options.set_prevent_scroll(true);
        element.focus_with_options(&options).ok();
        self.moving.set(false);
    }

    /// The page's focus moved into the container: a screen reader (or Tab)
    /// put it on an element, whose node the toolkit then focuses; or on
    /// the container itself, handed on to the toolkit's focus.
    fn focused(&self, event: &web_sys::Event) {
        if self.moving.get() {
            return;
        }
        let Some(target) = event
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
        else {
            return;
        };
        let request = {
            let mirror = self.mirror.borrow();
            if target == *self.container.unchecked_ref::<web_sys::Element>() {
                match mirror.focused() {
                    Some(element) => {
                        drop(mirror);
                        self.focus(&element);
                        return;
                    }
                    None => mirror.first_focusable(),
                }
            } else {
                match self.key(&target) {
                    Some(key) if !mirror.has_focus(key) => mirror.target(key),
                    _ => None,
                }
            }
        };
        if let Some((node, tree)) = request {
            self.act(Action::Focus, node, tree);
        }
    }

    /// A screen reader activated an element: its node (or the nearest
    /// ancestor taking clicks) is clicked.
    fn clicked(&self, event: &web_sys::Event) {
        let Some(key) = event
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
            .and_then(|t| self.key(&t))
        else {
            return;
        };
        let target = self.mirror.borrow().clickable(key);
        if let Some((node, tree)) = target {
            event.prevent_default();
            self.act(Action::Click, node, tree);
        }
    }

    fn act(&self, action: Action, target_node: accesskit::NodeId, target_tree: accesskit::TreeId) {
        (self.on_action.borrow_mut())(ActionRequest {
            action,
            target_tree,
            target_node,
            data: None,
        });
    }

    /// Shows the toolkit's field in its element, when it differs from what
    /// the element was last made to show (or the page touched it); or,
    /// for a field edited before the toolkit focused it, sends the edit
    /// now that it has.
    fn show(&self, state: FieldState<web_sys::HtmlElement>) {
        let waiting = self
            .field
            .borrow()
            .as_ref()
            .is_some_and(|f| f.key == state.key && f.waiting);
        if waiting {
            self.send_edit(state.key, state.element.unchecked_ref());
            return;
        }
        let text: Vec<u16> = state.text.encode_utf16().collect();
        let (start, end) = (state.anchor.min(state.focus), state.anchor.max(state.focus));
        let touched = self.touched.replace(false);
        let mut field = self.field.borrow_mut();
        let same = field.as_ref().is_some_and(|f| {
            f.key == state.key && f.text == text && (f.start, f.end) == (start, end)
        });
        if same && !touched {
            return;
        }
        *field = Some(FieldSync {
            key: state.key,
            text,
            start,
            end,
            waiting: false,
        });
        drop(field);
        let element: &web_sys::Element = state.element.unchecked_ref();
        if native(element).is_some_and(|(value, _, _)| value != state.text) {
            set_native_value(element, &state.text);
        }
        let direction = if state.focus < state.anchor {
            "backward"
        } else {
            "forward"
        };
        set_native_selection(element, start, end, direction);
    }

    /// The page edited a field's element (typing, pasting, an input
    /// method's word, a phone keyboard's correction): the toolkit's field
    /// gets the same edit. A field the toolkit hasn't focused yet (typed in
    /// the moment it was reached) is focused first, and its edit sent once
    /// it is.
    fn edited(&self, event: &web_sys::Event) {
        if event
            .dyn_ref::<web_sys::InputEvent>()
            .is_some_and(web_sys::InputEvent::is_composing)
        {
            // An input method is composing: its word comes when it ends.
            return;
        }
        let Some(element) = event
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
        else {
            return;
        };
        let Some(key) = self.key(&element) else {
            return;
        };
        let shown = self.field.borrow().as_ref().is_some_and(|f| f.key == key);
        if !shown {
            let focus = {
                let mirror = self.mirror.borrow();
                let Some(state) = mirror.field_state_of(key) else {
                    return;
                };
                let waiting = !mirror.has_focus(key);
                *self.field.borrow_mut() = Some(FieldSync {
                    key,
                    text: state.text.encode_utf16().collect(),
                    start: state.anchor.min(state.focus),
                    end: state.anchor.max(state.focus),
                    waiting,
                });
                waiting.then(|| mirror.target(key)).flatten()
            };
            if let Some((node, tree)) = focus {
                self.act(Action::Focus, node, tree);
                return;
            }
        } else if self.field.borrow().as_ref().is_some_and(|f| f.waiting) {
            // Still waiting for the toolkit's focus: the edits add up.
            return;
        }
        self.send_edit(key, &element);
    }

    /// Sends the toolkit what changed in the field `key` names since its
    /// element last showed the toolkit's field: a selection (unless the
    /// change was at the selection) and its replacement.
    fn send_edit(&self, key: u128, element: &web_sys::Element) {
        let Some((value, now_start, now_end)) = native(element) else {
            return;
        };
        let value: Vec<u16> = value.encode_utf16().collect();
        let requests = {
            let mirror = self.mirror.borrow();
            let mut field = self.field.borrow_mut();
            let Some(shown) = field.as_mut().filter(|f| f.key == key) else {
                return;
            };
            let (start, end, inserted) = field::diff(&shown.text, &value);
            let mut requests = Vec::new();
            if (start, end) != (shown.start, shown.end) {
                requests.extend(mirror.select(key, start, end));
            }
            if start != end || !inserted.is_empty() {
                requests
                    .extend(mirror.replace_selection(key, &String::from_utf16_lossy(&inserted)));
            }
            *shown = FieldSync {
                key,
                text: value,
                start: now_start,
                end: now_end,
                waiting: false,
            };
            requests
        };
        self.touched.set(true);
        for request in requests {
            (self.on_action.borrow_mut())(request);
        }
    }

    /// The page moved a field's caret (a screen reader's own commands, a
    /// phone's text navigation): the toolkit's field selects the same.
    fn caret_moved(&self) {
        let Some(active) = self
            .container
            .owner_document()
            .and_then(|d| d.active_element())
        else {
            return;
        };
        let (Some(key), Some((_, start, end))) = (self.key(&active), native(&active)) else {
            return;
        };
        let request = {
            let mut field = self.field.borrow_mut();
            let Some(shown) = field.as_mut().filter(|f| f.key == key) else {
                return;
            };
            if (shown.start, shown.end) == (start, end) {
                return;
            }
            shown.start = start;
            shown.end = end;
            self.mirror.borrow().select(key, start, end)
        };
        self.touched.set(true);
        if let Some(request) = request {
            (self.on_action.borrow_mut())(request);
        }
    }

    /// The key of the node `element` (or its nearest ancestor among the
    /// adapter's elements) stands for.
    fn key(&self, element: &web_sys::Element) -> Option<u128> {
        let prefix = format!("{}-", self.container.id());
        let mut at = Some(element.clone());
        while let Some(element) = at {
            if let Some(key) = element.id().strip_prefix(&prefix) {
                return key.parse().ok();
            }
            at = element.parent_element();
        }
        None
    }
}

/// The page's DOM, as the mirror changes it.
struct PageDom {
    document: web_sys::Document,
    container: web_sys::HtmlElement,
    /// The container's `id`, and so the start of its elements'.
    prefix: String,
    polite: web_sys::HtmlElement,
    assertive: web_sys::HtmlElement,
    /// What each announcer said last (polite, assertive).
    said: [String; 2],
}

impl Dom for PageDom {
    type Element = web_sys::HtmlElement;

    fn create(&mut self, key: u128, tag: Tag) -> Self::Element {
        let name = match tag {
            Tag::Div => "div",
            Tag::Input | Tag::Password => "input",
            Tag::TextArea => "textarea",
        };
        let element: web_sys::HtmlElement = self
            .document
            .create_element(name)
            .expect("an element")
            .unchecked_into();
        element.set_id(&format!("{}-{key}", self.prefix));
        if tag.is_field() {
            if tag == Tag::Password {
                element.set_attribute("type", "password").ok();
            }
            // The toolkit's field decides; the browser's helpers would
            // only get in the way.
            for (name, value) in [
                ("autocomplete", "off"),
                ("autocapitalize", "off"),
                ("spellcheck", "false"),
            ] {
                element.set_attribute(name, value).ok();
            }
        }
        element
    }

    fn replace(&mut self, old: &Self::Element, new: &Self::Element) {
        old.replace_with_with_node_1(new).ok();
    }

    fn set_value(&mut self, element: &Self::Element, value: &str) {
        let element: &web_sys::Element = element.unchecked_ref();
        if native(element).is_some_and(|(now, _, _)| now != value) {
            set_native_value(element, value);
        }
    }

    fn set_attribute(&mut self, element: &Self::Element, name: &str, value: &str) {
        element.set_attribute(name, value).ok();
    }

    fn remove_attribute(&mut self, element: &Self::Element, name: &str) {
        element.remove_attribute(name).ok();
    }

    fn set_text(&mut self, element: &Self::Element, text: Option<&str>) {
        element.set_text_content(text);
    }

    fn place(&mut self, element: &Self::Element, bounds: Option<Rect>) {
        match bounds {
            Some(b) => {
                element.set_attribute("data-placed", "").ok();
                element.style().set_css_text(&format!(
                    "left:{}px;top:{}px;width:{}px;height:{}px",
                    b.x0,
                    b.y0,
                    (b.x1 - b.x0).max(0.0),
                    (b.y1 - b.y0).max(0.0)
                ));
            }
            None => {
                element.remove_attribute("data-placed").ok();
                element.style().set_css_text("");
            }
        }
    }

    fn set_children(&mut self, parent: &Self::Element, children: &[Self::Element]) {
        for (i, child) in children.iter().enumerate() {
            let at = parent.children().item(i as u32);
            if at.as_ref() != Some(child.unchecked_ref::<web_sys::Element>()) {
                parent.insert_before(child, at.as_deref()).ok();
            }
        }
        while parent.children().length() as usize > children.len() {
            match parent.last_element_child() {
                Some(extra) => extra.remove(),
                None => break,
            }
        }
    }

    fn set_root(&mut self, element: &Self::Element) {
        self.container
            .insert_before(element, self.container.first_child().as_ref())
            .ok();
    }

    fn remove(&mut self, element: &Self::Element) {
        element.remove();
    }

    fn announce(&mut self, text: &str, assertive: bool) {
        let (region, said) = if assertive {
            (&self.assertive, &mut self.said[1])
        } else {
            (&self.polite, &mut self.said[0])
        };
        // The same words twice are a change only if they differ: a
        // no-break space after them, every other time.
        let mut words = text.to_owned();
        if *said == words {
            words.push('\u{a0}');
        }
        region.set_text_content(Some(&words));
        *said = words;
    }
}

/// A field element's text and selection (UTF-16 code units), as the page
/// has them; `None` for an element that isn't a field.
fn native(element: &web_sys::Element) -> Option<(String, usize, usize)> {
    let (value, start, end) = if let Some(input) = element.dyn_ref::<web_sys::HtmlInputElement>() {
        (
            input.value(),
            input.selection_start(),
            input.selection_end(),
        )
    } else {
        let area = element.dyn_ref::<web_sys::HtmlTextAreaElement>()?;
        (area.value(), area.selection_start(), area.selection_end())
    };
    let start = start.ok().flatten().unwrap_or(0) as usize;
    let end = end.ok().flatten().map_or(start, |end| end as usize);
    Some((value, start, end))
}

fn set_native_value(element: &web_sys::Element, value: &str) {
    if let Some(input) = element.dyn_ref::<web_sys::HtmlInputElement>() {
        input.set_value(value);
    } else if let Some(area) = element.dyn_ref::<web_sys::HtmlTextAreaElement>() {
        area.set_value(value);
    }
}

fn set_native_selection(element: &web_sys::Element, start: usize, end: usize, direction: &str) {
    let (start, end) = (start as u32, end as u32);
    if let Some(input) = element.dyn_ref::<web_sys::HtmlInputElement>() {
        input
            .set_selection_range_with_direction(start, end, direction)
            .ok();
    } else if let Some(area) = element.dyn_ref::<web_sys::HtmlTextAreaElement>() {
        area.set_selection_range_with_direction(start, end, direction)
            .ok();
    }
}

/// A live region saying what the adapter announces.
fn announcer(document: &web_sys::Document, live: &str) -> Result<web_sys::HtmlElement, JsValue> {
    let region: web_sys::HtmlElement = document.create_element("div")?.dyn_into()?;
    region.set_attribute("aria-live", live)?;
    region.set_attribute("aria-atomic", "true")?;
    Ok(region)
}

/// Adds the adapter's style sheet to the page, once.
fn add_style(document: &web_sys::Document) -> Result<(), JsValue> {
    if document.get_element_by_id(STYLE_ID).is_some() {
        return Ok(());
    }
    let style = document.create_element("style")?;
    style.set_id(STYLE_ID);
    style.set_text_content(Some(STYLE));
    let head = document
        .head()
        .map(|h| h.unchecked_into::<web_sys::Node>())
        .or_else(|| document.document_element().map(Into::into))
        .ok_or_else(|| JsValue::from_str("the page has no head"))?;
    head.append_child(&style)?;
    Ok(())
}

/// A number for each adapter on the page, naming its container.
fn next_number() -> u32 {
    thread_local! {
        static NEXT: Cell<u32> = const { Cell::new(0) };
    }
    NEXT.with(|next| {
        let number = next.get();
        next.set(number + 1);
        number
    })
}
