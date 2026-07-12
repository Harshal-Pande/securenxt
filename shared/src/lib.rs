// FFI definitions for Securenxt Core Engine (compiled to library and wrapped via UniFFI)

use std::sync::Arc;

#[derive(Debug, thiserror::Error, serde::Serialize, serde::Deserialize)]
pub enum SecurenxtError {
    #[error("Git engine error: {0}")]
    GitError(String),
    #[error("Security / Cryptographic failure: {0}")]
    CryptoError(String),
    #[error("Network connection / Bluetooth error: {0}")]
    NetworkError(String),
    #[error("Database storage error: {0}")]
    StorageError(String),
    #[error("Authentication failed: {0}")]
    AuthError(String),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PeerInfo {
    pub device_id: String,
    pub alias: String,
    pub signal_strength: i32,
    pub is_trusted: bool,
    pub last_sync_time: Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncProgress {
    pub repo_id: String,
    pub current_chunk: u32,
    pub total_chunks: u32,
    pub bytes_transferred: u64,
    pub estimated_time_seconds: u32,
    pub status: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CommitMeta {
    pub hash: String,
    pub author: String,
    pub date: u64,
    pub message: String,
    pub signature_valid: bool,
}

// AppInterface represents the primary controller binding GUI/CLI apps to the Rust core engine.
pub struct AppInterface {
    // Inner connection pools, SQLCipher contexts, and Git references will reside here.
}

impl AppInterface {
    /// Initialize a new instance of the Securenxt core application context.
    pub fn new(database_path: &str, passphrase: &str) -> Result<Self, SecurenxtError> {
        // Core initialization: open encrypted SQLCipher DB, verify cryptographic keys, and set up state.
        Ok(Self {})
    }

    /// Initialize a directory as a secure, local-first Securenxt repository.
    pub fn init_repository(&self, repo_path: &str, repo_name: &str) -> Result<String, SecurenxtError> {
        // Under the hood: run git init, generate unique repo UUID, generate root truststore, encrypt database.
        Ok("repo-uuid-placeholder".to_string())
    }

    /// Start scanning for nearby Bluetooth/Wi-Fi devices advertising Securenxt repositories.
    pub fn start_discovery(&self, repo_id: &str) -> Result<(), SecurenxtError> {
        // Interfaces with transport module to trigger BLE scanner or mDNS queries.
        Ok(())
    }

    /// Stop looking for nearby devices.
    pub fn stop_discovery(&self) -> Result<(), SecurenxtError> {
        Ok(())
    }

    /// Retrieve the current list of discovered nearby peers.
    pub fn get_nearby_peers(&self) -> Result<Vec<PeerInfo>, SecurenxtError> {
        Ok(vec![])
    }

    /// Initiate pairing flow with a remote peer. Displays an OTP or generates a QR challenge.
    pub fn initiate_pairing(&self, peer_id: &str) -> Result<String, SecurenxtError> {
        // Generates 6-digit passcode or QR payload, configures Noise handshake state machine.
        Ok("581904".to_string())
    }

    /// Accept pairing challenge by inputting the OTP code provided by the other peer.
    pub fn submit_otp(&self, peer_id: &str, otp: &str) -> Result<(), SecurenxtError> {
        // Validates Noise handshake, generates AES session keys, updates trusted store database.
        Ok(())
    }

    /// Start synchronization of a specific repository with a trusted peer.
    pub fn start_sync(&self, repo_id: &str, peer_id: &str) -> Result<(), SecurenxtError> {
        // Runs "want/have" negotiation, generates delta packfile, streams data over connection channel.
        Ok(())
    }

    /// Retrieve the current status/progress of an ongoing synchronization session.
    pub fn get_sync_progress(&self, repo_id: &str) -> Result<SyncProgress, SecurenxtError> {
        Ok(SyncProgress {
            repo_id: repo_id.to_string(),
            current_chunk: 0,
            total_chunks: 0,
            bytes_transferred: 0,
            estimated_time_seconds: 0,
            status: "Idle".to_string(),
        })
    }

    /// Retrieve recent commit history with digital signature verification metadata.
    pub fn get_commit_history(&self, repo_path: &str, limit: u32) -> Result<Vec<CommitMeta>, SecurenxtError> {
        // Reads git commit objects, verifies Ed25519 signatures inside git custom headers.
        Ok(vec![])
    }
}

// Generate the FFI bindings using UniFFI templates.
uniffi::include_scaffolding!("securenxt");
