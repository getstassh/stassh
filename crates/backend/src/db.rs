use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::ProjectDirs;
use rusqlite::{Connection, Error as SqlError, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::{Config, sql_migrations};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum DbEncryption {
    None,
    Passphrase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbOpenStatus {
    Missing,
    Plain,
    PassphraseRequired,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HostAuth {
    KeyPath { key_path: String },
    KeyInline { private_key: String },
    Password { password: String },
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct SshEndpoint {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SshHost {
    pub id: u32,
    pub name: String,
    pub user: String,
    pub endpoints: Vec<SshEndpoint>,
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
    pub hosts: Vec<SshHost>,
    pub next_host_id: u32,
    pub trusted_host_keys: Vec<TrustedHostKey>,
}

impl Database {
    pub(crate) fn default() -> Self {
        Self {
            hosts: Vec::new(),
            next_host_id: 1,
            trusted_host_keys: Vec::new(),
        }
    }
}

fn project_dirs() -> Result<ProjectDirs> {
    ProjectDirs::from("com", "bylazar", "stassh")
        .ok_or_else(|| anyhow::anyhow!("could not determine app dirs"))
}

fn db_path() -> Result<PathBuf> {
    let dirs = project_dirs()?;
    let dir = dirs.data_dir();
    fs::create_dir_all(dir)?;
    Ok(dir.join("db.sqlite"))
}

pub(crate) fn delete_db() -> Result<()> {
    let path = db_path()?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub(crate) fn db_open_status() -> Result<DbOpenStatus> {
    let path = db_path()?;
    if !path.exists() {
        return Ok(DbOpenStatus::Missing);
    }

    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open database {}", path.display()))?;
    configure_sqlcipher_logging(&conn);
    match validate_connection(&conn) {
        Ok(()) => Ok(DbOpenStatus::Plain),
        Err(err) if is_wrong_passphrase_error(&err) => Ok(DbOpenStatus::PassphraseRequired),
        Err(err) => Err(err),
    }
}

pub(crate) fn is_correct_password(passphrase: &str) -> Result<bool> {
    let path = db_path()?;

    if !path.exists() {
        return Ok(true);
    }

    match open_connection(Some(passphrase)) {
        Ok(_) => Ok(true),
        Err(err) if is_wrong_passphrase_error(&err) => Ok(false),
        Err(err) => Err(err),
    }
}

pub(crate) fn load_state(passphrase: Option<&str>) -> Result<(Database, Config)> {
    let conn = open_connection(passphrase)?;
    let mut config = load_or_init_config(&conn)?;
    let mut db = load_database(&conn)?;

    db.next_host_id = db.next_host_id.max(max_host_id_plus_one(&db.hosts));
    config.ssh_idle_timeout_seconds = config.ssh_idle_timeout_seconds.max(1);
    config.ssh_connect_timeout_seconds = config.ssh_connect_timeout_seconds.max(1);

    save_database(&conn, &db)?;
    save_config(&conn, &config)?;

    Ok((db, config))
}

pub(crate) fn save_state(
    db: &Database,
    config: &Config,
    encryption: DbEncryption,
    passphrase: Option<&str>,
) -> Result<()> {
    let conn = open_connection_for_write(encryption, passphrase)?;
    save_database(&conn, db)?;
    save_config(&conn, config)?;
    Ok(())
}

pub(crate) fn save_config_only(
    config: &Config,
    encryption: DbEncryption,
    passphrase: Option<&str>,
) -> Result<()> {
    let conn = open_connection_for_write(encryption, passphrase)?;
    save_config(&conn, config)
}

pub(crate) fn rekey_database(
    current_passphrase: Option<&str>,
    new_passphrase: Option<&str>,
) -> Result<()> {
    if let Some(new_passphrase) = new_passphrase
        && new_passphrase.trim().is_empty()
    {
        anyhow::bail!("new passphrase cannot be empty");
    }

    if current_passphrase.is_none() && new_passphrase.is_some() {
        encrypt_plaintext_database(new_passphrase.context("missing new passphrase")?)?;
        return Ok(());
    }

    if current_passphrase.is_some() && new_passphrase.is_none() {
        decrypt_encrypted_database(current_passphrase.context("missing current passphrase")?)?;
        return Ok(());
    }

    let conn = open_connection(current_passphrase)?;
    apply_sqlcipher_rekey(&conn, new_passphrase)?;

    let path = db_path()?;
    drop(conn);

    let verify_conn = Connection::open(&path)
        .with_context(|| format!("failed to re-open database {}", path.display()))?;
    configure_sqlcipher_logging(&verify_conn);
    if let Some(passphrase) = new_passphrase {
        apply_sqlcipher_key(&verify_conn, passphrase)?;
    }
    validate_connection(&verify_conn)?;

    Ok(())
}

fn open_connection_for_write(
    encryption: DbEncryption,
    passphrase: Option<&str>,
) -> Result<Connection> {
    match encryption {
        DbEncryption::None => open_connection(None),
        DbEncryption::Passphrase => {
            let passphrase = passphrase.context("missing passphrase for encrypted database")?;
            open_connection(Some(passphrase))
        }
    }
}

fn open_connection(passphrase: Option<&str>) -> Result<Connection> {
    let path = db_path()?;
    let conn = Connection::open(&path)
        .with_context(|| format!("failed to open database {}", path.display()))?;
    configure_sqlcipher_logging(&conn);

    if let Some(passphrase) = passphrase {
        apply_sqlcipher_key(&conn, passphrase)?;
    }

    validate_connection(&conn)?;
    sql_migrations::apply(&conn)?;
    Ok(conn)
}

fn validate_connection(conn: &Connection) -> Result<()> {
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()))
        .map_err(map_sql_error)
}

fn apply_sqlcipher_key(conn: &Connection, passphrase: &str) -> Result<()> {
    let escaped = passphrase.replace('\'', "''");
    conn.execute_batch(&format!("PRAGMA key = '{escaped}';"))
        .map_err(map_sql_error)
}

fn apply_sqlcipher_rekey(conn: &Connection, passphrase: Option<&str>) -> Result<()> {
    let escaped = passphrase.unwrap_or_default().replace('\'', "''");
    conn.execute_batch(&format!("PRAGMA rekey = '{escaped}';"))
        .map_err(map_sql_error)
}

fn configure_sqlcipher_logging(conn: &Connection) {
    let _ = conn.execute_batch("PRAGMA cipher_log = OFF;");
    let _ = conn.execute_batch("PRAGMA cipher_log_level = 0;");
}

fn encrypt_plaintext_database(new_passphrase: &str) -> Result<()> {
    let path = db_path()?;
    let conn = open_connection(None)?;

    let tmp_path = path.with_extension("sqlite.enc_tmp");
    if tmp_path.exists() {
        fs::remove_file(&tmp_path).with_context(|| {
            format!(
                "failed to remove existing temporary database {}",
                tmp_path.display()
            )
        })?;
    }

    let escaped_tmp_path = tmp_path.to_string_lossy().replace('\'', "''");
    let escaped_passphrase = new_passphrase.replace('\'', "''");
    conn.execute_batch(&format!(
        "ATTACH DATABASE '{escaped_tmp_path}' AS encrypted KEY '{escaped_passphrase}';\
         SELECT sqlcipher_export('encrypted');\
         DETACH DATABASE encrypted;"
    ))
    .map_err(map_sql_error)?;

    drop(conn);

    replace_database_file(&path, &tmp_path)?;

    let verify_conn = Connection::open(&path)
        .with_context(|| format!("failed to re-open encrypted database {}", path.display()))?;
    configure_sqlcipher_logging(&verify_conn);
    apply_sqlcipher_key(&verify_conn, new_passphrase)?;
    validate_connection(&verify_conn)?;

    Ok(())
}

fn decrypt_encrypted_database(current_passphrase: &str) -> Result<()> {
    let path = db_path()?;
    let conn = open_connection(Some(current_passphrase))?;

    let tmp_path = path.with_extension("sqlite.dec_tmp");
    if tmp_path.exists() {
        fs::remove_file(&tmp_path).with_context(|| {
            format!(
                "failed to remove existing temporary database {}",
                tmp_path.display()
            )
        })?;
    }

    let escaped_tmp_path = tmp_path.to_string_lossy().replace('\'', "''");
    conn.execute_batch(&format!(
        "ATTACH DATABASE '{escaped_tmp_path}' AS plaintext KEY '';\
         SELECT sqlcipher_export('plaintext');\
         DETACH DATABASE plaintext;"
    ))
    .map_err(map_sql_error)?;

    drop(conn);

    replace_database_file(&path, &tmp_path)?;

    let verify_conn = Connection::open(&path)
        .with_context(|| format!("failed to re-open decrypted database {}", path.display()))?;
    configure_sqlcipher_logging(&verify_conn);
    validate_connection(&verify_conn)?;

    Ok(())
}

fn replace_database_file(path: &PathBuf, tmp_path: &PathBuf) -> Result<()> {
    let backup_path = path.with_extension("sqlite.rekey_backup");
    if backup_path.exists() {
        fs::remove_file(&backup_path).with_context(|| {
            format!(
                "failed to remove existing backup database {}",
                backup_path.display()
            )
        })?;
    }

    fs::rename(path, &backup_path).with_context(|| {
        format!(
            "failed to move original database {} to backup {}",
            path.display(),
            backup_path.display()
        )
    })?;

    if let Err(err) = fs::rename(tmp_path, path) {
        let _ = fs::rename(&backup_path, path);
        return Err(anyhow::anyhow!(
            "failed to finalize database replacement {}: {}",
            path.display(),
            err
        ));
    }

    fs::remove_file(&backup_path).with_context(|| {
        format!(
            "failed to remove rekey backup database {}",
            backup_path.display()
        )
    })?;

    Ok(())
}

fn load_or_init_config(conn: &Connection) -> Result<Config> {
    let maybe_config = conn
        .query_row(
            "SELECT
                enable_telemetry,
                telemetry_uuid,
                last_telemetry_report_at_unix_ms,
                db_encryption,
                show_debug_panel,
                ssh_idle_timeout_seconds,
                ssh_connect_timeout_seconds
             FROM app_config WHERE id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, Option<i64>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            },
        )
        .optional()
        .map_err(map_sql_error)?;

    if let Some((
        enable_telemetry,
        telemetry_uuid,
        last_telemetry_report_at_unix_ms,
        db_encryption,
        show_debug_panel,
        ssh_idle_timeout_seconds,
        ssh_connect_timeout_seconds,
    )) = maybe_config
    {
        let config = Config {
            enable_telemetry: enable_telemetry.map(|v| v != 0),
            telemetry_uuid,
            last_telemetry_report_at_unix_ms: last_telemetry_report_at_unix_ms
                .map(|v| v.max(0) as u64),
            db_encryption: db_encryption
                .as_deref()
                .map(parse_db_encryption)
                .transpose()?,
            show_debug_panel: show_debug_panel != 0,
            ssh_idle_timeout_seconds: ssh_idle_timeout_seconds.max(1) as u64,
            ssh_connect_timeout_seconds: ssh_connect_timeout_seconds.max(1) as u64,
        };
        return Ok(config);
    }

    let config = Config::default();
    save_config(conn, &config)?;
    Ok(config)
}

fn load_database(conn: &Connection) -> Result<Database> {
    let next_host_id = read_next_host_id(conn)?;

    let mut hosts_stmt = conn
        .prepare("SELECT id, name, user, auth_json_value FROM hosts ORDER BY id ASC")
        .map_err(map_sql_error)?;
    let host_rows = hosts_stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(map_sql_error)?;

    let mut hosts = Vec::new();
    for row in host_rows {
        let (id, name, user, auth_json) = row.map_err(map_sql_error)?;
        let auth: HostAuth =
            serde_json::from_str(&auth_json).context("failed to parse host auth JSON payload")?;
        let endpoints = load_host_endpoints(conn, id as u32)?;

        hosts.push(SshHost {
            id: id as u32,
            name,
            user,
            endpoints,
            auth,
        });
    }

    let mut trusted_stmt = conn
        .prepare(
            "SELECT host, port, algorithm, public_key_base64, fingerprint_sha256
             FROM trusted_host_keys
             ORDER BY host ASC, port ASC",
        )
        .map_err(map_sql_error)?;
    let trusted_rows = trusted_stmt
        .query_map([], |row| {
            Ok(TrustedHostKey {
                host: row.get(0)?,
                port: row.get::<_, i64>(1)? as u16,
                algorithm: row.get(2)?,
                public_key_base64: row.get(3)?,
                fingerprint_sha256: row.get(4)?,
            })
        })
        .map_err(map_sql_error)?;

    let mut trusted_host_keys = Vec::new();
    for row in trusted_rows {
        trusted_host_keys.push(row.map_err(map_sql_error)?);
    }

    Ok(Database {
        hosts,
        next_host_id,
        trusted_host_keys,
    })
}

fn load_host_endpoints(conn: &Connection, host_id: u32) -> Result<Vec<SshEndpoint>> {
    let mut stmt = conn
        .prepare(
            "SELECT host, port
             FROM host_endpoints
             WHERE host_id = ?1
             ORDER BY endpoint_index ASC",
        )
        .map_err(map_sql_error)?;
    let rows = stmt
        .query_map(params![host_id], |row| {
            Ok(SshEndpoint {
                host: row.get(0)?,
                port: row.get::<_, i64>(1)? as u16,
            })
        })
        .map_err(map_sql_error)?;

    let mut endpoints = Vec::new();
    for row in rows {
        endpoints.push(row.map_err(map_sql_error)?);
    }
    Ok(endpoints)
}

fn save_database(conn: &Connection, db: &Database) -> Result<()> {
    let tx = conn.unchecked_transaction().map_err(map_sql_error)?;
    tx.execute("DELETE FROM host_endpoints", [])
        .map_err(map_sql_error)?;
    tx.execute("DELETE FROM hosts", []).map_err(map_sql_error)?;
    tx.execute("DELETE FROM trusted_host_keys", [])
        .map_err(map_sql_error)?;

    for host in &db.hosts {
        let auth_json = serde_json::to_string(&host.auth).context("failed to encode host auth")?;
        tx.execute(
            "INSERT INTO hosts(id, name, user, auth_kind, auth_json_value) VALUES(?1, ?2, ?3, ?4, ?5)",
            params![host.id, host.name, host.user, auth_kind(&host.auth), auth_json],
        )
        .map_err(map_sql_error)?;

        for (idx, endpoint) in host.endpoints.iter().enumerate() {
            tx.execute(
                "INSERT INTO host_endpoints(host_id, endpoint_index, host, port) VALUES(?1, ?2, ?3, ?4)",
                params![host.id, idx as i64, endpoint.host, endpoint.port],
            )
            .map_err(map_sql_error)?;
        }
    }

    for key in &db.trusted_host_keys {
        tx.execute(
            "INSERT INTO trusted_host_keys(host, port, algorithm, public_key_base64, fingerprint_sha256)
             VALUES(?1, ?2, ?3, ?4, ?5)",
            params![
                key.host,
                key.port,
                key.algorithm,
                key.public_key_base64,
                key.fingerprint_sha256
            ],
        )
        .map_err(map_sql_error)?;
    }

    let next_host_id = db.next_host_id.max(max_host_id_plus_one(&db.hosts));
    tx.execute(
        "INSERT INTO app_meta(key, json_value) VALUES('next_host_id', ?1)
         ON CONFLICT(key) DO UPDATE SET json_value = excluded.json_value",
        params![serde_json::to_string(&next_host_id)?],
    )
    .map_err(map_sql_error)?;

    tx.commit().map_err(map_sql_error)?;
    Ok(())
}

fn save_config(conn: &Connection, config: &Config) -> Result<()> {
    conn.execute(
        "INSERT INTO app_config(
            id,
            enable_telemetry,
            telemetry_uuid,
            last_telemetry_report_at_unix_ms,
            db_encryption,
            show_debug_panel,
            ssh_idle_timeout_seconds,
            ssh_connect_timeout_seconds
        ) VALUES(1, ?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ON CONFLICT(id) DO UPDATE SET
            enable_telemetry = excluded.enable_telemetry,
            telemetry_uuid = excluded.telemetry_uuid,
            last_telemetry_report_at_unix_ms = excluded.last_telemetry_report_at_unix_ms,
            db_encryption = excluded.db_encryption,
            show_debug_panel = excluded.show_debug_panel,
            ssh_idle_timeout_seconds = excluded.ssh_idle_timeout_seconds,
            ssh_connect_timeout_seconds = excluded.ssh_connect_timeout_seconds",
        params![
            config
                .enable_telemetry
                .map(|v| if v { 1_i64 } else { 0_i64 }),
            config.telemetry_uuid.as_deref(),
            config.last_telemetry_report_at_unix_ms.map(|v| v as i64),
            config.db_encryption.as_ref().map(db_encryption_to_str),
            if config.show_debug_panel {
                1_i64
            } else {
                0_i64
            },
            config.ssh_idle_timeout_seconds as i64,
            config.ssh_connect_timeout_seconds as i64,
        ],
    )
    .map_err(map_sql_error)?;
    Ok(())
}

fn parse_db_encryption(value: &str) -> Result<DbEncryption> {
    match value {
        "none" => Ok(DbEncryption::None),
        "passphrase" => Ok(DbEncryption::Passphrase),
        _ => anyhow::bail!("invalid db_encryption value: {value}"),
    }
}

fn db_encryption_to_str(value: &DbEncryption) -> &'static str {
    match value {
        DbEncryption::None => "none",
        DbEncryption::Passphrase => "passphrase",
    }
}

fn read_next_host_id(conn: &Connection) -> Result<u32> {
    let value: Option<String> = conn
        .query_row(
            "SELECT json_value FROM app_meta WHERE key = 'next_host_id'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(map_sql_error)?;

    if let Some(value) = value {
        let parsed: u32 = serde_json::from_str(&value).context("invalid next_host_id value")?;
        return Ok(parsed.max(1));
    }

    Ok(1)
}

fn auth_kind(auth: &HostAuth) -> &'static str {
    match auth {
        HostAuth::KeyPath { .. } => "key_path",
        HostAuth::KeyInline { .. } => "key_inline",
        HostAuth::Password { .. } => "password",
    }
}

fn max_host_id_plus_one(hosts: &[SshHost]) -> u32 {
    hosts
        .iter()
        .map(|host| host.id.saturating_add(1))
        .max()
        .unwrap_or(1)
        .max(1)
}

fn map_sql_error(err: SqlError) -> anyhow::Error {
    anyhow::anyhow!("database error: {err}")
}

fn is_wrong_passphrase_error(err: &anyhow::Error) -> bool {
    let msg = err.to_string();
    msg.contains("file is not a database") || msg.contains("file is encrypted or is not a database")
}
