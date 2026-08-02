//! Password -> AES-256 key derivation via scrypt (`docs/plans/tab-lock/PLAN.md` §2).
//!
//! `KdfParams` mirrors the on-disk `kdf` object verbatim (both the eventual `lock.kdf` field and
//! the crash-recovery marker's own `kdf` field, `docs/plans/tab-lock/CRASH-RECOVERY.md` — the same
//! salt/params generated once at `tab_lock` step 5 are reused for both, per that file's own note).
//! Params are always read from the file at decrypt time, never hardcoded, so a future parameter
//! increase never breaks an already-locked tab (`tab-lock/PLAN.md` §2).

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use rand::RngExt;
use scrypt::{scrypt, Params};
use serde::{Deserialize, Serialize};

/// New locks always use these params (`tab-lock/PLAN.md` §2: "New locks: `N=32768, r=8, p=1,
/// keyLen=32` (~32MB memory cost)"). `N = 2^CURRENT_LOG_N`.
const CURRENT_LOG_N: u8 = 15; // 2^15 = 32768
const CURRENT_R: u32 = 8;
const CURRENT_P: u32 = 1;
/// AES-256 key length in bytes. Every `KdfParams` this codebase generates sets `keyLen` to this
/// value; `derive_key` always produces exactly this many bytes regardless of what a `keyLen` value
/// read from disk says, since AES-256-GCM (`tab-lock/PLAN.md` §2) has no other valid key size.
pub const KEY_LEN: usize = 32;
/// Salt length in bytes — not itself part of the wire schema's fixed fields, only the resulting
/// `saltB64` is persisted.
const SALT_LEN: usize = 16;

/// `<tab>/.tabtreenote.json`'s `lock.kdf` field, and `.tabtreenote.lock-inprogress`'s `kdf` field
/// (`tab-lock/PLAN.md` §2, `tab-lock/CRASH-RECOVERY.md`'s "Marker File Format"). Field spelling
/// matches the schema exactly: `{ "algo": "scrypt", "N": ..., "r": ..., "p": ..., "keyLen": ...,
/// "saltB64": "..." }` — every field but `N` already lands on the right camelCase spelling by the
/// struct-level `rename_all`, so only `n` needs an explicit override.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KdfParams {
    pub algo: String,
    #[serde(rename = "N")]
    pub n: u32,
    pub r: u32,
    pub p: u32,
    pub key_len: usize,
    pub salt_b64: String,
}

#[derive(Debug)]
pub enum KdfError {
    /// `n` is not a power of two, or the resulting `(log_n, r, p)` triple fails scrypt's own
    /// internal sanity bounds.
    InvalidParams,
    /// `saltB64` is not valid base64.
    InvalidSalt,
    /// The underlying `scrypt` call itself failed (only possible for a pathological `output`
    /// length, which never happens here since `KEY_LEN` is a small fixed constant).
    ScryptFailed,
    /// The `spawn_blocking` task panicked or was cancelled.
    TaskJoinFailed,
}

impl KdfParams {
    /// A fresh salt + the current recommended params (`tab_lock` step 5: "Generate a fresh KDF
    /// salt; derive the encryption key once via async scrypt").
    pub fn generate_new() -> Self {
        let mut salt = [0u8; SALT_LEN];
        rand::rng().fill(&mut salt);
        Self {
            algo: "scrypt".to_string(),
            n: 1u32 << CURRENT_LOG_N,
            r: CURRENT_R,
            p: CURRENT_P,
            key_len: KEY_LEN,
            salt_b64: BASE64.encode(salt),
        }
    }

    fn log_n(&self) -> Result<u8, KdfError> {
        if self.n == 0 || !self.n.is_power_of_two() {
            return Err(KdfError::InvalidParams);
        }
        Ok(self.n.trailing_zeros() as u8)
    }
}

/// Derives a 32-byte AES-256 key from `password` and `kdf` (`tab-lock/PLAN.md` §2: "`crypto/kdf.rs`'s
/// scrypt call must be async ... never block the Tauri command-dispatch thread during unlock").
/// Wraps the synchronous `scrypt` crate call in `tokio::task::spawn_blocking`.
pub async fn derive_key(password: String, kdf: KdfParams) -> Result<[u8; KEY_LEN], KdfError> {
    tokio::task::spawn_blocking(move || derive_key_sync(&password, &kdf))
        .await
        .map_err(|_| KdfError::TaskJoinFailed)?
}

fn derive_key_sync(password: &str, kdf: &KdfParams) -> Result<[u8; KEY_LEN], KdfError> {
    let log_n = kdf.log_n()?;
    let params = Params::new(log_n, kdf.r, kdf.p).map_err(|_| KdfError::InvalidParams)?;
    let salt = BASE64.decode(&kdf.salt_b64).map_err(|_| KdfError::InvalidSalt)?;

    let mut output = [0u8; KEY_LEN];
    scrypt(password.as_bytes(), &salt, &params, &mut output).map_err(|_| KdfError::ScryptFailed)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn derives_the_same_key_for_the_same_password_and_params() {
        let kdf = KdfParams::generate_new();
        let a = derive_key("hunter2".to_string(), kdf.clone()).await.unwrap();
        let b = derive_key("hunter2".to_string(), kdf).await.unwrap();
        assert_eq!(a, b);
    }

    #[tokio::test]
    async fn different_passwords_derive_different_keys() {
        let kdf = KdfParams::generate_new();
        let a = derive_key("hunter2".to_string(), kdf.clone()).await.unwrap();
        let b = derive_key("hunter3".to_string(), kdf).await.unwrap();
        assert_ne!(a, b);
    }

    #[tokio::test]
    async fn different_salts_derive_different_keys_for_the_same_password() {
        let a = derive_key("hunter2".to_string(), KdfParams::generate_new()).await.unwrap();
        let b = derive_key("hunter2".to_string(), KdfParams::generate_new()).await.unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn kdf_params_round_trip_through_the_documented_wire_shape() {
        let kdf = KdfParams {
            algo: "scrypt".to_string(),
            n: 32768,
            r: 8,
            p: 1,
            key_len: 32,
            salt_b64: "AAAA".to_string(),
        };
        let json = serde_json::to_value(&kdf).unwrap();
        assert_eq!(json["algo"], "scrypt");
        assert_eq!(json["N"], 32768);
        assert_eq!(json["r"], 8);
        assert_eq!(json["p"], 1);
        assert_eq!(json["keyLen"], 32);
        assert_eq!(json["saltB64"], "AAAA");
        let back: KdfParams = serde_json::from_value(json).unwrap();
        assert_eq!(back, kdf);
    }

    #[test]
    fn generate_new_uses_the_current_recommended_params() {
        let kdf = KdfParams::generate_new();
        assert_eq!(kdf.algo, "scrypt");
        assert_eq!(kdf.n, 32768);
        assert_eq!(kdf.r, 8);
        assert_eq!(kdf.p, 1);
        assert_eq!(kdf.key_len, 32);
        assert!(BASE64.decode(&kdf.salt_b64).is_ok());
    }
}
