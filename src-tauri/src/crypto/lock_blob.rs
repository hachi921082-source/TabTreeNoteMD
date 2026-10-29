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

use crate::fs::path_safety::{validate_name, ValidationMode};
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

/// One `imageFolders` entry (GitHub issue #271): a locked tab's memo name -> its per-memo image
/// folder's random name (`<tab_folder>/<folder>/NNN.ext`). Kept only inside the encrypted
/// payload so the real memo name is never derivable from the plain filesystem.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageFolderEntry {
    pub memo_name: String,
    pub folder: String,
}

/// GitHub issue #311: `token` は `obfuscated_naming::generate_token` の出力形式(小文字 16 進 32 文字)のみ許可する。
/// パス区切り・ドライブ指定・`..` などを含む値でタブフォルダ外へ `join` されるのを防ぐ。
pub fn is_valid_token(token: &str) -> bool {
    token.len() == 32 && token.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// GitHub issue #311: 画像フォルダ名は `obfuscated_naming::generate_image_folder_name` の出力形式
/// (16 進 16 文字)。生成は小文字だが、大文字小文字を区別しない FS 上の既存データを弾かないよう大文字も許す。
pub fn is_valid_image_folder_name(name: &str) -> bool {
    name.len() == 16 && name.bytes().all(|b| b.is_ascii_hexdigit())
}

/// GitHub issue #311: メモ名(`<メモ名>.md` の語幹)の検査。既存の `validate_name`(`Lenient`)と同じ基準:
/// 空・パス区切り・`:`・その他の禁止文字・制御文字・予約デバイス名を拒否する。
/// 既存ユーザーのメモ名(走査は `Lenient`)を弾かないよう、先頭/末尾の空白・末尾ピリオドは許す
/// (`.md` が付くため Win32 の正規化で削られることはなく、`<メモ名>.md` は常に 1 要素)。
/// ただし `.` / `..` の単独名は拒否する。
pub fn is_valid_memo_name(name: &str) -> bool {
    name != "." && name != ".." && validate_name(name, ValidationMode::Lenient).is_ok()
}

impl FileMapEntry {
    pub fn is_valid(&self) -> bool {
        is_valid_token(&self.token) && is_valid_memo_name(&self.memo_name)
    }
}

impl ImageFolderEntry {
    pub fn is_valid(&self) -> bool {
        is_valid_image_folder_name(&self.folder) && is_valid_memo_name(&self.memo_name)
    }
}

/// `fileMap` / `imageFolders` の全エントリが妥当なら `true`。1 件でも違反があれば全体を不正として扱う。
pub fn entries_are_valid(file_map: &[FileMapEntry], image_folders: &[ImageFolderEntry]) -> bool {
    file_map.iter().all(FileMapEntry::is_valid) && image_folders.iter().all(ImageFolderEntry::is_valid)
}

/// The decrypted "lock payload" (`tab-lock/PLAN.md` §2) — never written to disk in this shape,
/// only its encrypted form (`LockField::ciphertext_b64`) is persisted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockPayload {
    pub real_tab_name: String,
    pub tree: Vec<MetaTreeNode>,
    pub file_map: Vec<FileMapEntry>,
    /// `docs/plans/v1.1.0/tab-memo-memory/PLAN.md` §1 — the locked tab's own "last viewed memo"
    /// pointer, kept inside this encrypted payload (never a plaintext sibling of `.tabtreenote.
    /// json`'s `lock` field) so even *which* memo was last viewed isn't readable without the
    /// password. `#[serde(default)]` so a pre-v1.1.0 encrypted payload (written before this field
    /// existed) still decrypts cleanly, with this defaulting to `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_viewed_memo: Option<String>,
    /// GitHub issue #271: memo name -> random image-folder name map. スキーマ互換方針:
    /// - 追加フィールドのみ(`formatVersion` は 1 のまま据え置き)。`#[serde(default)]` により、
    ///   このフィールドを持たない既存のロック済みタブ(#271 以前に作成)はそのまま復号でき、空の
    ///   対応表として扱われる(移行処理なし)。
    /// - 旧ビルドは `formatVersion` を検査せず、payload の未知フィールドも無視して復号する
    ///   (`deny_unknown_fields` 不使用)ため、新しい blob を開いてもエラーにも破損にもならない。
    ///   ただし旧ビルドがセッション中に構造変更を保存すると、この対応表は再暗号化時に落ちる
    ///   (画像フォルダ・画像ファイル・本文中のリンクはそのまま残り、表示も変わらない。失われるのは
    ///   以後の削除時にフォルダをごみ箱へ送る追跡だけ)。`formatVersion` を上げても旧ビルドは検査
    ///   しないため防げず、逆に将来の互換判定を複雑にするだけなので上げない。
    /// 空のときは書き出さない(#271 以前と同じ JSON 形状を保つ)。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub image_folders: Vec<ImageFolderEntry>,
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
    let payload: LockPayload = serde_json::from_slice(&plaintext).map_err(|_| LockCryptoError::DecryptFailed)?;
    // GitHub issue #311: 細工された token / メモ名が 1 件でもあればペイロードごと拒否する(破損扱い)
    if !entries_are_valid(&payload.file_map, &payload.image_folders) {
        return Err(LockCryptoError::DecryptFailed);
    }
    Ok(payload)
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
            last_viewed_memo: None,
            image_folders: vec![],
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

    /// `docs/plans/v1.1.0/tab-memo-memory/PLAN.md` §1: `last_viewed_memo` round-trips through the
    /// encrypted payload like every other field.
    #[test]
    fn last_viewed_memo_round_trips_through_the_encrypted_payload() {
        let key = LockKey::new(&[3u8; 32]);
        let mut payload = sample_payload();
        payload.last_viewed_memo = Some("Diary 2026-07".to_string());
        let field = encrypt_lock_field(&key, KdfParams::generate_new(), None, &payload).unwrap();
        let decrypted = decrypt_lock_field(&key, &field).unwrap();
        assert_eq!(decrypted.last_viewed_memo.as_deref(), Some("Diary 2026-07"));
    }

    /// A pre-v1.1.0 encrypted payload (no `lastViewedMemo` key at all) must still decrypt cleanly,
    /// defaulting to `None` — `#[serde(default)]` on the field.
    #[test]
    fn pre_v1_1_0_payload_with_no_last_viewed_memo_field_decrypts_as_none() {
        let key = LockKey::new(&[3u8; 32]);
        let legacy_json = serde_json::json!({
            "realTabName": "Private",
            "tree": [],
            "fileMap": [],
        });
        let bytes = serde_json::to_vec(&legacy_json).unwrap();
        let (iv, ciphertext_with_tag) = key.encrypt_bytes(&bytes);
        let split_at = ciphertext_with_tag.len() - GCM_TAG_LEN;
        let (ciphertext, tag) = ciphertext_with_tag.split_at(split_at);
        let field = LockField {
            format_version: LOCK_FORMAT_VERSION,
            kdf: KdfParams::generate_new(),
            cipher: CipherParams {
                algo: CIPHER_ALGO.to_string(),
                iv_b64: BASE64.encode(iv),
                tag_b64: BASE64.encode(tag),
            },
            hint: None,
            ciphertext_b64: BASE64.encode(ciphertext),
        };
        let decrypted = decrypt_lock_field(&key, &field).unwrap();
        assert_eq!(decrypted.last_viewed_memo, None);
    }

    /// GitHub issue #271: 日本語メモ名 -> ランダムフォルダ名の対応表が暗号化 payload を往復する。
    #[test]
    fn image_folders_map_round_trips_through_the_encrypted_payload() {
        let key = LockKey::new(&[3u8; 32]);
        let mut payload = sample_payload();
        payload.image_folders = vec![
            ImageFolderEntry { memo_name: "日記".to_string(), folder: "a3f9c2d4e5f60718".to_string() },
            ImageFolderEntry { memo_name: "買い物メモ".to_string(), folder: "0123456789abcdef".to_string() },
        ];
        let field = encrypt_lock_field(&key, KdfParams::generate_new(), None, &payload).unwrap();
        // 平文の lock フィールド側には対応表(実メモ名)が一切現れない
        let plain_json = serde_json::to_string(&field).unwrap();
        assert!(!plain_json.contains("日記") && !plain_json.contains("imageFolders"));
        let decrypted = decrypt_lock_field(&key, &field).unwrap();
        assert_eq!(decrypted.image_folders, payload.image_folders);
        assert_eq!(decrypted, payload);
    }

    /// GitHub issue #271: 対応表を持たない既存(旧形式)の payload は空の対応表として復号できる。
    #[test]
    fn legacy_payload_without_image_folders_decrypts_as_empty_map() {
        let key = LockKey::new(&[3u8; 32]);
        let legacy_json = serde_json::json!({
            "realTabName": "非公開",
            "tree": [],
            "fileMap": [{ "memoName": "日記", "token": "9f2c7a1e4b3d0f8c6a5e2d1b0c9a8f7e" }],
            "lastViewedMemo": "日記",
        });
        let bytes = serde_json::to_vec(&legacy_json).unwrap();
        let (iv, ciphertext_with_tag) = key.encrypt_bytes(&bytes);
        let split_at = ciphertext_with_tag.len() - GCM_TAG_LEN;
        let (ciphertext, tag) = ciphertext_with_tag.split_at(split_at);
        let field = LockField {
            format_version: LOCK_FORMAT_VERSION,
            kdf: KdfParams::generate_new(),
            cipher: CipherParams {
                algo: CIPHER_ALGO.to_string(),
                iv_b64: BASE64.encode(iv),
                tag_b64: BASE64.encode(tag),
            },
            hint: None,
            ciphertext_b64: BASE64.encode(ciphertext),
        };
        let decrypted = decrypt_lock_field(&key, &field).unwrap();
        assert!(decrypted.image_folders.is_empty());
        assert_eq!(decrypted.file_map.len(), 1);
    }

    /// GitHub issue #271: 空の対応表は JSON に書き出さない(#271 以前と同じ形状)。
    #[test]
    fn empty_image_folders_map_is_omitted_from_the_plaintext_payload_json() {
        let json = serde_json::to_value(sample_payload()).unwrap();
        assert!(json.get("imageFolders").is_none());
    }

    // GitHub issue #311: 細工された fileMap / imageFolders の拒否
    const GOOD_TOKEN: &str = "9f2c7a1e4b3d0f8c6a5e2d1b0c9a8f7e";

    #[test]
    fn token_accepts_only_32_lowercase_hex() {
        assert!(is_valid_token(GOOD_TOKEN));
        assert!(is_valid_token(&crate::crypto::obfuscated_naming::generate_token()));
        for bad in [
            "",
            "abc123",
            r"C:\Users\x\y.docx",
            r"..\..\x",
            "C:x",
            "foo/bar",
            "a:b",
            "CON",
            "9F2C7A1E4B3D0F8C6A5E2D1B0C9A8F7E",
            "9f2c7a1e4b3d0f8c6a5e2d1b0c9a8f7",
            "9f2c7a1e4b3d0f8c6a5e2d1b0c9a8f7e0",
            "9f2c7a1e4b3d0f8c6a5e2d1b0c9a8f7g",
        ] {
            assert!(!is_valid_token(bad), "{bad:?} must be rejected");
        }
    }

    #[test]
    fn memo_name_accepts_real_world_names_and_rejects_path_tricks() {
        for ok in ["日記", "Diary 2026-07", "買い物 メモ", "a.b", "memo(1)", "#tag & more", " lead", "trail ", "名前."] {
            assert!(is_valid_memo_name(ok), "{ok:?} must be accepted");
        }
        for bad in ["", ".", "..", r"C:\Users\x\y.docx", r"..\..\x", "C:x", "foo/bar", "a:b", "CON", "con", "nul.txt", "a\u{0007}b", "x|y", "a*b"] {
            assert!(!is_valid_memo_name(bad), "{bad:?} must be rejected");
        }
    }

    #[test]
    fn image_folder_name_accepts_only_16_hex() {
        assert!(is_valid_image_folder_name("a3f9c2d4e5f60718"));
        assert!(is_valid_image_folder_name(&crate::crypto::obfuscated_naming::generate_image_folder_name()));
        for bad in ["", "..", "a3f9c2d4e5f6071", "a3f9c2d4e5f607181", r"C:\x", r"..\..\xxxxxxxx", "a3f9c2d4e5f6071g"] {
            assert!(!is_valid_image_folder_name(bad), "{bad:?} must be rejected");
        }
    }

    #[test]
    fn decrypt_lock_field_rejects_the_whole_payload_on_any_bad_entry() {
        let key = LockKey::new(&[3u8; 32]);
        let kdf = KdfParams::generate_new();
        let bad_entries = [
            FileMapEntry { memo_name: "readme".into(), token: r"C:\Users\x\y.docx".into() },
            FileMapEntry { memo_name: "readme".into(), token: r"..\..\x".into() },
            FileMapEntry { memo_name: r"..\..\x".into(), token: GOOD_TOKEN.into() },
            FileMapEntry { memo_name: "CON".into(), token: GOOD_TOKEN.into() },
            FileMapEntry { memo_name: "a:b".into(), token: GOOD_TOKEN.into() },
        ];
        for bad in bad_entries {
            let mut payload = sample_payload();
            payload.file_map.push(bad);
            let field = encrypt_lock_field(&key, kdf.clone(), None, &payload).unwrap();
            assert!(matches!(decrypt_lock_field(&key, &field), Err(LockCryptoError::DecryptFailed)));
        }
        let mut payload = sample_payload();
        payload.image_folders.push(ImageFolderEntry { memo_name: "Diary 2026-07".into(), folder: "..".into() });
        let field = encrypt_lock_field(&key, kdf.clone(), None, &payload).unwrap();
        assert!(matches!(decrypt_lock_field(&key, &field), Err(LockCryptoError::DecryptFailed)));

        let mut payload = sample_payload();
        payload.image_folders.push(ImageFolderEntry { memo_name: "C:x".into(), folder: "a3f9c2d4e5f60718".into() });
        let field = encrypt_lock_field(&key, kdf, None, &payload).unwrap();
        assert!(matches!(decrypt_lock_field(&key, &field), Err(LockCryptoError::DecryptFailed)));
    }
}
