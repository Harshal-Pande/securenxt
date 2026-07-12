use crate::crypto_state::{SessionKey, NonceManager, ReplayProtection};
use crate::error::SessionError;
use crate::packet::EncryptedPacket;
use securenxt_protocol::TransportConnection;
use async_trait::async_trait;

/// A transparently encrypted session over a raw transport connection.
pub struct SecureSession {
    inner: Box<dyn TransportConnection>,
    key: SessionKey,
    tx_nonces: NonceManager,
    rx_replay: ReplayProtection,
}

impl SecureSession {
    pub fn new(inner: Box<dyn TransportConnection>, key: SessionKey) -> Self {
        Self {
            inner,
            key,
            tx_nonces: NonceManager::new(),
            rx_replay: ReplayProtection::new(),
        }
    }
}

#[async_trait]
impl TransportConnection for SecureSession {
    async fn send(&mut self, data: &[u8]) -> Result<(), securenxt_protocol::TransportError> {
        let packet = EncryptedPacket::encrypt(data, &self.key, &mut self.tx_nonces)
            .map_err(|e| securenxt_protocol::TransportError::WriteError(e.to_string()))?;
        
        self.inner.send(&packet).await
    }

    async fn receive(&mut self) -> Result<Vec<u8>, securenxt_protocol::TransportError> {
        let packet = self.inner.receive().await?;
        
        EncryptedPacket::decrypt(&packet, &self.key, &mut self.rx_replay)
            .map_err(|e| securenxt_protocol::TransportError::ReadError(e.to_string()))
    }

    async fn close(&mut self) -> Result<(), securenxt_protocol::TransportError> {
        // Here we could securely send a termination frame if needed.
        self.inner.close().await
    }
}
