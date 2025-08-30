// src/mcp/encryption.rs

use aes_gcm::{
    aead::{Aead, OsRng},
    Aes256Gcm, Key, KeyInit, Nonce,
    AeadCore, // FIX: Import the AeadCore trait to get access to generate_nonce.
};
use anyhow::{anyhow, Result};
use argon2::{
    password_hash::{PasswordHasher, SaltString},
    Argon2,
};
use base64::{engine::general_purpose, Engine as _};

// FIX: This struct holds the key derived from the master password.
// It is created once per password verification.
pub struct EncryptionKey(Key<Aes256Gcm>);

impl EncryptionKey {
    /// Derives a 256-bit key from a password and salt using Argon2.
    pub fn new(password: &str, salt: &SaltString) -> Result<Self> {
        let argon2 = Argon2::default();
        let key_hash = argon2
            .hash_password(password.as_bytes(), salt)
            .map_err(|e| anyhow!("Argon2 key derivation failed: {}", e))?;

        // Use the raw hash output as the key.
        let key_bytes = key_hash.hash.ok_or_else(|| anyhow!("Failed to get raw hash from Argon2"))?;

        Ok(Self(Key::<Aes256Gcm>::clone_from_slice(key_bytes.as_bytes())))
    }

    /// Encrypts data using the derived key. A new random nonce is generated for each encryption.
    pub fn encrypt(&self, plaintext: &str) -> Result<Vec<u8>> {
        let cipher = Aes256Gcm::new(&self.0);
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng); // Generate a random 96-bit nonce.

        let ciphertext = cipher
            .encrypt(&nonce, plaintext.as_bytes())
            .map_err(|e| anyhow!("Encryption failed: {}", e))?;

        // Prepend the nonce to the ciphertext. [ nonce (12 bytes) | ciphertext ]
        let mut result = nonce.to_vec();
        result.extend_from_slice(&ciphertext);
        Ok(result)
    }

    /// Decrypts data using the derived key. The nonce is extracted from the ciphertext itself.
    pub fn decrypt(&self, encrypted_data: &[u8]) -> Result<String> {
        if encrypted_data.len() < 12 {
            return Err(anyhow!("Invalid encrypted data: too short to contain a nonce"));
        }

        let cipher = Aes256Gcm::new(&self.0);
        let (nonce_bytes, ciphertext) = encrypted_data.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        let decrypted_bytes = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| anyhow!("Decryption failed (likely incorrect password): {}", e))?;

        String::from_utf8(decrypted_bytes)
            .map_err(|e| anyhow!("Failed to decode decrypted bytes to UTF-8: {}", e))
    }
}


// FIX: The main encryption function now handles salt generation and storage.
// The output format is "salt.encrypted_payload", both Base64 encoded.
pub fn encrypt_private_key(private_key: &str, master_password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);

    let key = EncryptionKey::new(master_password, &salt)?;
    let encrypted_payload = key.encrypt(private_key)?;

    let salt_b64 = general_purpose::STANDARD_NO_PAD.encode(salt.as_str());
    let payload_b64 = general_purpose::STANDARD_NO_PAD.encode(encrypted_payload);

    Ok(format!("{}.{}", salt_b64, payload_b64))
}

// FIX: The decryption function now parses the salt from the input string.
pub fn decrypt_private_key(encrypted_string: &str, master_password: &str) -> Result<String> {
    let parts: Vec<&str> = encrypted_string.split('.').collect();
    if parts.len() != 2 {
        return Err(anyhow!("Invalid encrypted string format"));
    }

    let salt_b64 = parts[0];
    let payload_b64 = parts[1];

    let salt_str = String::from_utf8(general_purpose::STANDARD_NO_PAD.decode(salt_b64)?)?;
    let salt = SaltString::from_b64(&salt_str)
        .map_err(|e| anyhow!("Invalid salt format: {}", e))?;

    let encrypted_payload = general_purpose::STANDARD_NO_PAD.decode(payload_b64)?;

    let key = EncryptionKey::new(master_password, &salt)?;
    key.decrypt(&encrypted_payload)
}