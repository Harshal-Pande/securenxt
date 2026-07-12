use thiserror::Error;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("Handshake failed: {0}")]
    HandshakeFailed(String),
    #[error("Decryption failed")]
    DecryptionFailed,
    #[error("Encryption failed")]
    EncryptionFailed,
    #[error("Replay detected: Nonce is too old")]
    ReplayDetected,
    #[error("Identity verification failed: {0}")]
    IdentityVerificationFailed(String),
    #[error("Transport error: {0}")]
    Transport(#[from] securenxt_protocol::TransportError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Unknown error: {0}")]
    Unknown(String),
}
