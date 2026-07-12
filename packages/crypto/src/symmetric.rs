use crate::error::CryptoError;
use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Nonce, Key,
};

/// Generates a random 256-bit key for AES-GCM.
pub fn generate_aes_key() -> [u8; 32] {
    let key = Aes256Gcm::generate_key(OsRng);
    key.into()
}

/// Generates a random 96-bit nonce for AES-GCM.
pub fn generate_nonce() -> [u8; 12] {
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    nonce.into()
}

/// Encrypts plaintext using AES-256-GCM.
pub fn encrypt_gcm(key: &[u8; 32], plaintext: &[u8], nonce: &[u8; 12]) -> Result<Vec<u8>, CryptoError> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Nonce::from_slice(nonce);
    cipher.encrypt(nonce, plaintext).map_err(|_| CryptoError::EncryptionFailed)
}

/// Decrypts ciphertext using AES-256-GCM.
pub fn decrypt_gcm(key: &[u8; 32], ciphertext: &[u8], nonce: &[u8; 12]) -> Result<Vec<u8>, CryptoError> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Nonce::from_slice(nonce);
    cipher.decrypt(nonce, ciphertext).map_err(|_| CryptoError::DecryptionFailed)
}
