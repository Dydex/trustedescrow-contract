//! Reference implementation of the TrustEscrow delivery code.
//!
//! A delivery code is 80 bits of entropy written as 16 characters of
//! Crockford base32 and displayed in groups of four, `K7M2-9XQF-4TBN-R3WD`.
//! The escrow stores the SHA-256 of the 16 canonical ASCII characters and
//! releases when someone presents a code whose hash matches.
//!
//! The hash is public on-chain, so the code's only protection against offline
//! brute force is its entropy. Every client that generates or accepts codes
//! (the SDK, wallets, a CLI) must produce exactly the bytes this crate does.

mod hash;
mod normalise;

pub use hash::release_code_hash;
pub use normalise::{is_canonical, normalise, CodeError};

/// Crockford base32: digits and uppercase letters without I, L, O and U.
pub const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Random bytes behind one code: 80 bits.
pub const ENTROPY_BYTES: usize = 10;

/// Characters in a canonical code. 80 bits / 5 bits per character.
pub const CODE_LEN: usize = 16;

/// Encode 80 bits of entropy as a canonical 16-character code, most
/// significant bits first.
///
/// The caller supplies the entropy, which must come from a CSPRNG. This crate
/// deliberately does no randomness of its own.
pub fn encode(entropy: &[u8; ENTROPY_BYTES]) -> [u8; CODE_LEN] {
    let bits = entropy
        .iter()
        .fold(0u128, |acc, byte| (acc << 8) | u128::from(*byte));
    let mut code = [0u8; CODE_LEN];
    for (i, slot) in code.iter_mut().enumerate() {
        let shift = 5 * (CODE_LEN - 1 - i);
        *slot = ALPHABET[((bits >> shift) & 0x1f) as usize];
    }
    code
}

/// The display form: four groups of four separated by hyphens.
pub fn display(code: &[u8; CODE_LEN]) -> String {
    let mut out = String::with_capacity(CODE_LEN + 3);
    for (i, c) in code.iter().enumerate() {
        if i > 0 && i % 4 == 0 {
            out.push('-');
        }
        out.push(char::from(*c));
    }
    out
}

#[cfg(test)]
mod test {
    use super::*;

    fn encoded(entropy: [u8; ENTROPY_BYTES]) -> String {
        String::from_utf8(encode(&entropy).to_vec()).unwrap()
    }

    #[test]
    fn extremes_map_to_first_and_last_characters() {
        assert_eq!(encoded([0; 10]), "0000000000000000");
        assert_eq!(encoded([0xff; 10]), "ZZZZZZZZZZZZZZZZ");
    }

    #[test]
    fn bits_are_encoded_most_significant_first() {
        // Top bit set: the first character carries 0b10000 = 16 = 'G'.
        let mut top = [0; 10];
        top[0] = 0x80;
        assert_eq!(encoded(top), "G000000000000000");

        // Lowest bit set: only the last character changes.
        let mut low = [0; 10];
        low[9] = 0x01;
        assert_eq!(encoded(low), "0000000000000001");

        // 0x20 = 32 = 1 * 32 + 0 spills into the second-to-last character.
        let mut spill = [0; 10];
        spill[9] = 0x20;
        assert_eq!(encoded(spill), "0000000000000010");
    }

    #[test]
    fn output_never_contains_ambiguous_letters() {
        for seed in 0u8..=255 {
            let entropy = core::array::from_fn(|i| seed.wrapping_mul(31).wrapping_add(i as u8));
            let code = encode(&entropy);
            assert!(code.iter().all(|c| ALPHABET.contains(c)));
            assert!(!code.iter().any(|c| b"ILOU".contains(c)));
        }
    }

    #[test]
    fn display_groups_in_fours() {
        assert_eq!(display(b"K7M29XQF4TBNR3WD"), "K7M2-9XQF-4TBN-R3WD");
    }
}
