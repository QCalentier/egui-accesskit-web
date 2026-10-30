//! What one node's element says: its role, name, states and place, worked
//! out from the node alone, so the mirror only compares and applies.

use accesskit::{Action, Live, Orientation, Rect, Role, Toggled};
use accesskit_consumer::Node;

use crate::dom::Tag;
use crate::role;

/// An element as it should be, for one node.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Element {
    /// Its kind: a field one types in is a real one.
    pub tag: Tag,
    /// A field's text.
    pub value: Option<String>,
    /// `role` and `aria-*` attributes, `tabindex`, sorted by name.
    pub attributes: Vec<(&'static str, String)>,
    /// Its text: a leaf's name when its role is named by its content, the
    /// value of a field.
    pub text: Option<String>,
    /// Where it is, in CSS pixels from the top left corner of its nearest
    /// ancestor that has bounds (the element it is placed in), or of the
    /// canvas.
    pub bounds: Option<Rect>,
    /// Where it is, in CSS pixels from the canvas's top left corner: when
    /// it moves, what is placed in it is placed again.
    pub absolute: Option<Rect>,
    /// What a screen reader should say when it appears or its name
    /// changes, and how urgently (`true`: at once).
    pub announce: Option<(String, bool)>,
}

/// The element standing for `node`, its bounds divided by `scale` (the
/// device pixels in a CSS pixel).
pub(crate) fn describe(node: &Node, scale: f64) -> Element {
    let role = node.role();
    let tag = tag(node);
    let leaf = node.child_ids().len() == 0;
    let mut attributes = Vec::new();
    let mut set = |name: &'static str, value: String| attributes.push((name, value));

    // A field's own element says it is a field; a search field or an
    // editable list says what it is too.
    if let Some(aria) = role::aria(role)
        && (!tag.is_field() || matches!(role, Role::SearchInput | Role::EditableComboBox))
    {
        set("role", aria.to_owned());
    }

    // The name: as content when the role takes it so (text, a button, a
    // heading), else as a label. A text's words are its value.
    let name = if role == Role::Label || role == Role::TextRun {
        node.value()
    } else {
        node.label()
    }
    .filter(|name| !name.trim().is_empty());
    let mut text = None;
    // An element without a role takes no name (ARIA ignores it, and its
    // text, in the elements it holds, says it already: egui gives a text
    // its words as text runs under it).
    if leaf && role::named_by_content(role) {
        text = name.clone();
    } else if let Some(name) = &name
        && role::aria(role).is_some()
    {
        set("aria-label", name.clone());
    }
    if leaf && role::editable(role) {
        text = node.value().filter(|value| !value.is_empty());
    }
    if let Some(description) = node.description() {
        set("aria-description", description);
    }
    if let Some(placeholder) = node.placeholder() {
        let attribute = if tag.is_field() {
            "placeholder"
        } else {
            "aria-placeholder"
        };
        set(attribute, placeholder.to_owned());
    }
    if let Some(shortcut) = node.data().keyboard_shortcut() {
        set("aria-keyshortcuts", shortcut.to_owned());
    }

    // States.
    if node.is_hidden() {
        set("aria-hidden", "true".to_owned());
    }
    if node.is_disabled() {
        set("aria-disabled", "true".to_owned());
    }
    if node.is_read_only_supported() && node.is_read_only() {
        set("aria-readonly", "true".to_owned());
    }
    if node.is_required() {
        set("aria-required", "true".to_owned());
    }
    if node.is_busy() {
        set("aria-busy", "true".to_owned());
    }
    if node.is_modal() {
        set("aria-modal", "true".to_owned());
    }
    if node.is_multiselectable() {
        set("aria-multiselectable", "true".to_owned());
    }
    if node.is_multiline() {
        set("aria-multiline", "true".to_owned());
    }
    if let Some(toggled) = node.toggled() {
        let state = match toggled {
            Toggled::True => "true",
            Toggled::False => "false",
            Toggled::Mixed => "mixed",
        };
        let attribute = if role::checkable(role) {
            "aria-checked"
        } else {
            "aria-pressed"
        };
        set(attribute, state.to_owned());
    }
    if let Some(selected) = node.is_selected() {
        if role::selectable(role) {
            set("aria-selected", selected.to_string());
        } else if node.toggled().is_none() {
            set("aria-pressed", selected.to_string());
        }
    }
    if let Some(expanded) = node.data().is_expanded() {
        set("aria-expanded", expanded.to_string());
    }
    if let Some(level) = node.level() {
        set("aria-level", level.to_string());
    }
    if let (Some(position), Some(size)) = (node.position_in_set(), node.size_of_set()) {
        set("aria-posinset", position.to_string());
        set("aria-setsize", size.to_string());
    }
    if let Some(orientation) = node.orientation() {
        let orientation = match orientation {
            Orientation::Horizontal => "horizontal",
            Orientation::Vertical => "vertical",
        };
        set("aria-orientation", orientation.to_owned());
    }
    if role::ranged(role) {
        let data = node.data();
        if let Some(now) = data.numeric_value() {
            set("aria-valuenow", number(now));
        }
        if let Some(min) = data.min_numeric_value() {
            set("aria-valuemin", number(min));
        }
        if let Some(max) = data.max_numeric_value() {
            set("aria-valuemax", number(max));
        }
        if let Some(said) = node.value() {
            set("aria-valuetext", said);
        }
    }
    // Live regions: a log is one (its lines said as they come); statuses,
    // alerts and other live nodes are said by the adapter's announcers,
    // which work for nodes just added too. Its own setting only: what a
    // live node holds is said with it.
    let live = node.data().live().unwrap_or(Live::Off);
    if role == Role::Log && live == Live::Assertive {
        set("aria-live", "assertive".to_owned());
    }
    // Kept quiet inside a live region (a log's line still growing, or one
    // scrolled into view): the browser says nothing of it.
    if node.data().live() == Some(Live::Off) {
        set("aria-live", "off".to_owned());
    }
    let announce = match live {
        Live::Off => None,
        _ if role == Role::Log => None,
        live => name.clone().map(|name| (name, live == Live::Assertive)),
    };

    // Focusable by script and by screen readers, not by Tab: the toolkit
    // moves its focus itself, and the element follows it; a widget's
    // active item too (focused through the widget), while it is.
    if node.data().supports_action(Action::Focus) || node.is_focused() {
        set("tabindex", "-1".to_owned());
    }

    attributes.sort_by_key(|(name, _)| *name);
    let css = |b: Rect| Rect::new(b.x0 / scale, b.y0 / scale, b.x1 / scale, b.y1 / scale);
    let absolute = node.bounding_box().map(css);
    let mut ancestor = node.parent();
    let origin = loop {
        match ancestor {
            Some(a) => match a.bounding_box() {
                Some(b) => break css(b),
                None => ancestor = a.parent(),
            },
            None => break Rect::ZERO,
        }
    };
    let bounds = absolute.map(|r| {
        Rect::new(
            r.x0 - origin.x0,
            r.y0 - origin.y0,
            r.x1 - origin.x0,
            r.y1 - origin.y0,
        )
    });
    Element {
        tag,
        value: tag.is_field().then(|| field_text(node)),
        attributes,
        text,
        bounds,
        absolute,
        announce,
    }
}

/// The kind of element standing for `node`.
pub(crate) fn tag(node: &Node) -> Tag {
    match node.role() {
        Role::PasswordInput => Tag::Password,
        role if role::typed_in(role) && node.is_multiline() => Tag::TextArea,
        role if role::typed_in(role) => Tag::Input,
        _ => Tag::Div,
    }
}

/// The text in a field: its text runs', or its value.
pub(crate) fn field_text(node: &Node) -> String {
    if node.supports_text_ranges() {
        node.document_range().text()
    } else {
        node.value().unwrap_or_default()
    }
}

/// A number as ARIA takes it: without a fractional part when it has none.
fn number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}
