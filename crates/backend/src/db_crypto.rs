use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use anyhow::{bail, Context, Result};
use argon2::Argon2;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use rand::{rngs::OsRng, RngCore};
use serde_json::Value;

#[derive(Debug, Clone)]
pub(crate) struct EncryptedPayload {
    pub(crate) salt_b64: String,
    pub(crate) nonce_b64: String,
    pub(crate) ciphertext_b64: String,
}

pub(crate) fn decrypt_db(passphrase: &str, payload: &EncryptedPayload) -> Result<Value> {
    let salt = B64
        .decode(&payload.salt_b64)
        .context("invalid db salt encoding")?;
    let nonce_raw = B64
        .decode(&payload.nonce_b64)
        .context("invalid db nonce encoding")?;
    let ciphertext = B64
        .decode(&payload.ciphertext_b64)
        .context("invalid db ciphertext encoding")?;

    if nonce_raw.len() != 12 {
        bail!("invalid db nonce length");
    }

    let key = derive_key(passphrase, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|err| anyhow::anyhow!("failed to initialize decrypt cipher: {err}"))?;

    let plaintext = cipher
        .decrypt(Nonce::from_slice(&nonce_raw), ciphertext.as_ref())
        .map_err(|_| {
            anyhow::anyhow!("failed to decrypt database (wrong passphrase or corrupt file)")
        })?;

    let db: Value =
        serde_json::from_slice(&plaintext).context("failed to parse decrypted database JSON")?;
    Ok(db)
}

pub(crate) fn encrypt_db(db: &Value, passphrase: &str) -> Result<EncryptedPayload> {
    let plaintext = serde_json::to_vec(db).context("failed to serialize database JSON")?;

    let mut salt = [0u8; 16];
    OsRng.fill_bytes(&mut salt);

    let key = derive_key(passphrase, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|err| anyhow::anyhow!("failed to initialize encrypt cipher: {err}"))?;

    let mut nonce_raw = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_raw);

    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_raw), plaintext.as_ref())
        .map_err(|_| anyhow::anyhow!("failed to encrypt database"))?;

    Ok(EncryptedPayload {
        salt_b64: B64.encode(salt),
        nonce_b64: B64.encode(nonce_raw),
        ciphertext_b64: B64.encode(ciphertext),
    })
}

fn derive_key(passphrase: &str, salt_raw: &[u8]) -> Result<[u8; 32]> {
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(passphrase.as_bytes(), salt_raw, &mut key)
        .map_err(|err| anyhow::anyhow!("failed to derive key from passphrase: {err}"))?;
    Ok(key)
}
