//! AccessKit's roles as ARIA's. AccessKit's roles follow Chromium's, which
//! follow ARIA's, so most map one for one; those with no ARIA role (a
//! plain container, a window, text) are elements without one.

use accesskit::Role;

/// The ARIA role of an element standing for a node of `role`, if any.
pub(crate) fn aria(role: Role) -> Option<&'static str> {
    use Role::*;

    Some(match role {
        // Structure.
        Main => "main",
        Banner | Header => "banner",
        ContentInfo | Footer => "contentinfo",
        Navigation => "navigation",
        Complementary => "complementary",
        Region | Section => "region",
        Search => "search",
        Form => "form",
        Group | Details => "group",
        Heading => "heading",
        Paragraph => "paragraph",
        Article => "article",
        Document => "document",
        Application => "application",
        Note => "note",
        Blockquote => "blockquote",
        Figure => "figure",
        FigureCaption | Caption => "caption",
        Code => "code",
        Emphasis => "emphasis",
        Strong => "strong",
        Definition => "definition",
        Term => "term",
        Time => "time",
        Math => "math",
        Toolbar => "toolbar",
        Tooltip => "tooltip",
        Feed => "feed",
        Marquee => "marquee",
        Timer => "timer",
        Image | Canvas | SvgRoot | GraphicsDocument | GraphicsObject | GraphicsSymbol => "img",
        // Lists, tables, trees.
        List | DescriptionList => "list",
        ListItem => "listitem",
        ListBox => "listbox",
        ListBoxOption | MenuListOption => "option",
        Table => "table",
        Grid | ListGrid => "grid",
        TreeGrid => "treegrid",
        Row => "row",
        RowGroup => "rowgroup",
        Cell => "cell",
        GridCell => "gridcell",
        ColumnHeader => "columnheader",
        RowHeader => "rowheader",
        Tree => "tree",
        TreeItem => "treeitem",
        // Controls.
        Button | DefaultButton | DisclosureTriangle => "button",
        Link => "link",
        CheckBox => "checkbox",
        Switch => "switch",
        RadioButton => "radio",
        RadioGroup => "radiogroup",
        ComboBox | EditableComboBox => "combobox",
        Slider => "slider",
        SpinButton => "spinbutton",
        ProgressIndicator => "progressbar",
        Meter => "meter",
        ScrollBar => "scrollbar",
        Splitter => "separator",
        TextInput | MultilineTextInput | PasswordInput | EmailInput | NumberInput
        | PhoneNumberInput | UrlInput | DateInput | DateTimeInput | WeekInput | MonthInput
        | TimeInput => "textbox",
        SearchInput => "searchbox",
        Tab => "tab",
        TabList => "tablist",
        TabPanel => "tabpanel",
        Menu | MenuListPopup => "menu",
        MenuBar => "menubar",
        MenuItem => "menuitem",
        MenuItemCheckBox => "menuitemcheckbox",
        MenuItemRadio => "menuitemradio",
        // Dialogs and messages. Statuses and alerts are said through the
        // adapter's announcers (see `mirror`); a log keeps its own live
        // region, its new lines said as they come.
        Dialog => "dialog",
        AlertDialog => "alertdialog",
        Log => "log",
        // No ARIA role: plain containers, windows, text, and the rest.
        _ => return None,
    })
}

/// Whether a leaf node of `role` is named by its text, as the element's
/// content (else by `aria-label`): text, headings, and controls ARIA
/// names from their content.
pub(crate) fn named_by_content(role: Role) -> bool {
    use Role::*;

    matches!(
        role,
        Label
            | TextRun
            | Heading
            | Paragraph
            | Button
            | DefaultButton
            | DisclosureTriangle
            | Link
            | CheckBox
            | Switch
            | RadioButton
            | Tab
            | TreeItem
            | ListItem
            | ListBoxOption
            | MenuListOption
            | MenuItem
            | MenuItemCheckBox
            | MenuItemRadio
            | Cell
            | GridCell
            | ColumnHeader
            | RowHeader
            | Tooltip
            | Status
            | Alert
            | Caption
            | FigureCaption
    )
}

/// Whether a node of `role` takes `aria-checked` for its toggled state
/// (else `aria-pressed`: a toggle button).
pub(crate) fn checkable(role: Role) -> bool {
    use Role::*;

    matches!(
        role,
        CheckBox | Switch | RadioButton | MenuItemCheckBox | MenuItemRadio | TreeItem
    )
}

/// Whether a node of `role` says it is selected with `aria-selected` (else
/// a selected button is pressed).
pub(crate) fn selectable(role: Role) -> bool {
    use Role::*;

    matches!(
        role,
        Tab | TreeItem | ListBoxOption | MenuListOption | Row | Cell | GridCell
    )
}

/// Whether a node of `role` has a value range (`aria-valuenow` and the
/// rest).
pub(crate) fn ranged(role: Role) -> bool {
    use Role::*;

    matches!(
        role,
        Slider | SpinButton | ProgressIndicator | Meter | ScrollBar | Splitter
    )
}

/// Whether a node of `role` holds a value one types (its value is its
/// element's content).
pub(crate) fn editable(role: Role) -> bool {
    use Role::*;

    matches!(
        role,
        TextInput
            | MultilineTextInput
            | SearchInput
            | EmailInput
            | NumberInput
            | PhoneNumberInput
            | UrlInput
            | DateInput
            | DateTimeInput
            | WeekInput
            | MonthInput
            | TimeInput
            | ComboBox
            | EditableComboBox
    )
}
