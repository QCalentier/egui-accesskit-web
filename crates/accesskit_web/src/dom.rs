//! What the mirror needs of a DOM: the page's, or a stand-in in tests.

use accesskit::Rect;

/// What kind of element stands for a node: a `div`, or for a field one
/// types in a real one, which the browser edits (caret, selection, input
/// methods, a phone's keyboard) and screen readers read as a field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Tag {
    #[default]
    Div,
    /// `<input type="text">`.
    Input,
    /// `<input type="password">`.
    Password,
    /// `<textarea>`: several lines.
    TextArea,
}

impl Tag {
    /// Whether it is a field one types in.
    pub(crate) fn is_field(self) -> bool {
        self != Tag::Div
    }
}

/// Elements to create, change, order and remove. The mirror only asks for
/// changes: an element is told its new attributes, text and place only
/// when they changed.
pub(crate) trait Dom {
    /// An element (a handle to it: cloning it doesn't copy the element).
    type Element: Clone;

    /// A new element of kind `tag`, for the node `key` names (its `id`
    /// says it), not in the document yet.
    fn create(&mut self, key: u128, tag: Tag) -> Self::Element;

    /// Puts `new` where `old` is (a node whose element changed kind).
    fn replace(&mut self, old: &Self::Element, new: &Self::Element);

    /// Sets an attribute (`role`, `aria-*`, `tabindex`).
    fn set_attribute(&mut self, element: &Self::Element, name: &str, value: &str);

    /// Removes an attribute.
    fn remove_attribute(&mut self, element: &Self::Element, name: &str);

    /// Sets its text, a leaf's only content (`None`: no text).
    fn set_text(&mut self, element: &Self::Element, text: Option<&str>);

    /// Sets a field's text (its `value`).
    fn set_value(&mut self, element: &Self::Element, value: &str);

    /// Places it, in CSS pixels from the canvas's top left corner; `None`:
    /// no box of its own (a container).
    fn place(&mut self, element: &Self::Element, bounds: Option<Rect>);

    /// Makes `children` its child elements, in this order (moving them
    /// from where they were).
    fn set_children(&mut self, parent: &Self::Element, children: &[Self::Element]);

    /// Makes `element` the root of the elements, in the container.
    fn set_root(&mut self, element: &Self::Element);

    /// Takes the element (and what it holds) out of the document.
    fn remove(&mut self, element: &Self::Element);

    /// Says `text` through a live region: at once if `assertive`, else
    /// when the screen reader is free.
    fn announce(&mut self, text: &str, assertive: bool);
}
