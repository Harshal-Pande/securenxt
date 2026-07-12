use crate::error::SecurityError;
use crate::models::{DeviceIdentity, PeerIdentity, TrustedPeer, RevokedPeer};
use securenxt_crypto::DeviceId;
use async_trait::async_trait;

#[async_trait]
pub trait TrustStore: Send + Sync {
    /// Saves the encrypted device identity to persistent storage.
    async fn save_device_identity(&self, encrypted_identity: &[u8]) -> Result<(), SecurityError>;
    
    /// Loads the encrypted device identity from storage.
    async fn load_device_identity(&self) -> Result<Option<Vec<u8>>, SecurityError>;

    /// Saves or updates a peer.
    async fn save_peer(&self, peer: &PeerIdentity) -> Result<(), SecurityError>;

    /// Retrieves a peer by their DeviceId.
    async fn get_peer(&self, device_id: &DeviceId) -> Result<Option<PeerIdentity>, SecurityError>;
    
    /// Marks a peer as explicitly trusted and saves the trust signature.
    async fn trust_peer(&self, trusted_peer: &TrustedPeer) -> Result<(), SecurityError>;
    
    /// Revokes a peer and saves the revocation state.
    async fn revoke_peer(&self, revoked_peer: &RevokedPeer) -> Result<(), SecurityError>;
    
    /// Retrieves all trusted peers.
    async fn get_trusted_peers(&self) -> Result<Vec<TrustedPeer>, SecurityError>;
}
