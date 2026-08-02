//! AES-256-GCM encryption for the lock blob (`.tabtreenote.json`'s `lock` field) and for
//! individual obfuscated memo-body files (`docs/plans/tab-lock/PLAN.md` §2).
//!
//! **Obfuscated memo-body on-disk layout** (this module's own call, per `tab-lock/PLAN.md` §2:
//! "document the chosen layout inline in `crypto/lock_blob.rs`'s doc comment once decided"):
//! `[12-byte IV][ciphertext][16-byte GCM tag]`, IV and tag concatenated directly around/after the
//! ciphertext with no separate version header. A version header is unnecessary here because the
//! containing tab's `lock.formatVersion` (in `.tabtreenote.json`) already versions the whole locked
//! tab's on-disk shape, including this per-file layout; a mismatch would need a tab-wide migration
//! step regardless of a per-file marker. The GCM tag has a fixed 16-byte length, so it is always
//! unambiguously the last 16 bytes of the ciphertext-plus-tag AEAD output.
//!
//! The **lock blob** itself (a JSON object) instead keeps `iv`/`tag`/`ciphertext` as three separate
//! base64 fields per the schema fixed in `tab-lock/PLAN.md` §2 (`cipher.ivB64`, `cipher.tagB64`,
//! top-level `ciphertextB64`) — this module splits the same underlying AEAD ciphertext+tag output
//! to populate those three fields, and re-concatenates them the same way on decrypt.

use aes_gcm::aead::{Aead, AeadCore, KeyInit};
use aes_gcm::Aes256Gcm;

/// AES-256-GCM's 96-bit nonce type, fixed by the algorithm — pinned to a concrete alias here
/// rather than repeating `aes_gcm::Nonce<<Aes256Gcm as AeadCore>::NonceSize>` at each call site.
type GcmNonce = aes_gcm::Nonce<<Aes256Gcm as AeadCore>::NonceSize>;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use rand::RngExt;
use serde::{Deserialize, Serialize};

use crate::model::tree::MetaTreeNode;

use super::kdf::KdfParams;

/// GCM tag length in bytes (fixed for AES-256-GCM).
const GCM_TAG_LEN: usize = 16;
/// GCM nonce/IV length in bytes (fixed at 96 bits, `tab-lock/PLAN.md` §2: "fresh random 12-byte IV
/// per save").
const IV_LEN: usize = 12;

pub const LOCK_FORMAT_VERSION: u32 = 1;
pub const CIPHER_ALGO: &str = "aes-256-gcm";

#[derive(Debug)]
pub enum LockCryptoError {
    /// Auth-tag verification failed, or the stored bytes were too malformed to even attempt
    /// decryption (malformed base64, wrong-length IV, truncated ciphertext, etc.) — collapsed into
    /// one variant deliberately, since `tab-lock/PLAN.md` §3 `tab_unlock` step 3 requires "no
    /// distinguishing-error-message side channel" between a wrong password and a corrupted blob.
    DecryptFailed,
    /// JSON (de)serialization of the plaintext lock payload failed.
    Serialize,
}

/// One `fileMap` entry (`tab-lock/PLAN.md` §2's decrypted lock payload shape, and
/// `tab-lock/CRASH-RECOVERY.md`'s marker `fileMap`). `token` is a 128-bit random hex string
/// (`crypto/obfuscated_naming.rs`), the obfuscated on-disk file name with no extension.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMapEntry {
    pub memo_name: String,
    pub token: String,
}

/// The decrypted "lock payload" (`tab-lock/PLAN.md` §2) — never written to disk in this shape,
/// only its encrypted form (`LockField::ciphertext_b64`) is persisted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockPayload {
    pub real_tab_name: String,
    pub tree: Vec<MetaTreeNode>,
    pub file_map: Vec<FileMapEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CipherParams {
    pub algo: String,
    pub iv_b64: String,
    pub tag_b64: String,
}

/// `.tabtreenote.json`'s `lock` field shape (`tab-lock/PLAN.md` §2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockField {
    pub format_version: u32,
    pub kdf: KdfParams,
    pub cipher: CipherParams,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    pub ciphertext_b64: String,
}

/// A derived AES-256 key, ready to encrypt/decrypt any number of independent messages (lock blob,
/// or any number of memo bodies) — `tab-lock/PLAN.md` §2: "key never re-derived per save (derive
/// once per session, reuse for all saves in that session)".
pub struct LockKey(Aes256Gcm);

impl LockKey {
    pub fn new(key_bytes: &[u8; 32]) -> Self {
        LockKey(Aes256Gcm::new_from_slice(key_bytes).expect("AES-256-GCM key must be exactly 32 bytes"))
    }

    /// Encrypts `plaintext` with a fresh random IV (`tab-lock/PLAN.md` §2: "IV: always fresh per
    /// write"). Returns `(iv, ciphertext_with_trailing_tag)`.
    fn encrypt_bytes(&self, plaintext: &[u8]) -> ([u8; IV_LEN], Vec<u8>) {
        let mut iv = [0u8; IV_LEN];
        rand::rng().fill(&mut iv);
        let nonce: GcmNonce = iv.into();
        let ciphertext_with_tag = self.0.encrypt(&nonce, plaintext).expect("AES-GCM encryption cannot fail for in-memory buffers");
        (iv, ciphertext_with_tag)
    }

    /// Decrypts `ciphertext_with_tag` (as produced by `encrypt_bytes`) using `iv`. Any failure —
    /// wrong key/password, corrupted bytes, or a truncated/malformed input — is collapsed into
    /// `LockCryptoError::DecryptFailed`, matching `tab-lock/PLAN.md` §3 `tab_unlock` step 3's "no
    /// distinguishing-error-message side channel" requirement.
    fn decrypt_bytes(&self, iv: &[u8; IV_LEN], ciphertext_with_tag: &[u8]) -> Result<Vec<u8>, LockCryptoError> {
        let nonce: GcmNonce = (*iv).into();
        self.0.decrypt(&nonce, ciphertext_with_tag).map_err(|_| LockCryptoError::DecryptFailed)
    }
}

/// Encrypts an obfuscated memo body (`tab-lock/PLAN.md` §2: "Each obfuscated memo file's content:
/// independently AES-256-GCM encrypted, fresh random 12-byte IV per save, same password-derived key
/// as the lock blob"). Returns the exact bytes to write to the token-named file.
pub fn encrypt_memo_body(key: &LockKey, plaintext: &[u8]) -> Vec<u8> {
    let (iv, ciphertext_with_tag) = key.encrypt_bytes(plaintext);
    let mut out = Vec::with_capacity(IV_LEN + ciphertext_with_tag.len());
    out.extend_from_slice(&iv);
    out.extend_from_slice(&ciphertext_with_tag);
    out
}

/// Decrypts bytes read from an obfuscated memo-body file (as produced by `encrypt_memo_body`).
pub fn decrypt_memo_body(key: &LockKey, bytes: &[u8]) -> Result<Vec<u8>, LockCryptoError> {
    if bytes.len() < IV_LEN + GCM_TAG_LEN {
        return Err(LockCryptoError::DecryptFailed);
    }
    let (iv_slice, ciphertext_with_tag) = bytes.split_at(IV_LEN);
    let iv: [u8; IV_LEN] = iv_slice.try_into().expect("split_at(IV_LEN) guarantees this length");
    key.decrypt_bytes(&iv, ciphertext_with_tag)
}

/// Builds an encrypted `LockField` from a plaintext `LockPayload` (`tab-lock/PLAN.md` §3 `tab_lock`
/// step 8, and the "Finish" recovery branch in `tab-lock/CRASH-RECOVERY.md`). `kdf`/`hint` are
/// threaded through by the caller (the same `kdf` used for the in-progress marker, reused verbatim
/// per `CRASH-RECOVERY.md`'s "Marker File Format" note).
pub fn encrypt_lock_field(key: &LockKey, kdf: KdfParams, hint: Option<String>, payload: &LockPayload) -> Result<LockField, LockCryptoError> {
    let json = serde_json::to_vec(payload).map_err(|_| LockCryptoError::Serialize)?;
    let (iv, ciphertext_with_tag) = key.encrypt_bytes(&json);
    if ciphertext_with_tag.len() < GCM_TAG_LEN {
        return Err(LockCryptoError::Serialize);
    }
    let split_at = ciphertext_with_tag.len() - GCM_TAG_LEN;
    let (ciphertext, tag) = ciphertext_with_tag.split_at(split_at);

    Ok(LockField {
        format_version: LOCK_FORMAT_VERSION,
        kdf,
        cipher: CipherParams {
            algo: CIPHER_ALGO.to_string(),
            iv_b64: BASE64.encode(iv),
            tag_b64: BASE64.encode(tag),
        },
        hint,
        ciphertext_b64: BASE64.encode(ciphertext),
    })
}

/// Decrypts a `LockField` back into its plaintext `LockPayload` (`tab-lock/PLAN.md` §3 `tab_unlock`
/// steps 2-3). Any failure (wrong password, corrupted field, malformed base64) is collapsed into
/// `LockCryptoError::DecryptFailed` — see `LockKey::decrypt_bytes`'s doc comment.
pub fn decrypt_lock_field(key: &LockKey, field: &LockField) -> Result<LockPayload, LockCryptoError> {
    let iv_bytes = BASE64.decode(&field.cipher.iv_b64).map_err(|_| LockCryptoError::DecryptFailed)?;
    let iv: [u8; IV_LEN] = iv_bytes.try_into().map_err(|_| LockCryptoError::DecryptFailed)?;
    let tag = BASE64.decode(&field.cipher.tag_b64).map_err(|_| LockCryptoError::DecryptFailed)?;
    let ciphertext = BASE64.decode(&field.ciphertext_b64).map_err(|_| LockCryptoError::DecryptFailed)?;

    let mut ciphertext_with_tag = ciphertext;
    ciphertext_with_tag.extend_from_slice(&tag);

    let plaintext = key.decrypt_bytes(&iv, &ciphertext_with_tag)?;
    serde_json::from_slice(&plaintext).map_err(|_| LockCryptoError::DecryptFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_payload() -> LockPayload {
        LockPayload {
            real_tab_name: "Private".to_string(),
            tree: vec![MetaTreeNode::default_for_new_file("Diary 2026-07".to_string())],
            file_map: vec![FileMapEntry {
                memo_name: "Diary 2026-07".to_string(),
                token: "9f2c7a1e4b3d0f8c6a5e2d1b0c9a8f7e".to_string(),
            }],
        }
    }

    #[test]
    fn memo_body_round_trips() {
        let key = LockKey::new(&[7u8; 32]);
        let plaintext = b"buy milk\nbuy eggs\n";
        let encrypted = encrypt_memo_body(&key, plaintext);
        assert!(encrypted.len() >= IV_LEN + GCM_TAG_LEN);
        let decrypted = decrypt_memo_body(&key, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn memo_body_fresh_iv_each_call_even_for_identical_plaintext() {
        let key = LockKey::new(&[7u8; 32]);
        let a = encrypt_memo_body(&key, b"same content");
        let b = encrypt_memo_body(&key, b"same content");
        assert_ne!(a, b, "IV must be fresh per save even for identical content");
    }

    #[test]
    fn memo_body_wrong_key_fails_to_decrypt() {
        let key = LockKey::new(&[7u8; 32]);
        let wrong_key = LockKey::new(&[8u8; 32]);
        let encrypted = encrypt_memo_body(&key, b"secret");
        assert!(matches!(decrypt_memo_body(&wrong_key, &encrypted), Err(LockCryptoError::DecryptFailed)));
    }

    #[test]
    fn memo_body_truncated_bytes_fail_gracefully() {
        let key = LockKey::new(&[7u8; 32]);
        assert!(matches!(decrypt_memo_body(&key, b"short"), Err(LockCryptoError::DecryptFailed)));
    }

    #[test]
    fn lock_field_round_trips_and_carries_the_documented_wire_shape() {
        let key = LockKey::new(&[3u8; 32]);
        let kdf = KdfParams::generate_new();
        let payload = sample_payload();

        let field = encrypt_lock_field(&key, kdf, Some("my hint".to_string()), &payload).unwrap();
        let json = serde_json::to_value(&field).unwrap();
        assert_eq!(json["formatVersion"], 1);
        assert_eq!(json["cipher"]["algo"], "aes-256-gcm");
        assert!(json["cipher"]["ivB64"].is_string());
        assert!(json["cipher"]["tagB64"].is_string());
        assert!(json["ciphertextB64"].is_string());
        assert_eq!(json["hint"], "my hint");

        let decrypted = decrypt_lock_field(&key, &field).unwrap();
        assert_eq!(decrypted, payload);
    }

    #[test]
    fn lock_field_no_hint_omits_the_field_from_json() {
        let key = LockKey::new(&[3u8; 32]);
        let field = encrypt_lock_field(&key, KdfParams::generate_new(), None, &sample_payload()).unwrap();
        let json = serde_json::to_value(&field).unwrap();
        assert!(json.get("hint").is_none());
    }

    #[test]
    fn lock_field_wrong_password_derived_key_fails_to_decrypt() {
        let key = LockKey::new(&[3u8; 32]);
        let wrong_key = LockKey::new(&[4u8; 32]);
        let field = encrypt_lock_field(&key, KdfParams::generate_new(), None, &sample_payload()).unwrap();
        assert!(matches!(decrypt_lock_field(&wrong_key, &field), Err(LockCryptoError::DecryptFailed)));
    }
}
