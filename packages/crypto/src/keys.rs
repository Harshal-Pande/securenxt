use crate::error::CryptoError;
use bs58;
use ed25519_dalek::{Signer, Verifier, Signature};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

/// A globally unique identity for a device, represented by its Ed25519 public key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceId(String);

impl DeviceId {
    /// Creates a DeviceId from raw public key bytes.
    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        Self(bs58::encode(bytes).into_string())
    }

    /// Creates a DeviceId from a pre-encoded Base58 string.
    pub fn new(s: String) -> Result<Self, CryptoError> {
        let decoded = bs58::decode(&s).into_vec().map_err(|_| CryptoError::InvalidKeyFormat("Invalid Base58".to_string()))?;
        if decoded.len() != 32 {
            return Err(CryptoError::InvalidKeyFormat("DeviceId must be 32 bytes".to_string()));
        }
        Ok(Self(s))
    }

    /// Returns the Base58 string representation.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A cryptographic keypair handling identity (Ed25519).
pub struct KeyPair {
    signing_key: ed25519_dalek::SigningKey,
}

impl KeyPair {
    /// Generates a new random keypair.
    pub fn generate() -> Self {
        let mut csprng = OsRng;
        let signing_key = ed25519_dalek::SigningKey::generate(&mut csprng);
        Self { signing_key }
    }

    /// Reconstructs a keypair from raw private key bytes.
    pub fn from_bytes(secret: &[u8; 32]) -> Self {
        let signing_key = ed25519_dalek::SigningKey::from_bytes(secret);
        Self { signing_key }
    }

    /// Returns the Ed25519 public key.
    pub fn public_key(&self) -> PublicKey {
        PublicKey {
            verifying_key: self.signing_key.verifying_key(),
        }
    }

    /// Signs a message.
    pub fn sign(&self, message: &[u8]) -> Vec<u8> {
        self.signing_key.sign(message).to_bytes().to_vec()
    }

    /// Returns the raw 32-byte secret seed.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }

    /// Derives the X25519 static secret for Diffie-Hellman from the Ed25519 seed.
    pub fn derive_dh_key(&self) -> x25519_dalek::StaticSecret {
        // According to the noise/signal protocol standard, the X25519 secret
        // can be derived by directly using the Ed25519 32-byte seed.
        // `x25519_dalek::StaticSecret::from` handles the clamping internally.
        x25519_dalek::StaticSecret::from(self.signing_key.to_bytes())
    }
}

/// A public key for verifying signatures.
#[derive(Clone, PartialEq, Eq)]
pub struct PublicKey {
    verifying_key: ed25519_dalek::VerifyingKey,
}

impl PublicKey {
    /// Reconstructs a public key from raw bytes.
    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self, CryptoError> {
        let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(bytes)
            .map_err(|_| CryptoError::InvalidKeyFormat("Invalid Ed25519 public key bytes".to_string()))?;
        Ok(Self { verifying_key })
    }

    /// Verifies a message signature.
    pub fn verify(&self, message: &[u8], signature_bytes: &[u8]) -> Result<(), CryptoError> {
        let signature = Signature::from_slice(signature_bytes)
            .map_err(|_| CryptoError::InvalidKeyFormat("Invalid signature length".to_string()))?;
        self.verifying_key.verify(message, &signature).map_err(|_| CryptoError::SignatureVerificationFailed)
    }

    /// Returns the raw 32-byte public key.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.verifying_key.to_bytes()
    }
    
    /// Derives the associated DeviceId (Base58 encoded public key).
    pub fn device_id(&self) -> DeviceId {
        DeviceId::from_bytes(&self.to_bytes())
    }
}
