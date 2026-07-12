use crate::transport::{TransportConnection, TransportError};

/// Manages incoming and outgoing raw transport connections.
pub struct TransportManager {
    // In the future this might hold lists of active raw transports
}

impl TransportManager {
    pub fn new() -> Self {
        Self {}
    }

    /// Yields a raw transport connection. Authentication happens at the Session Layer.
    pub async fn establish_connection(&self, connection: Box<dyn TransportConnection>) -> Result<Box<dyn TransportConnection>, TransportError> {
        // Raw connection established. Pass it up for the Session Layer to encrypt and authenticate.
        Ok(connection)
    }
}
