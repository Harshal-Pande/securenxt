use thiserror::Error;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("Encryption failed")]
    EncryptionFailed,
    #[error("Decryption failed")]
    DecryptionFailed,
    #[error("Key generation failed: {0}")]
    KeyGenerationFailed(String),
    #[error("Invalid key format: {0}")]
    InvalidKeyFormat(String),
    #[error("KDF operation failed")]
    KdfFailed,
    #[error("Signature verification failed")]
    SignatureVerificationFailed,
}
