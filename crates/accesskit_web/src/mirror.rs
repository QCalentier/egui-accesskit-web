//! The tree mirrored into elements: AccessKit's consumer says which nodes
//! were added, changed or removed in each update, and only their elements
//! are touched.

use std::collections::{HashMap, HashSet};

use accesskit::{Action, ActionData, ActionRequest, NodeId as LocalNodeId, TreeId, TreeUpdate};
use accesskit_consumer::{Node, NodeId, Tree, TreeChangeHandler};

use crate::dom::{Dom, Tag};
use crate::element::{self, Element, describe};
use crate::role;

/// The elements standing for a tree's nodes, kept in step with it.
pub(crate) struct Mirror<D: Dom> {
    dom: D,
    tree: Option<Tree>,
    shown: HashMap<NodeId, Shown<D::Element>>,
    /// Each node by its key, the number its element's `id` carries.
    keys: HashMap<u128, NodeId>,
    root: Option<NodeId>,
    focus: Option<NodeId>,
    /// Device pixels in a CSS pixel, as last placed.
    scale: f64,
}

/// A node's element, as it was last told to be.
struct Shown<E> {
    element: E,
    tag: Tag,
    described: Element,
    children: Vec<NodeId>,
}

/// The focused field, as the toolkit has it: its element, its text, and
/// its selection's ends (in UTF-16 code units, as a browser counts).
#[derive(Debug)]
pub(crate) struct FieldState<E> {
    pub key: u128,
    pub element: E,
    pub text: String,
    pub anchor: usize,
    pub focus: usize,
}

/// Where the focus should go after an update.
#[derive(Debug, PartialEq)]
pub(crate) enum Focus<E> {
    /// To this element.
    To(E),
    /// Nowhere in particular: the tree's root has it.
    Root,
}

/// The nodes an update added, changed and removed.
#[derive(Default)]
struct Changes {
    added: Vec<NodeId>,
    updated: Vec<NodeId>,
    removed: Vec<NodeId>,
}

impl TreeChangeHandler for Changes {
    fn node_added(&mut self, node: &Node) {
        self.added.push(node.id());
    }

    fn node_updated(&mut self, _old: &Node, new: &Node) {
        self.updated.push(new.id());
    }

    fn focus_moved(&mut self, _old: Option<&Node>, _new: Option<&Node>) {}

    fn node_removed(&mut self, node: &Node) {
        self.removed.push(node.id());
    }
}

impl<D: Dom> Mirror<D> {
    pub(crate) fn new(dom: D) -> Self {
        Self {
            dom,
            tree: None,
            shown: HashMap::new(),
            keys: HashMap::new(),
            root: None,
            focus: None,
            scale: 1.0,
        }
    }

    #[cfg(test)]
    pub(crate) fn dom(&self) -> &D {
        &self.dom
    }

    /// Applies `update` to the elements, placed for `scale` device pixels
    /// in a CSS pixel; answers where the focus went, if it moved.
    pub(crate) fn update(&mut self, update: TreeUpdate, scale: f64) -> Option<Focus<D::Element>> {
        let mut changes = Changes::default();
        let first = self.tree.is_none();
        match &mut self.tree {
            Some(tree) => tree.update_and_process_changes(update, &mut changes),
            None => {
                let tree = Tree::new(update, true);
                walk(&tree.state().root(), &mut |node| {
                    changes.added.push(node.id())
                });
                self.tree = Some(tree);
            }
        }
        let Some(tree) = &self.tree else {
            return None;
        };
        let state = tree.state();

        for id in &changes.removed {
            if let Some(shown) = self.shown.remove(id) {
                self.dom.remove(&shown.element);
                self.keys.remove(&u128::from(*id));
            }
        }
        for id in &changes.added {
            let key = u128::from(*id);
            let tag = state.node_by_id(*id).map_or(Tag::Div, |n| element::tag(&n));
            let element = self.dom.create(key, tag);
            self.keys.insert(key, *id);
            self.shown.insert(
                *id,
                Shown {
                    element,
                    tag,
                    described: Element::default(),
                    children: Vec::new(),
                },
            );
        }

        // Each changed node's element, then the children of each: once
        // every element they need exists. A node that moved has what is
        // placed in it placed again.
        let mut dirty: Vec<NodeId> = changes.added.clone();
        dirty.extend(&changes.updated);
        if scale != self.scale {
            self.scale = scale;
            dirty = self.shown.keys().copied().collect();
        }
        let mut seen: HashSet<NodeId> = dirty.iter().copied().collect();
        // A field's text is in nodes under it (text runs): when they
        // change, the field's element does.
        for at in 0..dirty.len() {
            let mut ancestor = state.node_by_id(dirty[at]).and_then(|n| n.parent());
            while let Some(node) = ancestor {
                if self.shown.get(&node.id()).is_some_and(|s| s.tag.is_field()) {
                    if seen.insert(node.id()) {
                        dirty.push(node.id());
                    }
                    break;
                }
                ancestor = node.parent();
            }
        }
        let mut said = Vec::new();
        let mut next = 0;
        while next < dirty.len() {
            let id = dirty[next];
            next += 1;
            let (Some(node), Some(shown)) = (state.node_by_id(id), self.shown.get_mut(&id)) else {
                continue;
            };
            let described = describe(&node, scale);
            if described.tag != shown.tag {
                // A field that stopped being one, or the other way: a new
                // element of its kind, where the old one was.
                let element = self.dom.create(u128::from(id), described.tag);
                self.dom.replace(&shown.element, &element);
                *shown = Shown {
                    element,
                    tag: described.tag,
                    described: Element::default(),
                    children: Vec::new(),
                };
            }
            let corner = |e: &Element| e.absolute.map(|r| (r.x0, r.y0));
            if corner(&described) != corner(&shown.described) {
                for child in node.child_ids() {
                    if seen.insert(child) {
                        dirty.push(child);
                    }
                }
            }
            if let Some(announce) = &described.announce
                && !first
                && shown.described.announce.as_ref() != Some(announce)
            {
                said.push((node.index_path(), announce.clone()));
            }
            apply(&mut self.dom, &shown.element, &shown.described, &described);
            shown.described = described;
        }
        for id in &dirty {
            let Some(node) = state.node_by_id(*id) else {
                continue;
            };
            // A field holds its text, not elements: its text runs aren't
            // shown.
            let field = self.shown.get(id).is_some_and(|s| s.tag.is_field());
            let children: Vec<NodeId> = if field {
                Vec::new()
            } else {
                node.child_ids().collect()
            };
            if self.shown.get(id).is_some_and(|s| s.children == children) {
                continue;
            }
            let elements: Vec<D::Element> = children
                .iter()
                .filter_map(|child| self.shown.get(child).map(|s| s.element.clone()))
                .collect();
            if let Some(shown) = self.shown.get_mut(id) {
                self.dom.set_children(&shown.element, &elements);
                shown.children = children;
            }
        }
        let root = state.root_id();
        if self.root != Some(root)
            && let Some(shown) = self.shown.get(&root)
        {
            self.dom.set_root(&shown.element);
            self.root = Some(root);
        }
        // In reading order (updates come in no order).
        said.sort_by(|(a, _), (b, _)| a.cmp(b));
        for (_, (text, assertive)) in said {
            self.dom.announce(&text, assertive);
        }

        let focus = state.focus_id();
        if focus == self.focus {
            return None;
        }
        self.focus = focus;
        Some(
            match focus
                .filter(|id| *id != root)
                .and_then(|id| self.shown.get(&id))
            {
                Some(shown) => Focus::To(shown.element.clone()),
                None => Focus::Root,
            },
        )
    }

    /// The node whose element has `key`, as an action names it.
    pub(crate) fn target(&self, key: u128) -> Option<(LocalNodeId, TreeId)> {
        let id = self.keys.get(&key)?;
        Some(self.tree.as_ref()?.state().node_by_id(*id)?.locate())
    }

    /// Whether the node `key` names has the focus.
    pub(crate) fn has_focus(&self, key: u128) -> bool {
        self.keys
            .get(&key)
            .is_some_and(|id| self.focus == Some(*id))
    }

    /// The name of the focused node when it is a field one types in (an
    /// empty name if it has none).
    pub(crate) fn focused_field(&self) -> Option<String> {
        let focus = self.focus.filter(|id| Some(*id) != self.root)?;
        let node = self.tree.as_ref()?.state().node_by_id(focus)?;
        role::typed_in(node.role()).then(|| node.label().unwrap_or_default())
    }

    /// The focused node, when it is a field one types in: its element,
    /// text and selection.
    pub(crate) fn focused_field_state(&self) -> Option<FieldState<D::Element>> {
        self.field_state(self.focus.filter(|id| Some(*id) != self.root)?)
    }

    /// The field `key` names, as the toolkit has it, focused or not.
    pub(crate) fn field_state_of(&self, key: u128) -> Option<FieldState<D::Element>> {
        self.field_state(*self.keys.get(&key)?)
    }

    fn field_state(&self, id: NodeId) -> Option<FieldState<D::Element>> {
        let shown = self.shown.get(&id).filter(|s| s.tag.is_field())?;
        let node = self.tree.as_ref()?.state().node_by_id(id)?;
        let text = element::field_text(&node);
        let end = text.encode_utf16().count();
        let (anchor, focus) = match (node.text_selection_anchor(), node.text_selection_focus()) {
            (Some(anchor), Some(focus)) => (
                anchor.to_global_utf16_index(),
                focus.to_global_utf16_index(),
            ),
            _ => (end, end),
        };
        Some(FieldState {
            key: u128::from(id),
            element: shown.element.clone(),
            text,
            anchor,
            focus,
        })
    }

    /// The request selecting `start..end` (UTF-16 code units) of the text
    /// of the field `key` names.
    pub(crate) fn select(&self, key: u128, start: usize, end: usize) -> Option<ActionRequest> {
        let node = self
            .tree
            .as_ref()?
            .state()
            .node_by_id(*self.keys.get(&key)?)?;
        if !node.supports_text_ranges() {
            return None;
        }
        let mut range = node
            .text_position_from_global_utf16_index(start.min(end))?
            .to_degenerate_range();
        range.set_end(node.text_position_from_global_utf16_index(start.max(end))?);
        let (target_node, target_tree) = node.locate();
        Some(ActionRequest {
            action: Action::SetTextSelection,
            target_tree,
            target_node,
            data: Some(ActionData::SetTextSelection(range.to_text_selection())),
        })
    }

    /// The request replacing the selected text of the field `key` names
    /// with `text` (deleting it, if empty).
    pub(crate) fn replace_selection(&self, key: u128, text: &str) -> Option<ActionRequest> {
        let (target_node, target_tree) = self.target(key)?;
        Some(ActionRequest {
            action: Action::ReplaceSelectedText,
            target_tree,
            target_node,
            data: Some(ActionData::Value(text.into())),
        })
    }

    /// The element of the focused node, unless the root has the focus.
    pub(crate) fn focused(&self) -> Option<D::Element> {
        let focus = self.focus.filter(|id| Some(*id) != self.root)?;
        Some(self.shown.get(&focus)?.element.clone())
    }

    /// The first node, in reading order, that takes the focus.
    pub(crate) fn first_focusable(&self) -> Option<(LocalNodeId, TreeId)> {
        let mut found = None;
        walk(&self.tree.as_ref()?.state().root(), &mut |node| {
            if found.is_none() && node.data().supports_action(Action::Focus) && !node.is_hidden() {
                found = Some(node.locate());
            }
        });
        found
    }

    /// The node `key` names, or its nearest ancestor, that takes a click
    /// (a screen reader activating text inside a button activates the
    /// button).
    pub(crate) fn clickable(&self, key: u128) -> Option<(LocalNodeId, TreeId)> {
        let state = self.tree.as_ref()?.state();
        let mut node = state.node_by_id(*self.keys.get(&key)?)?;
        loop {
            if node.data().supports_action(Action::Click) {
                return Some(node.locate());
            }
            node = node.parent()?;
        }
    }

    /// The language the root says the tree is in.
    pub(crate) fn language(&self) -> Option<String> {
        let state = self.tree.as_ref()?.state();
        state.root().data().language().map(str::to_owned)
    }
}

/// Calls `visit` on `node` and every node under it, in reading order.
fn walk(node: &Node, visit: &mut impl FnMut(&Node)) {
    visit(node);
    for child in node.children() {
        walk(&child, visit);
    }
}

/// Tells `element` what changed from `old` to `new`.
fn apply<D: Dom>(dom: &mut D, element: &D::Element, old: &Element, new: &Element) {
    for (name, value) in &new.attributes {
        let before = old.attributes.iter().find(|(n, _)| n == name);
        if before.map(|(_, v)| v) != Some(value) {
            dom.set_attribute(element, name, value);
        }
    }
    for (name, _) in &old.attributes {
        if !new.attributes.iter().any(|(n, _)| n == name) {
            dom.remove_attribute(element, name);
        }
    }
    if old.text != new.text {
        dom.set_text(element, new.text.as_deref());
    }
    if old.value != new.value {
        dom.set_value(element, new.value.as_deref().unwrap_or_default());
    }
    if old.bounds != new.bounds {
        dom.place(element, new.bounds);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use accesskit::{Live, Node as NodeData, NodeId as Id, Rect, Role, Toggled, Tree as TreeData};

    use super::*;

    /// A DOM in memory: elements by key, each with its attributes, text,
    /// place and children; what was announced.
    #[derive(Default)]
    struct Fake {
        elements: BTreeMap<u128, FakeElement>,
        root: Option<u128>,
        announced: Vec<(String, bool)>,
        /// How many changes it was asked for.
        writes: usize,
    }

    #[derive(Default, Debug)]
    struct FakeElement {
        tag: Tag,
        value: String,
        attributes: BTreeMap<String, String>,
        text: Option<String>,
        bounds: Option<Rect>,
        children: Vec<u128>,
    }

    impl Dom for Fake {
        type Element = u128;

        fn create(&mut self, key: u128, tag: Tag) -> u128 {
            self.elements.insert(
                key,
                FakeElement {
                    tag,
                    ..FakeElement::default()
                },
            );
            key
        }

        fn replace(&mut self, _old: &u128, _new: &u128) {}

        fn set_value(&mut self, element: &u128, value: &str) {
            self.writes += 1;
            self.elements.get_mut(element).unwrap().value = value.into();
        }

        fn set_attribute(&mut self, element: &u128, name: &str, value: &str) {
            self.writes += 1;
            let element = self.elements.get_mut(element).unwrap();
            element.attributes.insert(name.into(), value.into());
        }

        fn remove_attribute(&mut self, element: &u128, name: &str) {
            self.writes += 1;
            self.elements
                .get_mut(element)
                .unwrap()
                .attributes
                .remove(name);
        }

        fn set_text(&mut self, element: &u128, text: Option<&str>) {
            self.writes += 1;
            self.elements.get_mut(element).unwrap().text = text.map(str::to_owned);
        }

        fn place(&mut self, element: &u128, bounds: Option<Rect>) {
            self.writes += 1;
            self.elements.get_mut(element).unwrap().bounds = bounds;
        }

        fn set_children(&mut self, parent: &u128, children: &[u128]) {
            self.writes += 1;
            self.elements.get_mut(parent).unwrap().children = children.to_vec();
        }

        fn set_root(&mut self, element: &u128) {
            self.root = Some(*element);
        }

        fn remove(&mut self, element: &u128) {
            self.elements.remove(element);
        }

        fn announce(&mut self, text: &str, assertive: bool) {
            self.announced.push((text.into(), assertive));
        }
    }

    impl Fake {
        /// The element of the node with this local id (tree 0).
        fn of(&self, id: u64) -> &FakeElement {
            self.elements
                .iter()
                .find(|(key, _)| (**key >> 64) == u128::from(id))
                .map(|(_, e)| e)
                .unwrap_or_else(|| panic!("no element for node {id}"))
        }

        fn attribute(&self, id: u64, name: &str) -> Option<&str> {
            self.of(id).attributes.get(name).map(String::as_str)
        }
    }

    fn node(role: Role) -> NodeData {
        NodeData::new(role)
    }

    /// A window (0) holding `children`, numbered from 1; `focus` focused.
    fn update(children: Vec<NodeData>, focus: u64) -> TreeUpdate {
        let mut root = node(Role::Window);
        root.set_bounds(Rect::new(10.0, 10.0, 810.0, 610.0));
        root.set_children((1..=children.len() as u64).map(Id).collect::<Vec<_>>());
        let mut nodes = vec![(Id(0), root)];
        nodes.extend((1..).map(Id).zip(children));
        TreeUpdate {
            nodes,
            tree: Some(TreeData::new(Id(0))),
            tree_id: TreeId::ROOT,
            focus: Id(focus),
        }
    }

    fn button(name: &str) -> NodeData {
        let mut button = node(Role::Button);
        button.set_label(name);
        button.add_action(Action::Focus);
        button.add_action(Action::Click);
        button.set_bounds(Rect::new(20.0, 40.0, 120.0, 72.0));
        button
    }

    #[test]
    fn nodes_become_elements_saying_what_they_are() {
        let mut heading = node(Role::Heading);
        heading.set_label("Settings");
        heading.set_level(1);
        let mut text = node(Role::Label);
        text.set_value("Applies at once.");
        let mut slider = node(Role::Slider);
        slider.set_label("Text size");
        slider.set_value("120 %");
        slider.set_numeric_value(120.0);
        slider.set_min_numeric_value(50.0);
        slider.set_max_numeric_value(200.0);
        slider.add_action(Action::Focus);
        let mut switch = node(Role::Switch);
        switch.set_label("Sound");
        switch.set_toggled(Toggled::True);
        let mut tab = node(Role::Tab);
        tab.set_label("Map");
        tab.set_selected(true);
        let mut picture = node(Role::Image);
        picture.set_label("A map of the coast");
        let mut line = node(Role::Image);
        line.set_hidden();

        let mut m = Mirror::new(Fake::default());
        m.update(
            update(
                vec![
                    heading,
                    text,
                    slider,
                    switch,
                    tab,
                    picture,
                    line,
                    button("Save"),
                ],
                0,
            ),
            2.0,
        );
        let dom = m.dom();

        // The root holds them all, in order.
        let root = dom.root.expect("a root");
        assert_eq!(dom.elements[&root].children.len(), 8);
        assert_eq!(dom.attribute(0, "role"), None);

        assert_eq!(dom.attribute(1, "role"), Some("heading"));
        assert_eq!(dom.attribute(1, "aria-level"), Some("1"));
        assert_eq!(dom.of(1).text.as_deref(), Some("Settings"));
        assert_eq!(dom.attribute(1, "aria-label"), None, "named by its text");

        assert_eq!(dom.attribute(2, "role"), None);
        assert_eq!(dom.of(2).text.as_deref(), Some("Applies at once."));

        assert_eq!(dom.attribute(3, "role"), Some("slider"));
        assert_eq!(dom.attribute(3, "aria-label"), Some("Text size"));
        assert_eq!(dom.attribute(3, "aria-valuenow"), Some("120"));
        assert_eq!(dom.attribute(3, "aria-valuemin"), Some("50"));
        assert_eq!(dom.attribute(3, "aria-valuemax"), Some("200"));
        assert_eq!(dom.attribute(3, "aria-valuetext"), Some("120 %"));
        assert_eq!(dom.attribute(3, "tabindex"), Some("-1"));

        assert_eq!(dom.attribute(4, "role"), Some("switch"));
        assert_eq!(dom.attribute(4, "aria-checked"), Some("true"));
        assert_eq!(dom.attribute(5, "role"), Some("tab"));
        assert_eq!(dom.attribute(5, "aria-selected"), Some("true"));
        assert_eq!(dom.attribute(6, "role"), Some("img"));
        assert_eq!(dom.attribute(6, "aria-label"), Some("A map of the coast"));
        assert_eq!(dom.attribute(7, "aria-hidden"), Some("true"));

        // Placed in CSS pixels, each in its parent.
        assert_eq!(dom.attribute(8, "role"), Some("button"));
        assert_eq!(
            dom.elements[&root].bounds,
            Some(Rect::new(5.0, 5.0, 405.0, 305.0))
        );
        assert_eq!(dom.of(8).bounds, Some(Rect::new(5.0, 15.0, 55.0, 31.0)));
    }

    #[test]
    fn only_what_changed_is_touched() {
        let mut m = Mirror::new(Fake::default());
        m.update(update(vec![button("Save"), button("Quit")], 0), 1.0);
        let writes = m.dom().writes;

        // The same tree again (as egui sends it every frame): nothing.
        m.update(update(vec![button("Save"), button("Quit")], 0), 1.0);
        assert_eq!(m.dom().writes, writes);

        // One name changed: one write.
        m.update(update(vec![button("Save"), button("Leave")], 0), 1.0);
        assert_eq!(m.dom().writes, writes + 1);
        assert_eq!(m.dom().of(2).text.as_deref(), Some("Leave"));

        // A node gone: its element too, and its parent's children.
        m.update(update(vec![button("Save")], 0), 1.0);
        let root = m.dom().root.unwrap();
        assert_eq!(m.dom().elements[&root].children.len(), 1);
        assert_eq!(m.dom().elements.len(), 2);
    }

    #[test]
    fn what_is_in_a_node_that_moved_moves_with_it() {
        // The window moved (the canvas scrolled), its button didn't: the
        // button is placed again in it.
        let mut m = Mirror::new(Fake::default());
        m.update(update(vec![button("Save")], 0), 1.0);
        assert_eq!(
            m.dom().of(1).bounds,
            Some(Rect::new(10.0, 30.0, 110.0, 62.0))
        );
        let mut moved = update(vec![button("Save")], 0);
        moved.nodes[0]
            .1
            .set_bounds(Rect::new(0.0, 0.0, 800.0, 600.0));
        m.update(moved, 1.0);
        assert_eq!(
            m.dom().of(1).bounds,
            Some(Rect::new(20.0, 40.0, 120.0, 72.0))
        );
    }

    #[test]
    fn the_focus_is_followed_and_actions_find_their_node() {
        let mut m = Mirror::new(Fake::default());
        assert_eq!(
            m.update(update(vec![button("Save"), button("Quit")], 0), 1.0),
            Some(Focus::Root)
        );
        assert_eq!(m.first_focusable().map(|(id, _)| id), Some(Id(1)));

        // The toolkit focused Quit: its element should take the focus.
        let moved = m.update(update(vec![button("Save"), button("Quit")], 2), 1.0);
        let Some(Focus::To(element)) = moved else {
            panic!("{moved:?}")
        };
        assert!(m.has_focus(element));
        assert_eq!(m.target(element).map(|(id, _)| id), Some(Id(2)));
        assert_eq!(m.clickable(element).map(|(id, _)| id), Some(Id(2)));
        // Unchanged: nothing to move.
        assert_eq!(
            m.update(update(vec![button("Save"), button("Quit")], 2), 1.0),
            None
        );
    }

    #[test]
    fn statuses_are_announced_when_they_come_not_at_start() {
        let status = |text: &str, live: Live| {
            let mut status = node(Role::Status);
            status.set_label(text);
            status.set_live(live);
            status
        };
        let mut m = Mirror::new(Fake::default());
        m.update(update(vec![status("Ready", Live::Polite)], 0), 1.0);
        assert!(m.dom().announced.is_empty());

        m.update(
            update(
                vec![
                    status("Ready", Live::Polite),
                    status("Saved", Live::Polite),
                    status("Can't save", Live::Assertive),
                ],
                0,
            ),
            1.0,
        );
        assert_eq!(
            m.dom().announced,
            vec![("Saved".into(), false), ("Can't save".into(), true)]
        );
        // A status's element doesn't say it a second time.
        assert_eq!(m.dom().attribute(2, "role"), None);
        assert_eq!(m.dom().of(2).text.as_deref(), Some("Saved"));
    }

    #[test]
    fn a_log_says_its_new_lines_itself() {
        let log = |lines: &[&str]| {
            let mut log = node(Role::Log);
            log.set_live(Live::Polite);
            log.set_label("Chat");
            log.set_children(
                (0..lines.len() as u64)
                    .map(|i| Id(10 + i))
                    .collect::<Vec<_>>(),
            );
            let mut nodes = vec![(Id(1), log)];
            for (i, line) in lines.iter().enumerate() {
                let mut text = node(Role::Label);
                text.set_value(*line);
                nodes.push((Id(10 + i as u64), text));
            }
            let mut tree = update(Vec::new(), 0);
            tree.nodes[0].1.set_children(vec![Id(1)]);
            tree.nodes.extend(nodes);
            tree
        };
        let mut m = Mirror::new(Fake::default());
        m.update(log(&["> look"]), 1.0);
        m.update(log(&["> look", "A dark room."]), 1.0);
        assert!(m.dom().announced.is_empty());
        assert_eq!(m.dom().attribute(1, "role"), Some("log"));
        assert_eq!(m.dom().of(1).children.len(), 2);
        assert_eq!(m.dom().of(11).text.as_deref(), Some("A dark room."));
    }

    /// A text field (1) holding "hello" in one text run (2), its caret
    /// after "he".
    fn field(text: &str, caret: usize) -> TreeUpdate {
        use accesskit::{TextPosition, TextSelection};

        let mut field = node(Role::TextInput);
        field.set_label("Name");
        field.add_action(Action::Focus);
        field.set_children(vec![Id(2)]);
        let at = TextPosition {
            node: Id(2),
            character_index: caret,
        };
        field.set_text_selection(TextSelection {
            anchor: at,
            focus: at,
        });
        let mut run = node(Role::TextRun);
        run.set_value(text);
        run.set_character_lengths(vec![1; text.len()]);
        let mut tree = update(vec![field], 1);
        tree.nodes.push((Id(2), run));
        tree
    }

    #[test]
    fn a_field_is_a_real_one_holding_its_text_and_caret() {
        let mut m = Mirror::new(Fake::default());
        m.update(field("hello", 2), 1.0);
        let dom = m.dom();
        assert_eq!(dom.of(1).tag, Tag::Input);
        assert_eq!(dom.of(1).value, "hello");
        assert_eq!(dom.attribute(1, "aria-label"), Some("Name"));
        assert_eq!(dom.attribute(1, "role"), None, "an input says it itself");
        assert!(dom.of(1).children.is_empty(), "its text runs aren't shown");

        let state = m.focused_field_state().expect("the focused field");
        assert_eq!(
            (state.text.as_str(), state.anchor, state.focus),
            ("hello", 2, 2)
        );

        // Selecting "ell" for the toolkit: positions in its text run.
        let request = m.select(state.key, 1, 4).expect("a selection");
        let Some(ActionData::SetTextSelection(selection)) = request.data else {
            panic!("{request:?}")
        };
        assert_eq!(selection.anchor.character_index, 1);
        assert_eq!(selection.focus.character_index, 4);
        assert_eq!(request.target_node, Id(1));
        let typed = m.replace_selection(state.key, "i").unwrap();
        assert_eq!(typed.action, Action::ReplaceSelectedText);

        // The toolkit's text changed: the field's too.
        m.update(field("hi", 2), 1.0);
        assert_eq!(m.dom().of(1).value, "hi");
    }

    #[test]
    fn a_text_says_its_words_once() {
        // A text (1) holding its words in a text run (2), as egui gives
        // them: the words are the run's text, not a label as well.
        let mut text = node(Role::Label);
        text.set_value("Welcome.");
        text.set_children(vec![Id(2)]);
        let mut run = node(Role::TextRun);
        run.set_value("Welcome.");
        let mut tree = update(vec![text], 0);
        tree.nodes.push((Id(2), run));
        let mut m = Mirror::new(Fake::default());
        m.update(tree, 1.0);
        assert_eq!(m.dom().attribute(1, "aria-label"), None);
        assert_eq!(m.dom().of(1).text, None);
        assert_eq!(m.dom().of(2).text.as_deref(), Some("Welcome."));
    }
}
