use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Not connected")]
    NotConnected,
    #[error("Read error: {0}")]
    ReadError(String),
    #[error("Write error: {0}")]
    WriteError(String),
    #[error("Identity verification failed: {0}")]
    IdentityVerificationFailed(String),
    #[error("Unknown error: {0}")]
    Unknown(String),
}

/// A discovered peer that can be connected to.
#[derive(Debug, Clone)]
pub struct TransportAdvertisement {
    /// Opaque identifier used by the transport to connect
    pub address: String,
    /// Friendly name if available
    pub name: Option<String>,
}

/// A generic interface for a networking backend (Bluetooth, LAN, etc).
#[async_trait]
pub trait Transport: Send + Sync {
    /// Returns the name of the transport backend.
    fn name(&self) -> &'static str;

    /// Starts advertising our presence on this transport.
    async fn start_advertising(&self) -> Result<(), TransportError>;

    /// Stops advertising.
    async fn stop_advertising(&self) -> Result<(), TransportError>;

    /// Scans for available peers.
    async fn scan_for_peers(&self) -> Result<Vec<TransportAdvertisement>, TransportError>;

    /// Connects to a specific advertised peer.
    async fn connect(&self, address: &str) -> Result<Box<dyn TransportConnection>, TransportError>;
}

/// An established generic byte-stream connection.
#[async_trait]
pub trait TransportConnection: Send + Sync {
    /// Sends a raw packet.
    async fn send(&mut self, data: &[u8]) -> Result<(), TransportError>;

    /// Receives a raw packet.
    async fn receive(&mut self) -> Result<Vec<u8>, TransportError>;

    /// Closes the connection.
    async fn close(&mut self) -> Result<(), TransportError>;
}
