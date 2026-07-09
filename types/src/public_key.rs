use crate::{Account, RawKey, Signature, SignatureError, serialize_32_byte_string, u256_struct};
use ed25519_dalek::Verifier;

u256_struct!(PublicKey);
serialize_32_byte_string!(PublicKey);

impl PublicKey {
    /// IV for Key encryption
    pub fn initialization_vector(&self) -> [u8; 16] {
        self.0[..16].try_into().unwrap()
    }

    pub fn as_account(&self) -> Account {
        self.into()
    }

    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<(), SignatureError> {
        let public =
            ed25519_dalek::VerifyingKey::from_bytes(&self.0).map_err(|_| SignatureError {})?;
        let sig = ed25519_dalek::Signature::from_bytes(signature.as_bytes());
        public
            .verify(message, &sig)
            .map_err(|_| SignatureError {})?;
        Ok(())
    }
}

impl From<RawKey> for PublicKey {
    fn from(value: RawKey) -> Self {
        let secret = ed25519_dalek::SecretKey::from(*value.as_bytes());
        let signing_key = ed25519_dalek::SigningKey::from(&secret);
        let public = ed25519_dalek::VerifyingKey::from(&signing_key);
        Self::from_bytes(public.to_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_signature_error() {
        let key = PublicKey::from(123);
        let error = key
            .verify(b"test message", &Signature::from_bytes([1; 64]))
            .unwrap_err();
        assert_eq!(error.to_string(), "invalid signature");
    }
}
