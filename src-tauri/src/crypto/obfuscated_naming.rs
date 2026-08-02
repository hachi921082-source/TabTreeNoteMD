//! Obfuscated on-disk file naming for a locked tab's memo bodies (`docs/plans/tab-lock/PLAN.md`
//! §2): "Tokens: 128-bit random hex (32 hex chars), no file extension on the obfuscated on-disk
//! file, regenerate on filesystem collision."

use std::path::Path;

use rand::RngExt;

/// A fresh 128-bit random hex token (32 lowercase hex characters), with no collision check
/// against any existing folder contents.
pub fn generate_token() -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A fresh token guaranteed not to already exist as a file/directory entry directly under
/// `folder` at the moment of the check (`tab-lock/PLAN.md` §2: "regenerate on filesystem
/// collision"). Astronomically unlikely to ever loop more than once (128 bits of entropy), but
/// loops defensively rather than assuming it never will.
pub fn generate_unique_token(folder: &Path) -> String {
    loop {
        let token = generate_token();
        if !folder.join(&token).exists() {
            return token;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_32_char_lowercase_hex_tokens() {
        let token = generate_token();
        assert_eq!(token.len(), 32);
        assert!(token.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn generates_distinct_tokens() {
        let a = generate_token();
        let b = generate_token();
        assert_ne!(a, b);
    }

    #[test]
    fn unique_token_avoids_an_existing_collision() {
        let dir = std::env::temp_dir().join(format!("ttn-obfuscated-naming-test-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();

        let taken = generate_token();
        std::fs::write(dir.join(&taken), b"").unwrap();

        // Loop enough times that if `generate_unique_token` ever returned the taken token, this
        // would very likely surface it; it never should, by construction.
        for _ in 0..1000 {
            let candidate = generate_unique_token(&dir);
            assert_ne!(candidate, taken);
        }

        std::fs::remove_dir_all(&dir).ok();
    }
}
