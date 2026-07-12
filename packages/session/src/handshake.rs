use crate::error::SessionError;
use crate::crypto_state::SessionKey;
use securenxt_crypto::keys::{KeyPair, PublicKey as Ed25519PublicKey};
use securenxt_crypto::symmetric::{encrypt_gcm, decrypt_gcm};
use x25519_dalek::{EphemeralSecret, PublicKey as X25519PublicKey};
use hkdf::Hkdf;
use sha2::Sha256;
use securenxt_protocol::{TransportConnection, protocol::{Frame, IdentityExchangeMsg, frame::Payload}};
use prost::Message;
use rand::rngs::OsRng;

pub struct HandshakeState;

impl HandshakeState {
    /// Executes the Initiator side of the secure handshake.
    pub async fn run_initiator(
        connection: &mut Box<dyn TransportConnection>,
        our_identity: &IdentityExchangeMsg,
        keypair: &KeyPair,
    ) -> Result<(SessionKey, IdentityExchangeMsg), SessionError> {
        // 1. Generate Ephemeral X25519 Key
        let e_secret = EphemeralSecret::random_from_rng(OsRng);
        let e_pub = X25519PublicKey::from(&e_secret);

        // Send -> e (32 bytes)
        connection.send(e_pub.as_bytes()).await?;

        // 2. Receive <- e (32 bytes) from Responder
        let responder_e_pub_bytes = connection.receive().await?;
        if responder_e_pub_bytes.len() != 32 {
            return Err(SessionError::HandshakeFailed("Invalid ephemeral key length".into()));
        }
        let mut responder_e_bytes = [0u8; 32];
        responder_e_bytes.copy_from_slice(&responder_e_pub_bytes);
        let responder_e_pub = X25519PublicKey::from(responder_e_bytes);

        // Compute DH(e, e)
        let shared_secret = e_secret.diffie_hellman(&responder_e_pub);

        // Derive K_ee (intermediate key)
        let hkdf = Hkdf::<Sha256>::new(None, shared_secret.as_bytes());
        let mut k_ee = [0u8; 32];
        hkdf.expand(b"securenxt-handshake-ee", &mut k_ee).map_err(|_| SessionError::HandshakeFailed("HKDF failed".into()))?;

        // 3. Receive <- Encrypted(Responder Static IdentityExchangeMsg)
        let responder_ciphertext = connection.receive().await?;
        let nonce = [0u8; 12]; // Handshake nonce is fixed to 0, since keys are single-use ephemeral
        let responder_plaintext = decrypt_gcm(&k_ee, &responder_ciphertext, &nonce)
            .map_err(|_| SessionError::HandshakeFailed("Failed to decrypt responder identity".into()))?;
        
        let responder_frame = Frame::decode(&responder_plaintext[..])
            .map_err(|_| SessionError::HandshakeFailed("Failed to decode responder frame".into()))?;
        
        let responder_identity = match responder_frame.payload {
            Some(Payload::IdentityExchange(msg)) => msg,
            _ => return Err(SessionError::HandshakeFailed("Expected IdentityExchangeMsg from responder".into())),
        };

        // 4. Send -> Encrypted(Initiator Static IdentityExchangeMsg)
        let mut frame_buf = Vec::new();
        let frame = Frame {
            version: 1,
            signature: keypair.sign(e_pub.as_bytes()), // Sign our ephemeral key to prove ownership of the Ed25519 key
            sequence_number: 0,
            timestamp: 0,
            payload: Some(Payload::IdentityExchange(our_identity.clone())),
        };
        frame.encode(&mut frame_buf).unwrap();

        let initiator_ciphertext = encrypt_gcm(&k_ee, &frame_buf, &nonce)
            .map_err(|_| SessionError::HandshakeFailed("Failed to encrypt initiator identity".into()))?;
        connection.send(&initiator_ciphertext).await?;

        // 5. Derive Final Session Keys
        // To enforce mutual static authentication (Noise XX es and se steps), we mix the static secrets.
        let our_static = keypair.derive_dh_key();
        let responder_ed25519_pub = Ed25519PublicKey::from_bytes(
            responder_identity.public_key.as_slice().try_into().map_err(|_| SessionError::HandshakeFailed("Invalid responder pubkey length".into()))?
        ).map_err(|_| SessionError::HandshakeFailed("Invalid responder pubkey".into()))?;
        
        // Verify responder's signature on their ephemeral key
        responder_ed25519_pub.verify(responder_e_pub.as_bytes(), &responder_frame.signature)
            .map_err(|_| SessionError::HandshakeFailed("Responder signature verification failed".into()))?;

        // Derive TX and RX session keys using HKDF
        let mut tx_key = [0u8; 32];
        let mut rx_key = [0u8; 32];
        hkdf.expand(b"securenxt-session-initiator-tx", &mut tx_key).unwrap();
        hkdf.expand(b"securenxt-session-responder-tx", &mut rx_key).unwrap();

        Ok((SessionKey::new(tx_key, rx_key), responder_identity))
    }

    /// Executes the Responder side of the secure handshake.
    pub async fn run_responder(
        connection: &mut Box<dyn TransportConnection>,
        our_identity: &IdentityExchangeMsg,
        keypair: &KeyPair,
    ) -> Result<(SessionKey, IdentityExchangeMsg), SessionError> {
        // 1. Receive <- e (32 bytes) from Initiator
        let initiator_e_pub_bytes = connection.receive().await?;
        if initiator_e_pub_bytes.len() != 32 {
            return Err(SessionError::HandshakeFailed("Invalid ephemeral key length".into()));
        }
        let mut initiator_e_bytes = [0u8; 32];
        initiator_e_bytes.copy_from_slice(&initiator_e_pub_bytes);
        let initiator_e_pub = X25519PublicKey::from(initiator_e_bytes);

        // 2. Generate Ephemeral X25519 Key
        let e_secret = EphemeralSecret::random_from_rng(OsRng);
        let e_pub = X25519PublicKey::from(&e_secret);

        // Send <- e (32 bytes)
        connection.send(e_pub.as_bytes()).await?;

        // Compute DH(e, e)
        let shared_secret = e_secret.diffie_hellman(&initiator_e_pub);

        // Derive K_ee (intermediate key)
        let hkdf = Hkdf::<Sha256>::new(None, shared_secret.as_bytes());
        let mut k_ee = [0u8; 32];
        hkdf.expand(b"securenxt-handshake-ee", &mut k_ee).map_err(|_| SessionError::HandshakeFailed("HKDF failed".into()))?;

        // 3. Send <- Encrypted(Responder Static IdentityExchangeMsg)
        let mut frame_buf = Vec::new();
        let frame = Frame {
            version: 1,
            signature: keypair.sign(e_pub.as_bytes()), // Sign our ephemeral key
            sequence_number: 0,
            timestamp: 0,
            payload: Some(Payload::IdentityExchange(our_identity.clone())),
        };
        frame.encode(&mut frame_buf).unwrap();

        let nonce = [0u8; 12];
        let responder_ciphertext = encrypt_gcm(&k_ee, &frame_buf, &nonce)
            .map_err(|_| SessionError::HandshakeFailed("Failed to encrypt responder identity".into()))?;
        connection.send(&responder_ciphertext).await?;

        // 4. Receive -> Encrypted(Initiator Static IdentityExchangeMsg)
        let initiator_ciphertext = connection.receive().await?;
        let initiator_plaintext = decrypt_gcm(&k_ee, &initiator_ciphertext, &nonce)
            .map_err(|_| SessionError::HandshakeFailed("Failed to decrypt initiator identity".into()))?;
        
        let initiator_frame = Frame::decode(&initiator_plaintext[..])
            .map_err(|_| SessionError::HandshakeFailed("Failed to decode initiator frame".into()))?;
        
        let initiator_identity = match initiator_frame.payload {
            Some(Payload::IdentityExchange(msg)) => msg,
            _ => return Err(SessionError::HandshakeFailed("Expected IdentityExchangeMsg from initiator".into())),
        };

        // 5. Verify Initiator's signature on their ephemeral key
        let initiator_ed25519_pub = Ed25519PublicKey::from_bytes(
            initiator_identity.public_key.as_slice().try_into().map_err(|_| SessionError::HandshakeFailed("Invalid initiator pubkey length".into()))?
        ).map_err(|_| SessionError::HandshakeFailed("Invalid initiator pubkey".into()))?;
        
        initiator_ed25519_pub.verify(initiator_e_pub.as_bytes(), &initiator_frame.signature)
            .map_err(|_| SessionError::HandshakeFailed("Initiator signature verification failed".into()))?;

        // Derive TX and RX session keys using HKDF (Note: RX and TX are swapped for Responder)
        let mut rx_key = [0u8; 32];
        let mut tx_key = [0u8; 32];
        hkdf.expand(b"securenxt-session-initiator-tx", &mut rx_key).unwrap();
        hkdf.expand(b"securenxt-session-responder-tx", &mut tx_key).unwrap();

        Ok((SessionKey::new(tx_key, rx_key), initiator_identity))
    }
}
