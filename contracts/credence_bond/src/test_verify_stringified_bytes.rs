//! Tests for `verify_stringified_bytes`.
//!
//! Covers four categories:
//! * **Valid** — well-formed printable-ASCII byte strings that should pass.
//! * **Malformed** — byte sequences containing non-printable / high-byte octets.
//! * **Oversized** — payloads that exceed `MAX_STRINGIFIED_BYTES_LEN`.
//! * **Injected-null** — payloads that embed a NUL (`\x00`) byte.

#![cfg(test)]

extern crate std;

use soroban_sdk::{Bytes, Env};

use crate::validation::{verify_stringified_bytes, MAX_STRINGIFIED_BYTES_LEN};

// ─── helpers ─────────────────────────────────────────────────────────────────

/// Build a `Bytes` filled with `count` copies of `byte`.
fn repeat_byte(e: &Env, byte: u8, count: usize) -> Bytes {
    let vec: std::vec::Vec<u8> = core::iter::repeat(byte).take(count).collect();
    Bytes::from_slice(e, &vec)
}

// ─── Valid cases ──────────────────────────────────────────────────────────────

/// A single printable ASCII character is the shortest valid input.
#[test]
fn valid_single_printable_char() {
    let e = Env::default();
    let data = Bytes::from_slice(&e, b"x");
    verify_stringified_bytes(&e, &data); // must not panic
}

/// A 64-character lowercase hex string — the canonical shape of a SHA-256 digest.
#[test]
fn valid_sha256_hex_digest() {
    let e = Env::default();
    let data = Bytes::from_slice(
        &e,
        b"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    );
    verify_stringified_bytes(&e, &data);
}

/// A 128-character lowercase hex string (SHA-512 digest).
#[test]
fn valid_sha512_hex_digest() {
    let e = Env::default();
    let digest = b"cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce\
                   47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e";
    let data = Bytes::from_slice(&e, digest);
    verify_stringified_bytes(&e, &data);
}

/// An IPFS CIDv0 (base58 / printable ASCII). Verifies that the full set of
/// printable ASCII characters in a realistic hash reference is accepted.
#[test]
fn valid_ipfs_cid() {
    let e = Env::default();
    let data = Bytes::from_slice(&e, b"QmXoypizjW3WknFiJnKLwHCnL72vedxjQkDDP1mXWo6uco");
    verify_stringified_bytes(&e, &data);
}

/// Exactly `MAX_STRINGIFIED_BYTES_LEN` printable bytes must be accepted (boundary).
#[test]
fn valid_exactly_max_length() {
    let e = Env::default();
    let data = repeat_byte(&e, b'a', MAX_STRINGIFIED_BYTES_LEN as usize);
    verify_stringified_bytes(&e, &data);
}

/// Printable ASCII space character (`0x20`) is the lower boundary of the allowed range.
#[test]
fn valid_space_character_boundary() {
    let e = Env::default();
    let data = Bytes::from_slice(&e, b" ");
    verify_stringified_bytes(&e, &data);
}

/// Tilde (`~`, `0x7E`) is the upper boundary of the printable ASCII range.
#[test]
fn valid_tilde_character_boundary() {
    let e = Env::default();
    let data = Bytes::from_slice(&e, b"~");
    verify_stringified_bytes(&e, &data);
}

// ─── Malformed cases ──────────────────────────────────────────────────────────

/// A byte value of `0x80` — the first byte outside printable ASCII — must be rejected.
#[test]
#[should_panic(expected = "stringified bytes contain a non-printable byte")]
fn malformed_high_byte_0x80() {
    let e = Env::default();
    let data = Bytes::from_slice(&e, &[0x68, 0x65, 0x6c, 0x6c, 0x6f, 0x80]); // "hello" + 0x80
    verify_stringified_bytes(&e, &data);
}

/// A lone byte `0xFF` (all-bits-set) — invalid in any ASCII / UTF-8 context.
#[test]
#[should_panic(expected = "stringified bytes contain a non-printable byte")]
fn malformed_byte_0xff() {
    let e = Env::default();
    let data = Bytes::from_slice(&e, &[0xff]);
    verify_stringified_bytes(&e, &data);
}

/// A DEL character (`0x7F`) sits just above the printable range and must be rejected.
#[test]
#[should_panic(expected = "stringified bytes contain a non-printable byte")]
fn malformed_del_character_0x7f() {
    let e = Env::default();
    let data = Bytes::from_slice(&e, b"valid_prefix\x7f");
    verify_stringified_bytes(&e, &data);
}

/// A non-printable control character (`\r`, `0x0D`) embedded in otherwise-valid text.
#[test]
#[should_panic(expected = "stringified bytes contain a non-printable byte")]
fn malformed_carriage_return_control_byte() {
    let e = Env::default();
    let data = Bytes::from_slice(&e, b"line1\rline2");
    verify_stringified_bytes(&e, &data);
}

/// A newline character (`\n`, `0x0A`) — below the printable ASCII floor of `0x20`.
#[test]
#[should_panic(expected = "stringified bytes contain a non-printable byte")]
fn malformed_newline_control_byte() {
    let e = Env::default();
    let data = Bytes::from_slice(&e, b"data\ninjection");
    verify_stringified_bytes(&e, &data);
}

/// Multi-byte UTF-8 sequence (`é` encodes as `0xC3 0xA9`). These bytes sit in
/// the high range and must be rejected because we enforce printable ASCII only.
#[test]
#[should_panic(expected = "stringified bytes contain a non-printable byte")]
fn malformed_multibyte_utf8_sequence() {
    let e = Env::default();
    // "café" in UTF-8: b"caf\xc3\xa9"
    let data = Bytes::from_slice(&e, &[0x63, 0x61, 0x66, 0xc3, 0xa9]);
    verify_stringified_bytes(&e, &data);
}

// ─── Oversized cases ──────────────────────────────────────────────────────────

/// One byte past the maximum must be rejected.
#[test]
#[should_panic(expected = "stringified bytes too long")]
fn oversized_one_byte_above_limit() {
    let e = Env::default();
    let data = repeat_byte(&e, b'a', MAX_STRINGIFIED_BYTES_LEN as usize + 1);
    verify_stringified_bytes(&e, &data);
}

/// A much-larger payload (double the limit) is also rejected.
#[test]
#[should_panic(expected = "stringified bytes too long")]
fn oversized_double_the_limit() {
    let e = Env::default();
    let data = repeat_byte(&e, b'x', MAX_STRINGIFIED_BYTES_LEN as usize * 2);
    verify_stringified_bytes(&e, &data);
}

/// Size check is evaluated before content checks: an oversized payload of
/// otherwise-invalid bytes must still fail with the size error, not a
/// content error, ensuring predictable error ordering.
#[test]
#[should_panic(expected = "stringified bytes too long")]
fn oversized_takes_priority_over_malformed() {
    let e = Env::default();
    // All bytes are 0xFF (malformed), but the length check should fire first.
    let data = repeat_byte(&e, 0xff, MAX_STRINGIFIED_BYTES_LEN as usize + 1);
    verify_stringified_bytes(&e, &data);
}

// ─── Injected-null cases ──────────────────────────────────────────────────────

/// A lone NUL byte by itself must be caught.
#[test]
#[should_panic(expected = "stringified bytes contain a null byte")]
fn injected_null_single_nul_byte() {
    let e = Env::default();
    let data = Bytes::from_slice(&e, b"\x00");
    verify_stringified_bytes(&e, &data);
}

/// A NUL byte embedded in the middle of otherwise-valid text — the classic
/// null-injection attack vector.
#[test]
#[should_panic(expected = "stringified bytes contain a null byte")]
fn injected_null_in_middle_of_valid_string() {
    let e = Env::default();
    let data = Bytes::from_slice(&e, b"abc\x00def");
    verify_stringified_bytes(&e, &data);
}

/// NUL at the very beginning of the payload.
#[test]
#[should_panic(expected = "stringified bytes contain a null byte")]
fn injected_null_at_start() {
    let e = Env::default();
    let data = Bytes::from_slice(&e, b"\x00validprefix");
    verify_stringified_bytes(&e, &data);
}

/// NUL at the very end of the payload — simulates a C-style null-terminator
/// appended to an otherwise valid string.
#[test]
#[should_panic(expected = "stringified bytes contain a null byte")]
fn injected_null_at_end() {
    let e = Env::default();
    let data = Bytes::from_slice(&e, b"validprefix\x00");
    verify_stringified_bytes(&e, &data);
}

/// Multiple NUL bytes scattered through the payload.
#[test]
#[should_panic(expected = "stringified bytes contain a null byte")]
fn injected_null_multiple_nuls() {
    let e = Env::default();
    let data = Bytes::from_slice(&e, b"a\x00b\x00c");
    verify_stringified_bytes(&e, &data);
}

/// Null-byte check fires even when the payload also has a high byte later.
/// The null message should appear because nulls are tested first in the byte
/// iteration order (null appears before the high byte in this sequence).
#[test]
#[should_panic(expected = "stringified bytes contain a null byte")]
fn injected_null_before_high_byte() {
    let e = Env::default();
    // NUL comes before 0xFF in the payload
    let data = Bytes::from_slice(&e, &[0x61, 0x00, 0xff]); // 'a', NUL, 0xFF
    verify_stringified_bytes(&e, &data);
}
