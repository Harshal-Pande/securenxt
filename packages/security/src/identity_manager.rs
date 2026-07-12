use crate::error::SecurityError;
use crate::models::{DeviceIdentity, PeerIdentity, TrustStatus};
use crate::store::TrustStore;
use securenxt_crypto::{kdf, symmetric, KeyPair};
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Manages the local device's identity and its persistence.
pub struct IdentityManager {
    store: Arc<dyn TrustStore>,
    active_identity: RwLock<Option<DeviceIdentity>>,
}

impl IdentityManager {
    pub fn new(store: Arc<dyn TrustStore>) -> Self {
        Self {
            store,
            active_identity: RwLock::new(None),
        }
    }

    /// Generates a brand new device identity, encrypts it with a password, and persists it.
    pub async fn enroll_device(&self, password: &str) -> Result<DeviceIdentity, SecurityError> {
        let keypair = KeyPair::generate();
        let device_identity = DeviceIdentity::new(keypair);

        self.save_identity(&device_identity, password).await?;

        // Cache the active identity
        let mut active = self.active_identity.write().await;
        // In order to not require Clone on KeyPair (which wraps Ed25519 SigningKey),
        // we can just reconstruct it from bytes.
        let seed = device_identity.keypair.to_bytes();
        let cached_identity = DeviceIdentity::new(KeyPair::from_bytes(&seed));
        *active = Some(cached_identity);

        Ok(device_identity)
    }

    /// Loads an existing device identity from storage using the password.
    pub async fn load_identity(&self, password: &str) -> Result<DeviceIdentity, SecurityError> {
        let encrypted_data = self.store.load_device_identity().await?
            .ok_or(SecurityError::IdentityNotFound)?;

        // The serialized format:
        // [salt: 16 bytes] [nonce: 12 bytes] [ciphertext: 32 bytes seed + 16 bytes tag]
        if encrypted_data.len() != 16 + 12 + 32 + 16 {
            return Err(SecurityError::Crypto(securenxt_crypto::CryptoError::InvalidKeyFormat("Invalid encrypted identity length".into())));
        }

        let salt = &encrypted_data[0..16];
        let nonce = &encrypted_data[16..28];
        let ciphertext = &encrypted_data[28..];

        let wrapping_key = kdf::derive_wrapping_key(password, salt)?;
        
        let mut nonce_arr = [0u8; 12];
        nonce_arr.copy_from_slice(nonce);

        let plaintext = symmetric::decrypt_gcm(&wrapping_key, ciphertext, &nonce_arr)?;
        
        if plaintext.len() != 32 {
            return Err(SecurityError::Crypto(securenxt_crypto::CryptoError::InvalidKeyFormat("Decrypted seed is not 32 bytes".into())));
        }

        let mut seed = [0u8; 32];
        seed.copy_from_slice(&plaintext);

        let keypair = KeyPair::from_bytes(&seed);
        let device_identity = DeviceIdentity::new(keypair);

        let mut active = self.active_identity.write().await;
        let cached_identity = DeviceIdentity::new(KeyPair::from_bytes(&seed));
        *active = Some(cached_identity);

        Ok(device_identity)
    }

    /// Exports the public identity to be shared with peers.
    pub async fn export_public_identity(&self, display_name: &str) -> Result<PeerIdentity, SecurityError> {
        let active = self.active_identity.read().await;
        let identity = active.as_ref().ok_or(SecurityError::IdentityNotFound)?;
        
        let public_key = identity.keypair.public_key();
        let fingerprint = securenxt_crypto::fingerprint::generate_fingerprint(&public_key);

        Ok(PeerIdentity {
            device_id: identity.device_id.clone(),
            public_key,
            display_name: display_name.to_string(),
            fingerprint,
            status: TrustStatus::Pending,
            created_at: Utc::now(),
            last_seen_at: Some(Utc::now()),
        })
    }
    
    /// Provides access to the currently loaded identity to execute actions on its behalf.
    pub async fn with_identity<F, R>(&self, f: F) -> Result<R, SecurityError> 
    where
        F: FnOnce(&DeviceIdentity) -> R
    {
        let active = self.active_identity.read().await;
        let identity = active.as_ref().ok_or(SecurityError::IdentityNotFound)?;
        Ok(f(identity))
    }

    async fn save_identity(&self, identity: &DeviceIdentity, password: &str) -> Result<(), SecurityError> {
        let salt = kdf::generate_salt();
        let wrapping_key = kdf::derive_wrapping_key(password, &salt)?;

        let nonce = symmetric::generate_nonce();
        let plaintext_seed = identity.keypair.to_bytes();

        let ciphertext = symmetric::encrypt_gcm(&wrapping_key, &plaintext_seed, &nonce)?;

        let mut serialized = Vec::with_capacity(16 + 12 + ciphertext.len());
        serialized.extend_from_slice(&salt);
        serialized.extend_from_slice(&nonce);
        serialized.extend_from_slice(&ciphertext);

        self.store.save_device_identity(&serialized).await
    }
}
