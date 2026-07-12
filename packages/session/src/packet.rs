use crate::error::SessionError;
use crate::crypto_state::{NonceManager, SessionKey, ReplayProtection};

/// A wrapper for sending and receiving ciphertexts over the byte stream.
/// Frame format: [ Length (4 bytes) | Version (1 byte) | Nonce (12 bytes) | Ciphertext ]
pub struct EncryptedPacket;

impl EncryptedPacket {
    pub fn encrypt(plaintext: &[u8], key: &SessionKey, nonces: &mut NonceManager) -> Result<Vec<u8>, SessionError> {
        let nonce = nonces.next();
        let ciphertext = key.encrypt(plaintext, &nonce)?;
        
        let mut packet = Vec::with_capacity(4 + 1 + 12 + ciphertext.len());
        let total_len = (1 + 12 + ciphertext.len()) as u32;
        
        packet.extend_from_slice(&total_len.to_be_bytes()); // 4 bytes length
        packet.push(1); // 1 byte version
        packet.extend_from_slice(&nonce); // 12 bytes nonce
        packet.extend_from_slice(&ciphertext); // Ciphertext + MAC
        
        Ok(packet)
    }

    pub fn decrypt(packet: &[u8], key: &SessionKey, replay: &mut ReplayProtection) -> Result<Vec<u8>, SessionError> {
        if packet.len() < 1 + 12 {
            return Err(SessionError::DecryptionFailed);
        }
        
        let version = packet[0];
        if version != 1 {
            return Err(SessionError::DecryptionFailed);
        }
        
        let mut nonce = [0u8; 12];
        nonce.copy_from_slice(&packet[1..13]);
        
        let ciphertext = &packet[13..];
        
        // Ensure this nonce hasn't been replayed
        replay.validate(&nonce)?;
        
        // Decrypt validates the AEAD authentication tag intrinsically
        key.decrypt(ciphertext, &nonce)
    }
}
