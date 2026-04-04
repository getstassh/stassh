use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db_crypto::{EncryptedPayload, decrypt_db, encrypt_db};
use crate::migrations::{LATEST_DB_VERSION, migrate_db_value};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum DbEncryption {
    None,
    Passphrase,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HostAuth {
    Key { key_path: String },
    Password { password: String },
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SshHost {
    pub id: u32,
    pub name: String,
    pub host: String,
    pub user: String,
    pub port: u16,
    pub auth: HostAuth,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct TrustedHostKey {
    pub host: String,
    pub port: u16,
    pub algorithm: String,
    pub public_key_base64: String,
    pub fingerprint_sha256: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Database {
    pub version: &'static str,
    pub hosts: Vec<SshHost>,
    pub next_host_id: u32,
    pub trusted_host_keys: Vec<TrustedHostKey>,
}

impl Database {
    pub(crate) fn default() -> Self {
        Self {
            version: LATEST_DB_VERSION,
            hosts: Vec::new(),
            next_host_id: 1,
            trusted_host_keys: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum StoredDb {
    Plain {
        db: Value,
    },
    EncryptedV1 {
        salt_b64: String,
        nonce_b64: String,
        ciphertext_b64: String,
    },
}

fn project_dirs() -> Result<ProjectDirs> {
    ProjectDirs::from("com", "bylazar", "stassh")
        .ok_or_else(|| anyhow::anyhow!("could not determine app dirs"))
}

fn db_path() -> Result<PathBuf> {
    let dirs = project_dirs()?;
    let dir = dirs.data_dir();
    fs::create_dir_all(dir)?;
    Ok(dir.join("db.json"))
}

pub(crate) fn delete_db() -> Result<()> {
    let path = db_path()?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub(crate) fn is_correct_password(passphrase: &str) -> Result<bool> {
    let path = db_path()?;

    if !path.exists() {
        return Ok(true);
    }

    let text = fs::read_to_string(&path)
        .with_context(|| format!("failed to read db file {}", path.display()))?;

    let stored: StoredDb = serde_json::from_str(&text)
        .with_context(|| format!("failed to parse db JSON from {}", path.display()))?;

    match stored {
        StoredDb::Plain { .. } => Ok(true),
        StoredDb::EncryptedV1 {
            salt_b64,
            nonce_b64,
            ciphertext_b64,
        } => {
            let decrypted = decrypt_db(
                passphrase,
                &EncryptedPayload {
                    salt_b64,
                    nonce_b64,
                    ciphertext_b64,
                },
            );
            Ok(decrypted.is_ok())
        }
    }
}

pub(crate) fn load_db(encryption: DbEncryption, passphrase: Option<&str>) -> Result<Database> {
    let path = db_path()?;

    if !path.exists() {
        return Ok(Database::default());
    }

    let text = fs::read_to_string(&path)
        .with_context(|| format!("failed to read db file {}", path.display()))?;

    let stored: StoredDb = serde_json::from_str(&text)
        .with_context(|| format!("failed to parse db JSON from {}", path.display()))?;

    match stored {
        StoredDb::Plain { db } => {
            let (db, changed) = migrate_db_value(db)?;
            if changed {
                write_stored_db(
                    &path,
                    &StoredDb::Plain {
                        db: serde_json::to_value(&db)
                            .context("failed to serialize migrated plain database")?,
                    },
                )?;
            }
            Ok(db)
        }
        StoredDb::EncryptedV1 {
            salt_b64,
            nonce_b64,
            ciphertext_b64,
        } => {
            if !matches!(encryption, DbEncryption::Passphrase) {
                bail!("database is encrypted but passphrase mode is not enabled");
            }
            let passphrase =
                passphrase.context("missing passphrase for encrypted database decryption")?;
            let decrypted = decrypt_db(
                passphrase,
                &EncryptedPayload {
                    salt_b64,
                    nonce_b64,
                    ciphertext_b64,
                },
            )?;
            let (db, changed) = migrate_db_value(decrypted)?;
            if changed {
                let payload = encrypt_db(
                    &serde_json::to_value(&db)
                        .context("failed to serialize migrated encrypted database")?,
                    passphrase,
                )?;
                write_stored_db(
                    &path,
                    &StoredDb::EncryptedV1 {
                        salt_b64: payload.salt_b64,
                        nonce_b64: payload.nonce_b64,
                        ciphertext_b64: payload.ciphertext_b64,
                    },
                )?;
            }
            Ok(db)
        }
    }
}

pub(crate) fn save_db(
    db: &Database,
    encryption: DbEncryption,
    passphrase: Option<&str>,
) -> Result<()> {
    let path = db_path()?;

    let db_value = serde_json::to_value(db).context("failed to serialize database for save")?;

    let stored = match encryption {
        DbEncryption::None => StoredDb::Plain { db: db_value },
        DbEncryption::Passphrase => {
            let passphrase =
                passphrase.context("missing passphrase for encrypted database save")?;
            let payload = encrypt_db(&db_value, passphrase)?;
            StoredDb::EncryptedV1 {
                salt_b64: payload.salt_b64,
                nonce_b64: payload.nonce_b64,
                ciphertext_b64: payload.ciphertext_b64,
            }
        }
    };

    write_stored_db(&path, &stored)
}

fn write_stored_db(path: &PathBuf, stored: &StoredDb) -> Result<()> {
    let text = serde_json::to_string_pretty(&stored)?;
    fs::write(&path, text)
        .with_context(|| format!("failed to write db file {}", path.display()))?;
    Ok(())
}
