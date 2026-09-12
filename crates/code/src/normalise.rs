//! Turning what a person typed, read aloud or scanned into the canonical code.

use crate::{ALPHABET, CODE_LEN};
use core::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodeError {
    /// A character outside the alphabet that is not a known alias. Carries
    /// the character as the user entered it.
    Character(char),
    /// Not exactly 16 characters once separators are removed.
    Length(usize),
}

impl fmt::Display for CodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodeError::Character(c) => write!(f, "{c:?} is not a delivery code character"),
            CodeError::Length(n) => write!(f, "a delivery code has {CODE_LEN} characters, not {n}"),
        }
    }
}

impl std::error::Error for CodeError {}

/// Canonicalise user input.
///
/// Strips ASCII whitespace and hyphens, uppercases, and maps the Crockford
/// aliases `I` and `L` to `1` and `O` to `0`, so a code read over the phone or
/// typed in lowercase still works. `U` and every other character outside the
/// alphabet is rejected. Characters are checked before length, so the first
/// bad character is always the one reported.
pub fn normalise(input: &str) -> Result<[u8; CODE_LEN], CodeError> {
    let mut code = [0u8; CODE_LEN];
    let mut len = 0;
    for c in input.chars() {
        if c.is_ascii_whitespace() || c == '-' {
            continue;
        }
        let canonical = match c.to_ascii_uppercase() {
            'I' | 'L' => '1',
            'O' => '0',
            other => other,
        };
        if !canonical.is_ascii() || !ALPHABET.contains(&(canonical as u8)) {
            return Err(CodeError::Character(c));
        }
        if len < CODE_LEN {
            code[len] = canonical as u8;
        }
        len += 1;
    }
    if len != CODE_LEN {
        return Err(CodeError::Length(len));
    }
    Ok(code)
}

/// Whether `code` is already in canonical form: exactly 16 alphabet bytes.
pub fn is_canonical(code: &[u8]) -> bool {
    code.len() == CODE_LEN && code.iter().all(|c| ALPHABET.contains(c))
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{display, encode};

    const CANONICAL: &[u8; CODE_LEN] = b"K7M29XQF4TBNR3WD";

    #[test]
    fn display_form_round_trips() {
        for seed in 0u8..=255 {
            let entropy = core::array::from_fn(|i| seed.wrapping_mul(97).wrapping_add(i as u8));
            let code = encode(&entropy);
            assert_eq!(normalise(&display(&code)), Ok(code));
        }
    }

    #[test]
    fn separators_case_and_whitespace_are_ignored() {
        for input in [
            "K7M2-9XQF-4TBN-R3WD",
            "k7m2-9xqf-4tbn-r3wd",
            "K7M2 9XQF 4TBN R3WD",
            "\tK7M29XQF4TBNR3WD\n",
        ] {
            assert_eq!(normalise(input), Ok(*CANONICAL), "{input:?}");
        }
    }

    #[test]
    fn aliases_map_to_digits() {
        assert_eq!(normalise("IIII-LLLL-iiii-llll"), Ok(*b"1111111111111111"));
        assert_eq!(normalise("OOOO-oooo-0000-OOOO"), Ok(*b"0000000000000000"));
    }

    #[test]
    fn invalid_characters_are_reported_before_length() {
        assert_eq!(
            normalise("K7M2-9XQF-4TBN-R3WU"),
            Err(CodeError::Character('U'))
        );
        assert_eq!(normalise("u"), Err(CodeError::Character('u')));
        assert_eq!(normalise("K7M2_9XQF"), Err(CodeError::Character('_')));
        assert_eq!(
            normalise("K7M2\u{2013}9XQF"),
            Err(CodeError::Character('\u{2013}'))
        );
    }

    #[test]
    fn wrong_lengths_are_rejected() {
        assert_eq!(normalise(""), Err(CodeError::Length(0)));
        assert_eq!(normalise("K7M2-9XQF-4TBN-R3W"), Err(CodeError::Length(15)));
        assert_eq!(
            normalise("K7M2-9XQF-4TBN-R3WDX"),
            Err(CodeError::Length(17))
        );
    }

    #[test]
    fn canonical_check() {
        assert!(is_canonical(CANONICAL));
        assert!(!is_canonical(b"K7M2-9XQF-4TBN-R3WD"));
        assert!(!is_canonical(b"k7m29xqf4tbnr3wd"));
        assert!(!is_canonical(b"K7M29XQF4TBNR3W"));
    }
}
