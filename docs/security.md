# Securenxt Security Architecture Specification

This document details the threat model, cryptographic parameters, PKI design, and security countermeasures implemented in the Securenxt platform.

## 1. Threat Model & Security Goals

Securenxt is designed for high-risk, disconnected environments (remote field research, corporate offices, protest situations) where the local network and physical transport are untrusted.

| Threat | Description | Countermeasure |
| :--- | :--- | :--- |
| **Passive Eavesdropping** | Attacker captures packets on the wireless channel (BLE/Wi-Fi) to read code/commits. | **End-to-End Encryption:** All packets are encrypted via AES-256-GCM using keys derived during the Noise handshake. |
| **Active MITM Attack** | Attacker intercepts initial pairing connection, presenting fake public keys. | **OTP Pairing Verification:** Initiator and Responder authenticate using a 6-digit passcode mixed into the Noise handshake state. |
| **Physical Device Theft** | Peer laptop or mobile device is lost/stolen, exposing code repositories. | **Encryption-at-Rest:** Workspace database metadata is encrypted with SQLCipher. Project files are encrypted on disk. |
| **Compromised Peer Key** | An authorized collaborator's device is compromised, attempting to push malware. | **Key Revocation List:** The repository owner publishes a signed key revocation list. All nodes reject commits signed by blacklisted keys. |
| **Replay Attacks** | Attacker records old synchronization sequences and plays them back to regress state. | **Monotonically Increasing Nonces:** Every packet has a 64-bit frame counter. Stale nonces result in packet drops. |

---

## 2. Cryptographic Parameters and Specifications

Securenxt enforces strong, modern cryptographic configurations. We restrict primitives to standard, audited algorithms.

*   **Asymmetric Signature (Identity):** Ed25519 (256-bit keys) for commit signing and identity keys.
*   **Key Exchange (Diffie-Hellman):** X25519 (ECDH) for ephemeral session handshakes.
*   **Symmetric Encryption (In-Transit):** AES-256-GCM (Authenticated Encryption with Associated Data).
*   **Symmetric Encryption (At-Rest):** AES-256-CBC (SQLCipher default) and AES-256-GCM for repository files.
*   **Key Derivation Function (KDF):** HKDF-SHA256 (extract-and-expand) for session key derivation.
*   **Password-Based Key Derivation (Disk):** Argon2id (parameters: $m=262144\,\text{KB}$, $t=3$ iterations, parallelism $p=4$).
*   **Cryptographic Libraries:** We leverage Rust implementations from the `ring` and `RustCrypto` projects to avoid manual implementation bugs.

---

## 3. Web-of-Trust and Role-Based Access Control

Securenxt utilizes a decentralized Web-of-Trust (WoT) to grant repository permissions.

```
                  +--------------------------+
                  | Repository Owner (Admin) |
                  +------------+-------------+
                               |
            Signs public key   |   Signs public key
            with Ed25519       |   with Ed25519
                               |
         +---------------------v---------------------+
         |                                           |
+--------v-------+                           +--------v-------+
| Collaborator A |                           | Collaborator B |
| (Write Access) |                           | (Read-Only)    |
+----------------+                           +----------------+
```

1.  **Repository Trust Anchor:** The creator of the repository generates the repository ID and signs the initial truststore ledger.
2.  **Explicit Enrollment:** When a new collaborator pairs with the owner, the owner's device prompts the user to assign a role (`READ_ONLY`, `WRITE`, `ADMIN`). The owner signs a certificate containing the collaborator's public key and the assigned role.
3.  **Commit Validation:** When a peer receives commits during a push/pull, it traces the signature header on each commit back to a public key certified in the truststore. Unsigned commits or commits signed by unauthorized peers are rejected.

---

## 4. Key Rotation & Ephemeral Secrecy

*   **Ratcheting:** During a live session, keys are ratcheted using HKDF every $1\,\text{GB}$ of transmitted data or every 30 minutes of inactive time.
*   **Perfect Forward Secrecy:** Ephemeral keys generated for a session are zeroed from memory immediately after session teardown.

---

## 5. Security Countermeasures: Emergency Panic Wipe

In high-risk environments, developers may need to destroy cryptographic credentials quickly. Securenxt implements a **Panic Wipe** feature:

*   **Trigger:** Can be activated via key bindings, GUI panic buttons, or mobile gestures (e.g. shaking the phone while app is active).
*   **Action:**
    1.  The app overwrites the SQLCipher private database files and credentials using secure memory-zeroing patterns (writing random bits over the file path).
    2.  It deletes all session keys and permanent identity key files from disk.
    3.  It removes the repository metadata from the local device, leaving only unreadable, encrypted raw workspace data.
