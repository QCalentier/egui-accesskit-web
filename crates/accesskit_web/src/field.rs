//! A field typed in: what changed in its element, in the UTF-16 code units
//! a browser counts (`selectionStart`, `value.length`).

/// What changed from `old` to `new`: the range of `old` that was replaced
/// (`start..end`), and what replaced it. A character written in two code
/// units (a surrogate pair) is never cut in half.
pub(crate) fn diff(old: &[u16], new: &[u16]) -> (usize, usize, Vec<u16>) {
    let mut prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    if prefix > 0 && is_high_surrogate(old[prefix - 1]) {
        prefix -= 1;
    }
    let room = old.len().min(new.len()) - prefix;
    let mut suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(room)
        .take_while(|(a, b)| a == b)
        .count();
    if suffix > 0 && is_low_surrogate(old[old.len() - suffix]) {
        suffix -= 1;
    }
    (
        prefix,
        old.len() - suffix,
        new[prefix..new.len() - suffix].to_vec(),
    )
}

fn is_high_surrogate(unit: u16) -> bool {
    (0xD800..=0xDBFF).contains(&unit)
}

fn is_low_surrogate(unit: u16) -> bool {
    (0xDC00..=0xDFFF).contains(&unit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }

    fn change(old: &str, new: &str) -> (usize, usize, String) {
        let (start, end, inserted) = diff(&units(old), &units(new));
        (start, end, String::from_utf16(&inserted).unwrap())
    }

    #[test]
    fn typing_deleting_and_replacing() {
        // A letter typed at the end, in the middle.
        assert_eq!(change("hell", "hello"), (4, 4, "o".into()));
        assert_eq!(change("hllo", "hello"), (1, 1, "e".into()));
        // A letter deleted; a word replaced (autocorrect); pasted over all.
        assert_eq!(change("hello", "helo"), (3, 4, String::new()));
        assert_eq!(change("teh cat", "the cat"), (1, 3, "he".into()));
        assert_eq!(change("old", "new"), (0, 3, "new".into()));
        // Nothing changed.
        assert_eq!(change("same", "same"), (4, 4, String::new()));
        // A repeated letter: the change is placed after the common part.
        assert_eq!(change("aa", "aaa"), (2, 2, "a".into()));
    }

    #[test]
    fn a_character_in_two_code_units_stays_whole() {
        // 😀 (U+1F600) and 😁 (U+1F601) share their high surrogate.
        assert_eq!(change("a😀b", "a😁b"), (1, 3, "😁".into()));
        assert_eq!(change("😀", "😀😀"), (2, 2, "😀".into()));
        assert_eq!(change("😀😁", "😁"), (0, 2, String::new()));
    }
}
