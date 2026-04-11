use std::{
    env,
    sync::{Arc, Mutex, mpsc, mpsc::TryRecvError},
    thread,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use backend::{HostAuth, SshHost, TrustedHostKey};
use base64::Engine;
use russh::{
    ChannelMsg,
    client::{self, AuthResult},
    keys::{PrivateKeyWithHashAlg, decode_secret_key, load_secret_key, ssh_key::HashAlg},
};
use tokio::sync::mpsc as tokio_mpsc;

#[derive(Debug, Clone)]
pub(crate) struct TrustChallenge {
    pub(crate) proposed_key: TrustedHostKey,
    pub(crate) previous_fingerprint: Option<String>,
}

#[derive(Debug)]
pub(crate) enum SessionEvent {
    OutputBytes(Vec<u8>),
    Error(String),
    Closed(String),
}

#[derive(Debug)]
pub(crate) enum SessionInput {
    Data(Vec<u8>),
    Resize { cols: u16, rows: u16 },
    Disconnect,
}

pub(crate) struct LiveSshSession {
    input_tx: tokio_mpsc::UnboundedSender<SessionInput>,
    event_rx: mpsc::Receiver<SessionEvent>,
    join: Option<thread::JoinHandle<()>>,
}

impl LiveSshSession {
    pub(crate) fn send_input(&self, input: SessionInput) {
        let _ = self.input_tx.send(input);
    }

    pub(crate) fn try_recv(&mut self) -> Option<SessionEvent> {
        self.event_rx.try_recv().ok()
    }

    pub(crate) fn stop(&mut self) {
        let _ = self.input_tx.send(SessionInput::Disconnect);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

pub(crate) enum StartSessionResult {
    Started(LiveSshSession),
    TrustRequired(TrustChallenge),
    Error(String),
}

pub(crate) struct PendingSshStart {
    input_tx: Option<tokio_mpsc::UnboundedSender<SessionInput>>,
    event_rx: Option<mpsc::Receiver<SessionEvent>>,
    ready_rx: mpsc::Receiver<SessionReady>,
    join: Option<thread::JoinHandle<()>>,
}

impl PendingSshStart {
    pub(crate) fn try_recv(&mut self) -> Option<StartSessionResult> {
        match self.ready_rx.try_recv() {
            Ok(SessionReady::Started) => {
                let input_tx = self
                    .input_tx
                    .take()
                    .expect("pending SSH start missing input channel");
                let event_rx = self
                    .event_rx
                    .take()
                    .expect("pending SSH start missing event channel");

                Some(StartSessionResult::Started(LiveSshSession {
                    input_tx,
                    event_rx,
                    join: self.join.take(),
                }))
            }
            Ok(SessionReady::TrustRequired(challenge)) => {
                if let Some(join) = self.join.take() {
                    let _ = join.join();
                }
                Some(StartSessionResult::TrustRequired(challenge))
            }
            Ok(SessionReady::Error(error)) => {
                if let Some(join) = self.join.take() {
                    let _ = join.join();
                }
                Some(StartSessionResult::Error(error))
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                if let Some(join) = self.join.take() {
                    let _ = join.join();
                }
                Some(StartSessionResult::Error(
                    "SSH session failed to start".to_string(),
                ))
            }
        }
    }

    pub(crate) fn cancel(&mut self) {
        if let Some(input_tx) = &self.input_tx {
            let _ = input_tx.send(SessionInput::Disconnect);
        }
        self.input_tx.take();
        self.event_rx.take();
        self.join.take();
    }
}

#[derive(Debug)]
enum SessionReady {
    Started,
    TrustRequired(TrustChallenge),
    Error(String),
}

#[derive(Debug, Clone)]
struct SharedVerificationState {
    trust_challenge: Option<TrustChallenge>,
}

impl SharedVerificationState {
    fn new() -> Self {
        Self {
            trust_challenge: None,
        }
    }
}

#[derive(Debug, Clone)]
struct VerifyHandler {
    host: String,
    port: u16,
    trusted_host_keys: Vec<TrustedHostKey>,
    shared: Arc<Mutex<SharedVerificationState>>,
}

impl client::Handler for VerifyHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        let incoming = match map_server_key(&self.host, self.port, server_public_key) {
            Ok(key) => key,
            Err(_) => return Ok(false),
        };

        let trusted_for_host = self
            .trusted_host_keys
            .iter()
            .filter(|k| k.host == self.host && k.port == self.port)
            .collect::<Vec<_>>();

        if trusted_for_host.is_empty() {
            if let Ok(mut shared) = self.shared.lock() {
                shared.trust_challenge = Some(TrustChallenge {
                    proposed_key: incoming,
                    previous_fingerprint: None,
                });
            }
            return Ok(false);
        }

        if trusted_for_host.iter().any(|k| {
            k.algorithm == incoming.algorithm && k.public_key_base64 == incoming.public_key_base64
        }) {
            return Ok(true);
        }

        if let Ok(mut shared) = self.shared.lock() {
            shared.trust_challenge = Some(TrustChallenge {
                proposed_key: incoming,
                previous_fingerprint: Some(trusted_for_host[0].fingerprint_sha256.clone()),
            });
        }

        Ok(false)
    }
}

pub(crate) fn start_session_async(
    host: &SshHost,
    selected_endpoint_index: Option<usize>,
    trusted_host_keys: &[TrustedHostKey],
    rows: u16,
    cols: u16,
    inactivity_timeout_seconds: u64,
    connect_timeout_seconds: u64,
) -> PendingSshStart {
    let (input_tx, input_rx) = tokio_mpsc::unbounded_channel::<SessionInput>();
    let (event_tx, event_rx) = mpsc::channel::<SessionEvent>();
    let (ready_tx, ready_rx) = mpsc::channel::<SessionReady>();

    let host = host.clone();
    let trusted_host_keys = trusted_host_keys.to_vec();

    let join = thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_multi_thread()
            .enable_io()
            .enable_time()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                let _ = ready_tx.send(SessionReady::Error(format!(
                    "Failed to start SSH runtime: {e}"
                )));
                return;
            }
        };

        let shared = Arc::new(Mutex::new(SharedVerificationState::new()));
        let shared_for_connect = Arc::clone(&shared);
        let ready_for_connect = ready_tx.clone();
        let event_for_connect = event_tx.clone();

        let outcome = runtime.block_on(async {
            connect_and_run(
                host,
                selected_endpoint_index,
                trusted_host_keys,
                rows,
                cols,
                inactivity_timeout_seconds,
                connect_timeout_seconds,
                input_rx,
                event_for_connect,
                shared_for_connect,
                ready_for_connect,
            )
            .await
        });

        match outcome {
            Ok(()) => {}
            Err(e) => {
                let challenge = shared.lock().ok().and_then(|s| s.trust_challenge.clone());
                if let Some(challenge) = challenge {
                    let _ = event_tx.send(SessionEvent::Closed(
                        "Connection canceled: host key verification required".to_string(),
                    ));
                    let _ = ready_tx.send(SessionReady::TrustRequired(challenge));
                } else {
                    let _ = event_tx.send(SessionEvent::Error(format!("{e}")));
                    let _ = ready_tx.send(SessionReady::Error(format!("{e}")));
                }
            }
        }
    });

    PendingSshStart {
        input_tx: Some(input_tx),
        event_rx: Some(event_rx),
        ready_rx,
        join: Some(join),
    }
}

async fn connect_and_run(
    host: SshHost,
    selected_endpoint_index: Option<usize>,
    trusted_host_keys: Vec<TrustedHostKey>,
    rows: u16,
    cols: u16,
    inactivity_timeout_seconds: u64,
    connect_timeout_seconds: u64,
    mut input_rx: tokio_mpsc::UnboundedReceiver<SessionInput>,
    event_tx: mpsc::Sender<SessionEvent>,
    shared: Arc<Mutex<SharedVerificationState>>,
    ready_tx: mpsc::Sender<SessionReady>,
) -> Result<()> {
    let idle_timeout = Duration::from_secs(inactivity_timeout_seconds.max(1));
    let connect_timeout = Duration::from_secs(connect_timeout_seconds.max(1));
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(idle_timeout),
        keepalive_interval: Some(Duration::from_secs(15)),
        keepalive_max: 3,
        ..Default::default()
    });
    let mut last_error: Option<anyhow::Error> = None;
    let mut session = None;

    let endpoints = if let Some(index) = selected_endpoint_index {
        host.endpoints
            .get(index)
            .cloned()
            .map(|endpoint| vec![endpoint])
            .unwrap_or_default()
    } else {
        host.endpoints.clone()
    };

    for endpoint in endpoints {
        let handler = VerifyHandler {
            host: endpoint.host.clone(),
            port: endpoint.port,
            trusted_host_keys: trusted_host_keys.clone(),
            shared: Arc::clone(&shared),
        };

        let addr = (endpoint.host.as_str(), endpoint.port);
        let connect_result = tokio::time::timeout(
            connect_timeout,
            client::connect(config.clone(), addr, handler),
        )
        .await;
        let mut candidate = match connect_result {
            Ok(Ok(session)) => session,
            Ok(Err(err)) => {
                last_error = Some(anyhow::anyhow!(
                    "failed to connect to {}:{}: {err}",
                    endpoint.host,
                    endpoint.port
                ));
                continue;
            }
            Err(_) => {
                last_error = Some(anyhow::anyhow!(
                    "connect timeout to {}:{} after {}s",
                    endpoint.host,
                    endpoint.port,
                    connect_timeout.as_secs()
                ));
                continue;
            }
        };

        let auth_result = match &host.auth {
            HostAuth::KeyPath { key_path } => {
                let private_key = load_secret_key(key_path, None)
                    .with_context(|| format!("failed to load private key at {key_path}"))?;
                let hash_alg = candidate
                    .best_supported_rsa_hash()
                    .await
                    .context("failed to detect RSA hash algorithm")?
                    .flatten();
                candidate
                    .authenticate_publickey(
                        host.user.clone(),
                        PrivateKeyWithHashAlg::new(Arc::new(private_key), hash_alg),
                    )
                    .await
                    .context("public key authentication failed")?
            }
            HostAuth::KeyInline { private_key } => {
                let key = decode_secret_key(private_key, None)
                    .with_context(|| "failed to parse inline private key".to_string())?;

                let hash_alg = candidate
                    .best_supported_rsa_hash()
                    .await
                    .context("failed to detect RSA hash algorithm")?
                    .flatten();
                candidate
                    .authenticate_publickey(
                        host.user.clone(),
                        PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg),
                    )
                    .await
                    .context("inline key authentication failed")?
            }
            HostAuth::Password { password } => candidate
                .authenticate_password(host.user.clone(), password.clone())
                .await
                .context("password authentication failed")?,
        };

        if matches!(auth_result, AuthResult::Success) {
            session = Some(candidate);
            break;
        }

        last_error = Some(anyhow::anyhow!(
            "authentication rejected by {}:{}",
            endpoint.host,
            endpoint.port
        ));
    }

    let Some(session) = session else {
        if let Some(challenge) = shared.lock().ok().and_then(|s| s.trust_challenge.clone()) {
            bail!(
                "host key verification required for {}:{} ({})",
                challenge.proposed_key.host,
                challenge.proposed_key.port,
                challenge.proposed_key.fingerprint_sha256
            );
        }
        if let Some(err) = last_error {
            return Err(err);
        }
        if selected_endpoint_index.is_some() {
            bail!("selected endpoint is no longer available for connection");
        }
        bail!("no endpoints available for connection");
    };

    let mut channel = session
        .channel_open_session()
        .await
        .context("failed to open SSH session channel")?;

    let (cols, rows) = (cols.max(1), rows.max(1));
    channel
        .request_pty(
            true,
            &env::var("TERM").unwrap_or_else(|_| "xterm-256color".to_string()),
            cols as u32,
            rows as u32,
            0,
            0,
            &[],
        )
        .await
        .context("failed to request PTY")?;

    channel
        .request_shell(true)
        .await
        .context("failed to request remote shell")?;

    let _ = ready_tx.send(SessionReady::Started);

    loop {
        tokio::select! {
            maybe_input = input_rx.recv() => {
                match maybe_input {
                    Some(SessionInput::Data(bytes)) => {
                        channel.data(bytes.as_slice()).await.context("failed to send input to SSH")?;
                    }
                    Some(SessionInput::Resize { cols, rows }) => {
                        let _ = channel.window_change(cols as u32, rows as u32, 0, 0).await;
                    }
                    Some(SessionInput::Disconnect) | None => {
                        channel.eof().await.ok();
                        let _ = event_tx.send(SessionEvent::Closed("SSH session disconnected".to_string()));
                        break;
                    }
                }
            }
            maybe_msg = channel.wait() => {
                let Some(msg) = maybe_msg else {
                    let _ = event_tx.send(SessionEvent::Closed(
                        format!(
                            "SSH session timed out after {}s of inactivity",
                            idle_timeout.as_secs()
                        ),
                    ));
                    break;
                };
                match msg {
                    ChannelMsg::Data { data } => {
                        let _ = event_tx.send(SessionEvent::OutputBytes(data.to_vec()));
                    }
                    ChannelMsg::ExtendedData { data, .. } => {
                        let _ = event_tx.send(SessionEvent::OutputBytes(data.to_vec()));
                    }
                    ChannelMsg::ExitStatus { exit_status } => {
                        let _ = event_tx.send(SessionEvent::Closed(format!("SSH session closed with exit status {exit_status}")));
                        break;
                    }
                    ChannelMsg::Close | ChannelMsg::Eof => {
                        let _ = event_tx.send(SessionEvent::Closed("SSH session closed".to_string()));
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    session
        .disconnect(russh::Disconnect::ByApplication, "", "English")
        .await
        .ok();
    Ok(())
}

fn map_server_key(
    host: &str,
    port: u16,
    public_key: &russh::keys::ssh_key::PublicKey,
) -> Result<TrustedHostKey> {
    let public_key_base64 = base64::engine::general_purpose::STANDARD.encode(
        public_key
            .to_bytes()
            .context("failed to encode public key bytes")?,
    );

    Ok(TrustedHostKey {
        host: host.to_string(),
        port,
        algorithm: public_key.algorithm().to_string(),
        public_key_base64,
        fingerprint_sha256: public_key.fingerprint(HashAlg::Sha256).to_string(),
    })
}
