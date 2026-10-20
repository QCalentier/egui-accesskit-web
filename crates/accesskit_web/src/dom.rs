//! What the mirror needs of a DOM: the page's, or a stand-in in tests.

use accesskit::Rect;

/// Elements to create, change, order and remove. The mirror only asks for
/// changes: an element is told its new attributes, text and place only
/// when they changed.
pub(crate) trait Dom {
    /// An element (a handle to it: cloning it doesn't copy the element).
    type Element: Clone;

    /// A new element, for the node `key` names (its `id` says it), not in
    /// the document yet.
    fn create(&mut self, key: u128) -> Self::Element;

    /// Sets an attribute (`role`, `aria-*`, `tabindex`).
    fn set_attribute(&mut self, element: &Self::Element, name: &str, value: &str);

    /// Removes an attribute.
    fn remove_attribute(&mut self, element: &Self::Element, name: &str);

    /// Sets its text, a leaf's only content (`None`: no text).
    fn set_text(&mut self, element: &Self::Element, text: Option<&str>);

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
