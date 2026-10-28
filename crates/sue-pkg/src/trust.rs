//! Ed25519 signatures over canonical manifests.

use std::collections::BTreeMap;

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};

use crate::{PackageManifest, PkgError};

/// A manifest with the publisher's Ed25519 signature over its
/// [`canonical_bytes`](PackageManifest::canonical_bytes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignedManifest {
    /// The manifest.
    pub manifest: PackageManifest,
    /// Id of the publisher key that signed it.
    pub key_id: String,
    /// The 64-byte signature.
    pub signature: [u8; 64],
}

impl SignedManifest {
    /// Signs `manifest` with `key` (publisher side).
    pub fn sign(manifest: PackageManifest, key_id: impl Into<String>, key: &SigningKey) -> Self {
        let signature = key.sign(&manifest.canonical_bytes()).to_bytes();
        Self {
            manifest,
            key_id: key_id.into(),
            signature,
        }
    }
}

/// Publisher keys the user trusts.
#[derive(Clone, Debug, Default)]
pub struct TrustStore {
    keys: BTreeMap<String, VerifyingKey>,
}

impl TrustStore {
    /// No trusted keys.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Trusts the 32-byte Ed25519 public key `public_key` under `key_id`.
    ///
    /// # Errors
    /// [`PkgError::InvalidPublicKey`] if the bytes are not a valid key.
    pub fn trust(
        &mut self,
        key_id: impl Into<String>,
        public_key: &[u8; 32],
    ) -> Result<(), PkgError> {
        let key = VerifyingKey::from_bytes(public_key).map_err(|_| PkgError::InvalidPublicKey)?;
        self.keys.insert(key_id.into(), key);
        Ok(())
    }

    /// Stops trusting `key_id`. Returns `true` if it was trusted.
    pub fn revoke(&mut self, key_id: &str) -> bool {
        self.keys.remove(key_id).is_some()
    }

    /// Number of trusted keys.
    #[must_use]
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// `true` if no key is trusted.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Verifies a signed manifest with strict Ed25519 verification, which
    /// rejects malleable and small-order signatures.
    ///
    /// # Errors
    /// [`PkgError::UntrustedKey`] or [`PkgError::BadSignature`].
    pub fn verify(&self, signed: &SignedManifest) -> Result<(), PkgError> {
        let key = self
            .keys
            .get(&signed.key_id)
            .ok_or_else(|| PkgError::UntrustedKey(signed.key_id.clone()))?;
        let signature = Signature::from_bytes(&signed.signature);
        key.verify_strict(&signed.manifest.canonical_bytes(), &signature)
            .map_err(|_| PkgError::BadSignature {
                id: signed.manifest.id.clone(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PackageKind;

    fn publisher(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn manifest() -> PackageManifest {
        PackageManifest::new("sue.codec.jxl", "1.0.0", PackageKind::Codec, b"jxl codec").unwrap()
    }

    #[test]
    fn signed_manifests_verify() {
        let key = publisher(7);
        let mut trust = TrustStore::new();
        trust
            .trust("sue-release", key.verifying_key().as_bytes())
            .unwrap();
        let signed = SignedManifest::sign(manifest(), "sue-release", &key);
        assert_eq!(trust.verify(&signed), Ok(()));
        assert_eq!(trust.len(), 1);
    }

    #[test]
    fn tampering_is_detected() {
        let key = publisher(7);
        let mut trust = TrustStore::new();
        trust
            .trust("sue-release", key.verifying_key().as_bytes())
            .unwrap();
        let mut signed = SignedManifest::sign(manifest(), "sue-release", &key);
        signed.manifest.version = semver::Version::new(9, 9, 9);
        assert!(matches!(
            trust.verify(&signed),
            Err(PkgError::BadSignature { .. })
        ));

        let forged = SignedManifest::sign(manifest(), "sue-release", &publisher(8));
        assert!(matches!(
            trust.verify(&forged),
            Err(PkgError::BadSignature { .. })
        ));
    }

    #[test]
    fn unknown_and_revoked_keys_are_untrusted() {
        let key = publisher(3);
        let mut trust = TrustStore::new();
        let signed = SignedManifest::sign(manifest(), "someone", &key);
        assert_eq!(
            trust.verify(&signed),
            Err(PkgError::UntrustedKey("someone".into()))
        );
        trust
            .trust("someone", key.verifying_key().as_bytes())
            .unwrap();
        assert!(trust.revoke("someone"));
        assert!(trust.verify(&signed).is_err());
        assert!(trust.is_empty());
    }
}
