//! The hash the escrow stores and checks.

use crate::CODE_LEN;
use sha2::{Digest, Sha256};

/// `release_code_hash` for a canonical code: SHA-256 of its 16 ASCII bytes,
/// exactly what the escrow computes when a code is presented. Normalise user
/// input first; hashing a display-form code gives a different, useless hash.
pub fn release_code_hash(code: &[u8; CODE_LEN]) -> [u8; 32] {
    Sha256::digest(code).into()
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::normalise;

    #[test]
    fn hashes_the_canonical_ascii_bytes() {
        // sha256("0000000000000000"), computed independently.
        let expected = [
            0xfc, 0xdb, 0x4b, 0x42, 0x3f, 0x4e, 0x52, 0x83, 0xaf, 0xa2, 0x49, 0xd7, 0x62, 0xef,
            0x6a, 0xef, 0x15, 0x0e, 0x91, 0xfc, 0xcd, 0x81, 0x0d, 0x43, 0xe5, 0xe7, 0x19, 0xd1,
            0x45, 0x12, 0xde, 0xc7,
        ];
        assert_eq!(release_code_hash(b"0000000000000000"), expected);
    }

    #[test]
    fn every_way_of_writing_a_code_hashes_the_same() {
        let canonical = release_code_hash(b"K7M29XQF4TBNR3WD");
        for input in ["K7M2-9XQF-4TBN-R3WD", "k7m2 9xqf 4tbn r3wd"] {
            assert_eq!(release_code_hash(&normalise(input).unwrap()), canonical);
        }
    }

    #[test]
    fn one_character_changes_the_hash() {
        assert_ne!(
            release_code_hash(b"K7M29XQF4TBNR3WD"),
            release_code_hash(b"K7M29XQF4TBNR3WE")
        );
    }
}
