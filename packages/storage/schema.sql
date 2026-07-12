-- Database schema for Securenxt local metadata storage (SQLite/SQLCipher encrypted)

-- 1. Registry of known devices/peers
CREATE TABLE IF NOT EXISTS peers (
    peer_id TEXT PRIMARY KEY,               -- Base58 or hex encoded public identity key (Ed25519)
    alias TEXT NOT NULL,                     -- Developer human-readable display name
    is_trusted INTEGER DEFAULT 0,           -- 1 = Explicitly trusted by owner, 0 = Untrusted
    trust_level TEXT CHECK(trust_level IN ('ADMIN', 'WRITE', 'READ_ONLY')) DEFAULT 'READ_ONLY',
    added_at INTEGER NOT NULL,               -- Unix epoch timestamp
    last_seen_at INTEGER,                    -- Unix epoch timestamp
    trust_signature BLOB                     -- Owner's signature verifying this peer key
);

-- 2. Repositories managed by Securenxt on this device
CREATE TABLE IF NOT EXISTS repositories (
    repo_id TEXT PRIMARY KEY,                -- 32-byte UUID/Hash unique to the repository
    repo_name TEXT NOT NULL,                 -- Name of the project folder
    local_path TEXT NOT NULL UNIQUE,         -- Absolute filesystem path on local device
    owner_peer_id TEXT NOT NULL,             -- Peer ID of the repository creator/root
    master_key_wrapped BLOB,                 -- AES-256 encrypted repository key
    master_key_salt BLOB,                    -- Salt used for Argon2id derivation
    created_at INTEGER NOT NULL,
    FOREIGN KEY(owner_peer_id) REFERENCES peers(peer_id)
);

-- 3. Web-of-Trust certifications
CREATE TABLE IF NOT EXISTS trust_graph (
    issuer_peer_id TEXT,                     -- Who signed the certificate
    subject_peer_id TEXT,                    -- Whose key is being signed
    signature BLOB NOT NULL,                 -- Ed25519 signature of subject_peer_id by issuer
    created_at INTEGER NOT NULL,
    PRIMARY KEY (issuer_peer_id, subject_peer_id),
    FOREIGN KEY(issuer_peer_id) REFERENCES peers(peer_id),
    FOREIGN KEY(subject_peer_id) REFERENCES peers(peer_id)
);

-- 4. Synchronization sessions audit history
CREATE TABLE IF NOT EXISTS sync_history (
    session_id TEXT PRIMARY KEY,
    repo_id TEXT NOT NULL,
    peer_id TEXT NOT NULL,
    direction TEXT CHECK(direction IN ('PUSH', 'PULL', 'CLONE')) NOT NULL,
    started_at INTEGER NOT NULL,
    ended_at INTEGER,
    status TEXT CHECK(status IN ('SUCCESS', 'FAILED', 'IN_PROGRESS', 'CONFLICT')) NOT NULL,
    bytes_transferred INTEGER DEFAULT 0,
    commits_exchanged INTEGER DEFAULT 0,
    error_message TEXT,
    FOREIGN KEY(repo_id) REFERENCES repositories(repo_id),
    FOREIGN KEY(peer_id) REFERENCES peers(peer_id)
);

-- 5. Revoked keys blacklist
CREATE TABLE IF NOT EXISTS key_revocations (
    revoked_peer_id TEXT PRIMARY KEY,        -- Revoked peer public identity key
    revocation_reason TEXT,
    revoked_at INTEGER NOT NULL,
    authority_signature BLOB NOT NULL,       -- Owner's signature authorizing this revocation
    FOREIGN KEY(revoked_peer_id) REFERENCES peers(peer_id)
);

-- Create performance indexes
CREATE INDEX IF NOT EXISTS idx_peers_trust ON peers(is_trusted);
CREATE INDEX IF NOT EXISTS idx_sync_repo ON sync_history(repo_id);
CREATE INDEX IF NOT EXISTS idx_sync_peer ON sync_history(peer_id);
