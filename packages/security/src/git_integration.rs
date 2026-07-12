use securenxt_git_engine::types::CommitSigner;
use crate::identity_manager::IdentityManager;
use std::sync::Arc;
use tokio::runtime::Handle;

/// Implements Git-engine's CommitSigner trait to sign commits with the device's identity.
pub struct SecurityCommitSigner {
    identity_manager: Arc<IdentityManager>,
}

impl SecurityCommitSigner {
    pub fn new(identity_manager: Arc<IdentityManager>) -> Self {
        Self { identity_manager }
    }
}

// Satisfy the sealing marker trait.
impl securenxt_git_engine::types::private::Sealed for SecurityCommitSigner {}

impl CommitSigner for SecurityCommitSigner {
    fn sign(&self, data: &[u8]) -> Vec<u8> {
        // Since CommitSigner is synchronous (called deeply inside git2-rs callbacks),
        // we use block_on if we need to cross async boundaries.
        // In our case, with_identity uses RwLock which is async. 
        // We can block_on to acquire the lock.
        tokio::task::block_in_place(|| {
            Handle::current().block_on(async {
                self.identity_manager.with_identity(|identity| {
                    identity.keypair.sign(data)
                }).await.unwrap_or_default()
            })
        })
    }

    fn public_key_bytes(&self) -> [u8; 32] {
        tokio::task::block_in_place(|| {
            Handle::current().block_on(async {
                self.identity_manager.with_identity(|identity| {
                    identity.keypair.public_key().to_bytes()
                }).await.unwrap_or_default()
            })
        })
    }
}
