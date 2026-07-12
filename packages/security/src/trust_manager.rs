use crate::error::SecurityError;
use crate::models::{PeerIdentity, RevokedPeer, TrustStatus, TrustedPeer};
use crate::store::TrustStore;
use crate::identity_manager::IdentityManager;
use chrono::Utc;
use std::sync::Arc;
use securenxt_crypto::DeviceId;

/// Manages zero-trust access, peer trust relationships, and revocation.
pub struct TrustManager {
    store: Arc<dyn TrustStore>,
    identity_manager: Arc<IdentityManager>,
}

impl TrustManager {
    pub fn new(store: Arc<dyn TrustStore>, identity_manager: Arc<IdentityManager>) -> Self {
        Self { store, identity_manager }
    }

    /// Receives a new peer identity over the wire and registers it as Pending.
    pub async fn import_peer(&self, mut peer: PeerIdentity) -> Result<(), SecurityError> {
        // Enforce zero trust: incoming peers are always pending.
        peer.status = TrustStatus::Pending;
        
        // Prevent importing if it already exists to avoid overwriting trust state.
        if self.store.get_peer(&peer.device_id).await?.is_some() {
            return Err(SecurityError::PeerAlreadyExists);
        }

        self.store.save_peer(&peer).await
    }

    /// Retrieves a peer to verify their fingerprint manually out-of-band.
    pub async fn get_peer_for_verification(&self, device_id: &DeviceId) -> Result<PeerIdentity, SecurityError> {
        self.store.get_peer(device_id).await?
            .ok_or(SecurityError::IdentityNotFound)
    }

    /// Explicitly trusts a peer after fingerprint verification.
    /// Signs the peer's public key with our device identity to create a web of trust link.
    pub async fn trust_peer(&self, device_id: &DeviceId) -> Result<(), SecurityError> {
        let mut peer = self.store.get_peer(device_id).await?
            .ok_or(SecurityError::IdentityNotFound)?;

        if peer.status == TrustStatus::Revoked {
            return Err(SecurityError::PermissionDenied("Cannot trust a revoked peer".into()));
        }

        let signature = self.identity_manager.with_identity(|identity| {
            // Sign the peer's raw public key bytes to assert we trust them
            identity.keypair.sign(&peer.public_key.to_bytes())
        }).await?;

        peer.status = TrustStatus::Trusted;
        self.store.save_peer(&peer).await?;

        let trusted_peer = TrustedPeer {
            identity: peer,
            trust_signature: signature,
        };

        self.store.trust_peer(&trusted_peer).await
    }

    /// Revokes trust from a previously trusted or pending peer.
    pub async fn revoke_peer(&self, device_id: &DeviceId, reason: &str) -> Result<(), SecurityError> {
        let mut peer = self.store.get_peer(device_id).await?
            .ok_or(SecurityError::IdentityNotFound)?;

        let signature = self.identity_manager.with_identity(|identity| {
            // Sign the revocation assertion
            let mut payload = b"REVOKE:".to_vec();
            payload.extend_from_slice(&peer.public_key.to_bytes());
            identity.keypair.sign(&payload)
        }).await?;

        peer.status = TrustStatus::Revoked;
        self.store.save_peer(&peer).await?;

        let revoked_peer = RevokedPeer {
            identity: peer,
            revocation_reason: reason.to_string(),
            revoked_at: Utc::now(),
            authority_signature: signature,
        };

        self.store.revoke_peer(&revoked_peer).await
    }

    /// Retrieves all explicitly trusted peers.
    pub async fn get_trusted_peers(&self) -> Result<Vec<TrustedPeer>, SecurityError> {
        self.store.get_trusted_peers().await
    }
}
