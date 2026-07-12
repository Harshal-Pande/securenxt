# Securenxt System Architecture

This document describes the software architecture, package layout, database design, and FFI integration details of **Securenxt**, a decentralized, offline-first code collaboration platform.

## 1. Modular Architectural Overview

Securenxt is designed as a modular monorepo containing a shared Rust Core library (`securenxt-shared`), which is consumed by native applications across multiple operating systems.

```
                  +-----------------------------------+
                  |  Tauri Desktop App (HTML/CSS/JS)  |
                  +-----------------+-----------------+
                                    | IPC
                  +-----------------v-----------------+
                  |      Tauri Rust Backend Crate     |
                  +-----------------+-----------------+
                                    | Rust Imports
                  +-----------------v-----------------+
                  |     securenxt-shared (FFI Core)   |
                  +--------+--------------+-----------+
                           |              |
         +-----------------+              +-----------------+
         | Imports                                  Imports |
+--------v-------+                                 +--------v-------+
|  securenxt-cli |                                 | Android Native |
|   (Rust CLI)   |                                 | (Kotlin + JNI) |
+----------------+                                 +----------------+
```

### 1.1 Core Packages Breakdown

The core logic of Securenxt is split into dedicated packages in the `packages/` directory:

1.  **`packages/crypto`**: Wraps secure cryptographic primitives for symmetric encryption (AES-256-GCM), asymmetric identities (Ed25519), session key agreement (X25519), and key derivation (HKDF, Argon2id).
2.  **`packages/protocol`**: Houses the Protocol Buffer messages and their generated serialization wrappers. Handles packet serialization and network-level framing (including nonces and signatures).
3.  **`packages/security`**: Orchestrates the security state machine. Specifically implements the **Noise XX handshake** with passcode-derived Pre-Shared Key (PSK) inputs to establish end-to-end encryption.
4.  **`packages/storage`**: Manages the local SQLite database. Uses SQLCipher on-disk encryption to store keys, settings, access lists, and sync metadata securely.
5.  **`packages/git-engine`**: Integrates with `libgit2` via Rust `git2-rs`. Manages git commit DAG generation, reading objects, comparing indices, compiling packfiles, and performing 3-way merges.
6.  **`packages/bluetooth`**: Provides the Bluetooth transport layer. Encapsulates BlueZ (Linux), CoreBluetooth (macOS), WinRT Bluetooth (Windows), and Android Bluetooth APIs into a unified, socket-like stream wrapper.
7.  **`packages/sync`**: Orchestrates the sync negotiation protocol. Resolves differences between repositories using "want/have" DAG traversals and monitors file transmission.

---

## 2. Multi-Platform FFI Strategy (UniFFI)

To prevent code duplication, the entire business logic, security stack, and transport logic are compiled inside a single shared library target (`shared`).

We utilize **UniFFI** to generate high-performance, safe bindings for each target platform:

*   **Android (Kotlin):** UniFFI compiles the core library to a static dynamic library (`libsecurenxt_core.so`) for target architectures (ARM64, x86_64). It then generates Kotlin interface classes wrapping JNA (Java Native Access) calls, letting Jetpack Compose code interact natively with Rust structures.
*   **Desktop (Tauri):** The Tauri frontend communicates with the Tauri Rust backend via standard IPC (Inter-Process Communication). The Tauri Rust backend references the packages directly as standard Rust crates, enabling zero-overhead execution.
*   **CLI (Rust):** A simple Rust binary wraps the crates directly, compiling to a standalone, zero-dependency command line utility.

---

## 3. Storage Design (SQLCipher Metadata Database)

Our local SQLite database is encrypted-at-rest using SQLCipher. Below is the entity relationship layout for the metadata schema:

```
 +------------------+          +-----------------------+          +--------------------+
 |      PEERS       |          |      TRUST_GRAPH      |          |    REPOSITORIES    |
 +------------------+          +-----------------------+          +--------------------+
 | peer_id (PK)     |<----+    | issuer_peer_id (PK)   |<----+    | repo_id (PK)       |
 | alias            |     |    | subject_peer_id (PK)  |     |    | repo_name          |
 | is_trusted       |     +---| issuer_peer_id (FK)   |     |    | local_path         |
 | trust_level      |     |    | signature             |     +---| owner_peer_id (FK) |
 | added_at         |     +---| subject_peer_id (FK)  |          | master_key_wrapped |
 | last_seen_at     |          | created_at            |          | created_at         |
 | trust_signature  |          +-----------------------+          +--------------------+
 +------------------+
          ^
          |
          |                    +-----------------------+
          |                    |     SYNC_HISTORY      |
          |                    +-----------------------+
          |                    | session_id (PK)       |
          +--------------------| peer_id (FK)          |
                               | repo_id (FK)          |
                               | direction             |
                               | started_at            |
                               | status                |
                               +-----------------------+
```

---

## 4. Technology Selection Rationale Summary

*   **Rust** was chosen as the primary implementation language because its borrow checker guarantees compile-time memory safety. In a security-focused product that regularly parses byte frames from unknown wireless channels, avoiding Buffer Overflows, Use-After-Free, and Data Races is critical.
*   **Noise Protocol Framework** was chosen over TLS 1.3 because it avoids self-signed certificate dialogs or complex Certificate Authority configurations. It supports static public key pinning and integrates passcode validation natively.
*   **libgit2** was selected to handle version control because it provides the highest performance and maintains strict parity with standard Git. This ensures that any IDE, git client, or terminal tool can read or modify a Securenxt-managed project folder without friction.
