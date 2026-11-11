//! Secrets the hub keeps in its database, sealed with AES-256-GCM under a key that lives only
//! in the state directory (0600). A copy of the database alone gives away nothing, and a sealed
//! value bound to one name does not open under another.

use std::path::Path;

use aws_lc_rs::aead::{AES_256_GCM, Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use base64::{Engine, engine::general_purpose::STANDARD};

const KEY_FILE: &str = "settings.key";

pub struct Sealer {
    key: LessSafeKey,
}

impl std::fmt::Debug for Sealer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Sealer")
    }
}

impl Sealer {
    fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let key =
            UnboundKey::new(&AES_256_GCM, bytes).map_err(|_| "not a 32-byte key".to_string())?;
        Ok(Self {
            key: LessSafeKey::new(key),
        })
    }

    /// The sealer of `state_dir`; its key is made on first use, owner-only.
    pub fn open(state_dir: &Path) -> Result<Self, String> {
        let path = state_dir.join(KEY_FILE);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let bytes = crate::link::random::<32>().to_vec();
                crate::push::write_private(&path, &bytes).map_err(|error| error.message)?;
                bytes
            }
            Err(error) => return Err(format!("{}: {error}", path.display())),
        };
        Self::from_bytes(&bytes).map_err(|why| format!("{}: {why}", path.display()))
    }

    /// A sealer with a key of its own, gone with the process.
    pub fn ephemeral() -> Self {
        Self::from_bytes(&crate::link::random::<32>()).expect("a 32-byte key")
    }

    pub fn seal(&self, name: &str, secret: &str) -> String {
        let nonce = crate::link::random::<NONCE_LEN>();
        let mut sealed = secret.as_bytes().to_vec();
        self.key
            .seal_in_place_append_tag(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(name.as_bytes()),
                &mut sealed,
            )
            .expect("sealing");
        let mut out = nonce.to_vec();
        out.extend_from_slice(&sealed);
        STANDARD.encode(out)
    }

    pub fn open_value(&self, name: &str, sealed: &str) -> Option<String> {
        let bytes = STANDARD.decode(sealed).ok()?;
        if bytes.len() < NONCE_LEN {
            return None;
        }
        let (nonce, rest) = bytes.split_at(NONCE_LEN);
        let nonce: [u8; NONCE_LEN] = nonce.try_into().ok()?;
        let mut rest = rest.to_vec();
        let opened = self
            .key
            .open_in_place(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(name.as_bytes()),
                &mut rest,
            )
            .ok()?;
        String::from_utf8(opened.to_vec()).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sealed_value_opens_only_under_its_name_and_its_key() {
        let sealer = Sealer::ephemeral();
        let sealed = sealer.seal("google", "the secret");
        assert!(!sealed.contains("the secret"));
        assert_eq!(
            sealer.open_value("google", &sealed).as_deref(),
            Some("the secret")
        );
        assert_eq!(sealer.open_value("github", &sealed), None);
        assert_eq!(Sealer::ephemeral().open_value("google", &sealed), None);
        assert_ne!(
            sealer.seal("google", "the secret"),
            sealed,
            "a new nonce each time"
        );
    }

    #[test]
    fn the_key_is_kept_between_starts() {
        let dir = tempfile::tempdir().unwrap();
        let sealed = Sealer::open(dir.path()).unwrap().seal("google", "s");
        let again = Sealer::open(dir.path()).unwrap();
        assert_eq!(again.open_value("google", &sealed).as_deref(), Some("s"));
    }
}
