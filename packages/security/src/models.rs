use serde::{Deserialize, Serialize};
use securenxt_crypto::{DeviceId, KeyPair, PublicKey};
use chrono::{DateTime, Utc};

/// The full local identity of the current device.
/// Never transmitted over the wire; holds the private keypair.
pub struct DeviceIdentity {
    pub device_id: DeviceId,
    pub keypair: KeyPair,
}

impl DeviceIdentity {
    pub fn new(keypair: KeyPair) -> Self {
        let device_id = keypair.public_key().device_id();
        Self { device_id, keypair }
    }
}

/// Represents the status of a peer in the zero-trust model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrustStatus {
    Pending,
    Trusted,
    Revoked,
}

/// A remote peer identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerIdentity {
    pub device_id: DeviceId,
    pub public_key: PublicKey,
    pub display_name: String,
    pub fingerprint: String,
    pub status: TrustStatus,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: Option<DateTime<Utc>>,
}

/// A peer that has been explicitly trusted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustedPeer {
    pub identity: PeerIdentity,
    /// Owner's signature verifying this peer key
    pub trust_signature: Vec<u8>, 
}

/// A peer whose trust has been revoked.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevokedPeer {
    pub identity: PeerIdentity,
    pub revocation_reason: String,
    pub revoked_at: DateTime<Utc>,
    /// Owner's signature authorizing this revocation
    pub authority_signature: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionLevel {
    Owner,
    Maintainer,
    Contributor,
    ReadOnly,
}

/// Specifies what access a peer has to a specific repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryPermission {
    pub repo_id: String,
    pub peer_id: DeviceId,
    pub level: PermissionLevel,
}

/// Resolves a peer's membership and permission in a repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryMembership {
    pub peer: TrustedPeer,
    pub permission: RepositoryPermission,
}
