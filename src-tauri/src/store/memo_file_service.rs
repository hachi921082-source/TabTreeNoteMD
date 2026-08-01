//! Shared memo-body encoding/EOL logic (`docs/spec.md` §2.6, §5.5; `docs/detailed-spec.md`
//! §5.3; `docs/plans/editor-save/PLAN.md` §2) — used by `commands/editor.rs`'s `editor_load`/
//! `editor_save`, and (encoding detection only) `commands/tree.rs`'s `memo_get_properties`
//! (`memo-tree/PLAN.md` §3), so both call sites share one implementation instead of drifting
//! apart over time (see `commands/tree.rs`'s original `decode_memo_body` doc comment, which
//! flagged this exact consolidation as `editor-save/PLAN.md`'s job once implemented).

use serde::Serialize;

/// Informational, for the editor's status bar + re-save policy (`editor-save/PLAN.md` §2's
/// `MemoContent.encoding`). Save always re-encodes as UTF-8 today regardless of this value
/// (`[spec §3.4]`, `AppSettings.encoding.defaultSave` is currently single-valued).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Encoding {
    #[serde(rename = "utf-8")]
    Utf8,
    #[serde(rename = "shift-jis")]
    ShiftJis,
}

/// Detected/target line-ending style (`editor-save/PLAN.md` §2's `MemoContent.eol`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Eol {
    #[serde(rename = "crlf")]
    Crlf,
    #[serde(rename = "lf")]
    Lf,
    #[serde(rename = "cr")]
    Cr,
}

/// Encoding detection order (`[detailed-spec §5.3]`): (1) UTF-8 BOM present -> decode as UTF-8,
/// strip BOM. (2) No BOM -> attempt strict UTF-8. (3) Invalid UTF-8 -> Shift_JIS fallback. Both
/// `encoding_rs::SHIFT_JIS::decode`'s own internal fallback (replacement characters for bytes it
/// can't map) and a plain empty/ASCII-only file are treated as successful UTF-8, never reaching
/// the Shift_JIS branch. The line endings in the returned text are left untouched (CRLF/CR may
/// still be present) — callers that need the LF-normalized editor buffer call
/// `normalize_to_lf` separately, since `detect_eol` needs to see the original line endings first.
pub fn decode_memo_body(bytes: &[u8]) -> (String, Encoding) {
    const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];
    if let Some(rest) = bytes.strip_prefix(&UTF8_BOM) {
        return (String::from_utf8_lossy(rest).into_owned(), Encoding::Utf8);
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => (s.to_string(), Encoding::Utf8),
        Err(_) => {
            let (decoded, _, _) = encoding_rs::SHIFT_JIS.decode(bytes);
            (decoded.into_owned(), Encoding::ShiftJis)
        }
    }
}

/// Dominant line-ending style in `text` (called on text that may still contain CRLF/CR, i.e.
/// before `normalize_to_lf`). Empty file / no line breaks at all -> `Crlf` (Windows platform
/// default, `[detailed-spec §5.3]`).
pub fn detect_eol(text: &str) -> Eol {
    let bytes = text.as_bytes();
    let (mut crlf, mut lf, mut cr) = (0usize, 0usize, 0usize);
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\r' if i + 1 < bytes.len() && bytes[i + 1] == b'\n' => {
                crlf += 1;
                i += 2;
                continue;
            }
            b'\r' => cr += 1,
            b'\n' => lf += 1,
            _ => {}
        }
        i += 1;
    }
    if crlf == 0 && lf == 0 && cr == 0 {
        Eol::Crlf
    } else if crlf >= lf && crlf >= cr {
        Eol::Crlf
    } else if lf >= cr {
        Eol::Lf
    } else {
        Eol::Cr
    }
}

/// Normalizes any CRLF/CR line endings to LF — the in-editor buffer's internal representation
/// (`editor-save/PLAN.md` §1: "CodeMirror 6 holds ... LF-normalized internally").
pub fn normalize_to_lf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Restores `eol` on an LF-normalized `text` (the editor buffer) before writing to disk
/// (`editor-save/PLAN.md` §2 `editor_save` step 3).
pub fn restore_eol(text: &str, eol: Eol) -> String {
    match eol {
        Eol::Lf => text.to_string(),
        Eol::Crlf => text.replace('\n', "\r\n"),
        Eol::Cr => text.replace('\n', "\r"),
    }
}

/// Re-encodes `text` per `AppSettings.encoding.defaultSave` (`editor-save/PLAN.md` §2
/// `editor_save` step 4). Only `"utf-8"` is a currently-valid value (`[spec §3.4]`); any other
/// string found in settings (nothing in this codebase writes one) falls back to UTF-8 rather
/// than failing the save outright, since no other encoder is implemented.
pub fn encode_for_save(text: &str, _default_save: &str) -> Vec<u8> {
    text.as_bytes().to_vec()
}

/// Hex-encoded SHA-256 of raw file bytes — the whole-file-bytes hash used for `editor_load`'s
/// `MemoContent.hash` and `editor_save`'s conflict check (`[spec §5.5]`, `editor-save/PLAN.md`
/// §2). Same algorithm as `watch/conflict.rs`'s composite check, but returned as a `String` here
/// since this hash crosses the IPC boundary to the frontend (`watch/conflict.rs`'s `[u8; 32]` is
/// backend-internal only).
pub fn hash_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_memo_body_handles_bom_and_shift_jis_fallback() {
        assert_eq!(decode_memo_body("hello".as_bytes()), ("hello".to_string(), Encoding::Utf8));

        let mut with_bom = vec![0xEF, 0xBB, 0xBF];
        with_bom.extend_from_slice("bom".as_bytes());
        assert_eq!(decode_memo_body(&with_bom), ("bom".to_string(), Encoding::Utf8));

        // Shift_JIS encoding of "あ" (0x82 0xA0) is invalid UTF-8, so this should fall back to
        // Shift_JIS decoding rather than erroring.
        let shift_jis_a = [0x82, 0xA0];
        assert_eq!(decode_memo_body(&shift_jis_a), ("あ".to_string(), Encoding::ShiftJis));
    }

    #[test]
    fn detect_eol_picks_dominant_style() {
        assert_eq!(detect_eol("a\r\nb\r\nc"), Eol::Crlf);
        assert_eq!(detect_eol("a\nb\nc"), Eol::Lf);
        assert_eq!(detect_eol("a\rb\rc"), Eol::Cr);
        assert_eq!(detect_eol(""), Eol::Crlf);
        assert_eq!(detect_eol("no line breaks at all"), Eol::Crlf);
    }

    #[test]
    fn detect_eol_does_not_double_count_crlf_as_cr_and_lf() {
        // A file that is entirely CRLF should be classified as Crlf, not as a tie between
        // (fake) Cr and Lf counts from the same bytes.
        assert_eq!(detect_eol("one\r\ntwo\r\nthree\r\n"), Eol::Crlf);
    }

    #[test]
    fn normalize_to_lf_collapses_crlf_and_bare_cr() {
        assert_eq!(normalize_to_lf("a\r\nb\rc\nd"), "a\nb\nc\nd");
    }

    #[test]
    fn restore_eol_round_trips_from_lf() {
        assert_eq!(restore_eol("a\nb\nc", Eol::Lf), "a\nb\nc");
        assert_eq!(restore_eol("a\nb\nc", Eol::Crlf), "a\r\nb\r\nc");
        assert_eq!(restore_eol("a\nb\nc", Eol::Cr), "a\rb\rc");
    }

    #[test]
    fn hash_bytes_is_deterministic_and_content_sensitive() {
        let a = hash_bytes(b"hello");
        let b = hash_bytes(b"hello");
        let c = hash_bytes(b"hello!");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 64); // 32 bytes, hex-encoded
    }

    #[test]
    fn encoding_and_eol_serialize_to_the_documented_wire_strings() {
        assert_eq!(serde_json::to_string(&Encoding::Utf8).unwrap(), "\"utf-8\"");
        assert_eq!(serde_json::to_string(&Encoding::ShiftJis).unwrap(), "\"shift-jis\"");
        assert_eq!(serde_json::to_string(&Eol::Crlf).unwrap(), "\"crlf\"");
        assert_eq!(serde_json::to_string(&Eol::Lf).unwrap(), "\"lf\"");
        assert_eq!(serde_json::to_string(&Eol::Cr).unwrap(), "\"cr\"");
    }
}
