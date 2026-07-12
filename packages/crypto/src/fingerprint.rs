use sha2::{Digest, Sha256};
use crate::keys::PublicKey;

/// Generates a human-readable SHA-256 fingerprint for a public key.
/// This matches standard formats like `SHA256:Base64` or hex.
pub fn generate_fingerprint(public_key: &PublicKey) -> String {
    let mut hasher = Sha256::new();
    hasher.update(public_key.to_bytes());
    let result = hasher.finalize();
    
    // We'll return it as a colon-separated hex string for readability
    let hex_chars: Vec<String> = result
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect();
    
    // For extreme brevity we could just take the first N bytes, but for security 
    // full SHA256 (or at least 32 chars) is better. Let's format it as a continuous hex string.
    hex_chars.join("")
}
