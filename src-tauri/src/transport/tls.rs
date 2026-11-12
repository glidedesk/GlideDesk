// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Encrypted Local Transport & TLS Pinning via Rustls (BEAM_AUTH_V1 Pattern)

use sha2::{Digest, Sha256};
use std::sync::Arc;
use parking_lot::Mutex;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::client::danger::{ServerCertVerifier, HandshakeSignatureValid, ServerCertVerified};
use rustls::{DigitallySignedStruct, Error, SignatureScheme, ServerConfig, ClientConfig};

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    let clean = s.trim();
    if clean.len() % 2 != 0 {
        return None;
    }
    (0..clean.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&clean[i..i + 2], 16).ok())
        .collect()
}

pub struct LocalTlsIdentity {    pub cert_der: Vec<u8>,
    pub key_der: Vec<u8>,
    pub fingerprint_sha256: String,
}

pub fn ensure_crypto_provider() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

impl LocalTlsIdentity {
    pub fn generate() -> Result<Self, String> {
        let certified = rcgen::generate_simple_self_signed(vec![
            "localhost".to_string(),
            "glidedesk.local".to_string(),
            "GlideDesk Local Transport".to_string(),
        ])
        .map_err(|e| format!("Failed to generate self-signed TLS cert: {}", e))?;

        let cert_der = certified.cert.der().to_vec();
        let key_der = certified.signing_key.serialized_der().to_vec();
        Ok(Self::from_der(cert_der, key_der))
    }

    fn from_der(cert_der: Vec<u8>, key_der: Vec<u8>) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(&cert_der);
        let hash = hasher.finalize();
        let fingerprint = hash
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect::<Vec<_>>()
            .join(":");

        Self {
            cert_der,
            key_der,
            fingerprint_sha256: fingerprint,
        }
    }

    /// This install's identity: made once, then kept. A fresh certificate on every launch
    /// would break the pin a paired computer stored, so the pair could never reconnect.
    pub fn load_or_generate() -> Result<Self, String> {
        let path = crate::pairstore::identity_path();
        if let Ok(text) = std::fs::read_to_string(&path) {
            let parsed = serde_json::from_str::<serde_json::Value>(&text).ok();
            let cert = parsed.as_ref().and_then(|v| v.get("cert")).and_then(|v| v.as_str()).and_then(hex_decode);
            let key = parsed.as_ref().and_then(|v| v.get("key")).and_then(|v| v.as_str()).and_then(hex_decode);
            if let (Some(cert_der), Some(key_der)) = (cert, key) {
                return Ok(Self::from_der(cert_der, key_der));
            }
            log::warn!("[TLS] Ignoring unreadable identity at {}", path.display());
        }
        let id = Self::generate()?;
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let body = serde_json::json!({
            "cert": hex_encode(&id.cert_der),
            "key": hex_encode(&id.key_der),
        })
        .to_string();
        if let Err(e) = std::fs::write(&path, body) {
            log::warn!("[TLS] Could not save identity to {}: {}", path.display(), e);
        } else {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
            }
        }
        Ok(id)
    }

    pub fn server_config(&self) -> Result<ServerConfig, String> {
        ensure_crypto_provider();
        let cert_chain = vec![CertificateDer::from(self.cert_der.clone())];
        let key = PrivateKeyDer::Pkcs8(self.key_der.clone().into());

        ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(cert_chain, key)
            .map_err(|e| format!("Failed to build TLS ServerConfig: {}", e))
    }
}

#[derive(Debug, Clone)]
pub struct PinnedCertVerifier {
    pub expected_fingerprint_hex: String,
    pub peer_id: Option<String>,
    pub captured_fingerprint: Arc<Mutex<Option<String>>>,
}

impl PinnedCertVerifier {
    pub fn new(expected_fingerprint: &str, peer_id: Option<&str>) -> Self {
        Self {
            expected_fingerprint_hex: expected_fingerprint.to_string(),
            peer_id: peer_id.map(|s| s.to_string()),
            captured_fingerprint: Arc::new(Mutex::new(None)),
        }
    }
}

impl ServerCertVerifier for PinnedCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        let digest = Sha256::digest(end_entity.as_ref());
        let actual_hex = digest
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect::<Vec<_>>()
            .join(":");

        *self.captured_fingerprint.lock() = Some(actual_hex.clone());

        let clean_expected = self.expected_fingerprint_hex.replace(':', "").to_uppercase();
        let clean_actual = actual_hex.replace(':', "").to_uppercase();

        if clean_expected.is_empty() {
            log::info!("[TLS Pinning] Pin on first use: captured peer certificate fingerprint {}", actual_hex);
            return Ok(ServerCertVerified::assertion());
        }

        if clean_expected == clean_actual {
            log::info!("[TLS Pinning] Peer certificate fingerprint matched: {}", actual_hex);
            Ok(ServerCertVerified::assertion())
        } else {
            let err_msg = if let Some(ref pid) = self.peer_id {
                format!(
                    "certificate fingerprint changed for peer {}; possible MITM — re-pair to trust",
                    pid
                )
            } else {
                format!(
                    "Peer certificate fingerprint mismatch! Expected {}, got {}",
                    self.expected_fingerprint_hex, actual_hex
                )
            };
            log::error!("[TLS Pinning] {}", err_msg);
            Err(Error::General(err_msg))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::aws_lc_rs::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::aws_lc_rs::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        rustls::crypto::aws_lc_rs::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

pub fn create_pinned_client_config(expected_fingerprint: &str) -> ClientConfig {
    create_pinned_client_config_with_peer(expected_fingerprint, None, None)
}

pub fn create_pinned_client_config_with_peer(
    expected_fingerprint: &str,
    peer_id: Option<&str>,
    captured: Option<Arc<Mutex<Option<String>>>>,
) -> ClientConfig {
    ensure_crypto_provider();
    let verifier = Arc::new(PinnedCertVerifier {
        expected_fingerprint_hex: expected_fingerprint.to_string(),
        peer_id: peer_id.map(|s| s.to_string()),
        captured_fingerprint: captured.unwrap_or_else(|| Arc::new(Mutex::new(None))),
    });

    ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientTransportDecision {
    Tls {
        wss_url: String,
        host: String,
        port: u16,
        expected_fingerprint: Option<String>,
    },
    Plaintext {
        ws_url: String,
        host: String,
        port: u16,
    },
}

pub fn decide_client_transport(
    target_addr: &str,
    pinned_fingerprint: Option<&str>,
    allow_plaintext: bool,
) -> Result<ClientTransportDecision, String> {
    let clean_addr = target_addr.trim();
    let is_explicit_ws = clean_addr.starts_with("ws://");

    if is_explicit_ws && !allow_plaintext {
        return Err("peer does not support encrypted transport; refusing plaintext".to_string());
    }

    let addr_without_scheme = clean_addr
        .trim_start_matches("wss://")
        .trim_start_matches("ws://")
        .trim_end_matches('/');

    let (host, port) = if let Some(idx) = addr_without_scheme.rfind(':') {
        let (h, p) = addr_without_scheme.split_at(idx);
        let port_num = p[1..].parse::<u16>().unwrap_or(47800);
        (h.to_string(), port_num)
    } else {
        (addr_without_scheme.to_string(), 47800)
    };

    if is_explicit_ws && allow_plaintext {
        log::warn!(
            "[TLS Transport] INSECURE: Connecting via unencrypted plaintext to ws://{}:{} (allow_plaintext=true)",
            host, port
        );
        return Ok(ClientTransportDecision::Plaintext {
            ws_url: format!("ws://{}:{}", host, port),
            host,
            port,
        });
    }

    Ok(ClientTransportDecision::Tls {
        wss_url: format!("wss://{}:{}", host, port),
        host,
        port,
        expected_fingerprint: pinned_fingerprint.map(|s| s.to_string()),
    })
}

pub fn format_beam_auth_token(pairing_token: &str, fingerprint_sha256: &str) -> String {
    let clean_fp = fingerprint_sha256.replace(':', "").to_lowercase();
    format!("BEAM_AUTH_V1:{}:{}", pairing_token, clean_fp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_tls_identity_generation_and_fingerprint() {
        let identity = LocalTlsIdentity::generate().expect("generate self-signed cert");
        assert!(!identity.cert_der.is_empty());
        assert!(!identity.key_der.is_empty());
        assert_eq!(identity.fingerprint_sha256.split(':').count(), 32);

        // Verify fingerprint hash is accurate
        let mut hasher = Sha256::new();
        hasher.update(&identity.cert_der);
        let expected = hasher.finalize();
        let expected_fp = expected
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect::<Vec<_>>()
            .join(":");
        assert_eq!(identity.fingerprint_sha256, expected_fp);

        // Verify ServerConfig builds without errors
        let srv_cfg = identity.server_config();
        assert!(srv_cfg.is_ok());
    }

    #[test]
    fn test_pinned_cert_verifier_match_and_mismatch() {
        let identity = LocalTlsIdentity::generate().expect("generate cert");
        let cert_der = CertificateDer::from(identity.cert_der.clone());

        // 1. Correct fingerprint passes
        let verifier = PinnedCertVerifier::new(&identity.fingerprint_sha256, None);
        let dummy_name = ServerName::try_from("localhost").unwrap();
        let verify_ok = verifier.verify_server_cert(
            &cert_der,
            &[],
            &dummy_name,
            &[],
            UnixTime::now(),
        );
        assert!(verify_ok.is_ok());

        // 2. Mismatched fingerprint fails
        let wrong_verifier = PinnedCertVerifier::new(
            "00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF",
            None,
        );
        let verify_err = wrong_verifier.verify_server_cert(
            &cert_der,
            &[],
            &dummy_name,
            &[],
            UnixTime::now(),
        );
        assert!(verify_err.is_err());
    }

    #[test]
    fn test_pinned_cert_verifier_changed_fingerprint_refusal() {
        let identity = LocalTlsIdentity::generate().expect("generate cert");
        let cert_der = CertificateDer::from(identity.cert_der.clone());

        let peer_id = "peer_mac_alpha";
        let wrong_fp = "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99";
        let verifier = PinnedCertVerifier::new(wrong_fp, Some(peer_id));
        let dummy_name = ServerName::try_from("localhost").unwrap();

        let res = verifier.verify_server_cert(
            &cert_der,
            &[],
            &dummy_name,
            &[],
            UnixTime::now(),
        );

        assert!(res.is_err());
        let err_str = format!("{:?}", res.err().unwrap());
        assert!(
            err_str.contains("certificate fingerprint changed for peer peer_mac_alpha; possible MITM — re-pair to trust"),
            "Expected MITM error message but got: {}",
            err_str
        );
    }

    #[test]
    fn test_pinned_cert_verifier_rejects_forged_handshake_signature() {
        use rustls::internal::msgs::codec::{Codec, Reader};
        ensure_crypto_provider();
        let identity = LocalTlsIdentity::generate().expect("generate cert");
        let cert_der = CertificateDer::from(identity.cert_der.clone());
        let verifier = PinnedCertVerifier::new(&identity.fingerprint_sha256, None);

        // Construct wire-encoded forged signature:
        // scheme ECDSA_NISTP256_SHA256 = 0x0403 (big-endian), len = 6, bytes = 0xAA, 0xBB, ...
        let wire_bytes = vec![0x04, 0x03, 0x00, 0x06, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
        let mut reader = Reader::init(&wire_bytes);
        let forged_dss = DigitallySignedStruct::read(&mut reader).expect("decode DigitallySignedStruct");

        // Handshake verification MUST reject forged signature
        let verify_res = verifier.verify_tls13_signature(
            b"handshake transcript data to authenticate",
            &cert_der,
            &forged_dss,
        );
        assert!(verify_res.is_err(), "Forged signature must be rejected by real cryptographic verification");
    }

    #[test]
    fn test_plaintext_only_peer_refused_by_default() {
        // Plaintext URL refused when allow_plaintext is false
        let decision_default = decide_client_transport("ws://192.168.1.100:47800", None, false);
        assert!(decision_default.is_err());
        assert_eq!(
            decision_default.err().unwrap(),
            "peer does not support encrypted transport; refusing plaintext"
        );

        // Default host:port string targets encrypted wss transport
        let decision_tls = decide_client_transport("192.168.1.100:47800", Some("AA:BB:CC"), false).unwrap();
        match decision_tls {
            ClientTransportDecision::Tls { wss_url, host, port, expected_fingerprint } => {
                assert_eq!(wss_url, "wss://192.168.1.100:47800");
                assert_eq!(host, "192.168.1.100");
                assert_eq!(port, 47800);
                assert_eq!(expected_fingerprint.as_deref(), Some("AA:BB:CC"));
            }
            ClientTransportDecision::Plaintext { .. } => panic!("Expected TLS decision"),
        }

        // Opt-out config flag explicitly allows plaintext
        let decision_opt_out = decide_client_transport("ws://192.168.1.100:47800", None, true).unwrap();
        match decision_opt_out {
            ClientTransportDecision::Plaintext { ws_url, host, port } => {
                assert_eq!(ws_url, "ws://192.168.1.100:47800");
                assert_eq!(host, "192.168.1.100");
                assert_eq!(port, 47800);
            }
            ClientTransportDecision::Tls { .. } => panic!("Expected Plaintext decision"),
        }
    }

    #[test]
    fn test_tls_handshake_pinned_match_and_changed_refusal() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            ensure_crypto_provider();

            // 1. Server identity
            let server_identity = LocalTlsIdentity::generate().expect("server identity");
            let server_cfg = Arc::new(server_identity.server_config().expect("server config"));
            let acceptor = tokio_rustls::TlsAcceptor::from(server_cfg);

            // Bind ephemeral loopback port
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind listener");
            let server_addr = listener.local_addr().expect("local addr");

            // Spawn server accept task
            let srv_handle = tokio::spawn(async move {
                let mut accepted_count = 0;
                while accepted_count < 2 {
                    if let Ok((stream, _)) = listener.accept().await {
                        accepted_count += 1;
                        let acc = acceptor.clone();
                        tokio::spawn(async move {
                            let _ = acc.accept(stream).await;
                        });
                    }
                }
            });

            // 2. Client 1 with matching pinned fingerprint connects
            let client_cfg_ok = Arc::new(create_pinned_client_config(&server_identity.fingerprint_sha256));
            let connector_ok = tokio_rustls::TlsConnector::from(client_cfg_ok);
            let stream_ok = tokio::net::TcpStream::connect(server_addr).await.expect("connect stream");
            let server_name = ServerName::try_from("localhost").unwrap();
            let tls_client_ok = connector_ok.connect(server_name.clone(), stream_ok).await;
            assert!(tls_client_ok.is_ok(), "TLS handshake with matching pinned cert must succeed");

            // 3. Client 2 with changed fingerprint (MITM) connects and MUST be rejected
            let captured = Arc::new(Mutex::new(None));
            let changed_fp = "FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF:FF";
            let client_cfg_mitm = Arc::new(create_pinned_client_config_with_peer(
                changed_fp,
                Some("host_peer_01"),
                Some(captured),
            ));
            let connector_mitm = tokio_rustls::TlsConnector::from(client_cfg_mitm);
            let stream_mitm = tokio::net::TcpStream::connect(server_addr).await.expect("connect stream");
            let tls_client_mitm = connector_mitm.connect(server_name, stream_mitm).await;

            assert!(tls_client_mitm.is_err(), "TLS handshake with changed fingerprint must be rejected");
            let err_text = tls_client_mitm.err().unwrap().to_string();
            assert!(
                err_text.contains("certificate fingerprint changed for peer host_peer_01; possible MITM — re-pair to trust"),
                "Error text must indicate MITM / changed fingerprint: {}",
                err_text
            );

            let _ = srv_handle.await;
        });
    }

    #[test]
    fn test_beam_auth_token_format() {
        let token = format_beam_auth_token("secret_pair_tok", "AA:BB:CC:DD");
        assert_eq!(token, "BEAM_AUTH_V1:secret_pair_tok:aabbccdd");
    }
}
