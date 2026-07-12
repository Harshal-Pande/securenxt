use crate::error::CryptoError;
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2,
};
use hkdf::Hkdf;
use sha2::Sha256;

/// Derives a 32-byte wrapping key from a password using Argon2id.
pub fn derive_wrapping_key(password: &str, salt: &[u8]) -> Result<[u8; 32], CryptoError> {
    let argon2 = Argon2::default();
    let salt_string = SaltString::encode_b64(salt)
        .map_err(|_| CryptoError::KdfFailed)?;

    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt_string)
        .map_err(|_| CryptoError::KdfFailed)?;
    
    let hash = password_hash.hash.ok_or(CryptoError::KdfFailed)?;
    
    let mut key = [0u8; 32];
    let hash_bytes = hash.as_bytes();
    if hash_bytes.len() < 32 {
        return Err(CryptoError::KdfFailed);
    }
    key.copy_from_slice(&hash_bytes[..32]);
    Ok(key)
}

/// Generates a random 16-byte salt.
pub fn generate_salt() -> [u8; 16] {
    use rand::RngCore;
    let mut salt = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    salt
}

/// Derives keys using HKDF-SHA256 (e.g. for Noise).
pub fn hkdf_sha256(ikm: &[u8], salt: Option<&[u8]>, info: &[u8], out: &mut [u8]) -> Result<(), CryptoError> {
    let hkdf = Hkdf::<Sha256>::new(salt, ikm);
    hkdf.expand(info, out).map_err(|_| CryptoError::KdfFailed)
}
