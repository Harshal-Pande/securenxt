use thiserror::Error;
use securenxt_crypto::CryptoError;

#[derive(Debug, Error)]
pub enum SecurityError {
    #[error("Crypto error: {0}")]
    Crypto(#[from] CryptoError),
    #[error("Identity not found")]
    IdentityNotFound,
    #[error("Peer not trusted")]
    PeerNotTrusted,
    #[error("Peer already exists")]
    PeerAlreadyExists,
    #[error("Invalid signature")]
    InvalidSignature,
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
}
