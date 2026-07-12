use crate::error::SessionError;
use securenxt_crypto::symmetric::{encrypt_gcm, decrypt_gcm};

/// Manages a 96-bit (12 byte) AES-GCM nonce to ensure monotonic incrementing 
/// and prevent replay attacks.
pub struct NonceManager {
    sequence: u64,
}

impl NonceManager {
    pub fn new() -> Self {
        Self { sequence: 0 }
    }

    /// Returns the next nonce and increments the sequence counter.
    pub fn next(&mut self) -> [u8; 12] {
        let mut nonce = [0u8; 12];
        let bytes = self.sequence.to_be_bytes();
        // Pack the 8-byte sequence into the last 8 bytes of the 12-byte nonce
        nonce[4..12].copy_from_slice(&bytes);
        self.sequence += 1;
        nonce
    }
}

/// Tracks the receiving side to reject replayed sequence numbers.
pub struct ReplayProtection {
    expected_sequence: u64,
}

impl ReplayProtection {
    pub fn new() -> Self {
        Self { expected_sequence: 0 }
    }

    /// Validates the received nonce. In a full implementation this might use a sliding window.
    pub fn validate(&mut self, nonce: &[u8; 12]) -> Result<(), SessionError> {
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&nonce[4..12]);
        let sequence = u64::from_be_bytes(bytes);

        if sequence < self.expected_sequence {
            return Err(SessionError::ReplayDetected);
        }
        
        self.expected_sequence = sequence + 1;
        Ok(())
    }
}

/// Holds the transmit and receive symmetric keys derived after the Noise handshake.
pub struct SessionKey {
    pub tx_key: [u8; 32],
    pub rx_key: [u8; 32],
}

impl SessionKey {
    pub fn new(tx_key: [u8; 32], rx_key: [u8; 32]) -> Self {
        Self { tx_key, rx_key }
    }

    pub fn encrypt(&self, plaintext: &[u8], nonce: &[u8; 12]) -> Result<Vec<u8>, SessionError> {
        encrypt_gcm(&self.tx_key, plaintext, nonce)
            .map_err(|_| SessionError::EncryptionFailed)
    }

    pub fn decrypt(&self, ciphertext: &[u8], nonce: &[u8; 12]) -> Result<Vec<u8>, SessionError> {
        decrypt_gcm(&self.rx_key, ciphertext, nonce)
            .map_err(|_| SessionError::DecryptionFailed)
    }
}
