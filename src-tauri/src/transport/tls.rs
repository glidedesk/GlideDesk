// GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
// Encrypted Local Transport & TLS Pinning via Rustls (BEAM_AUTH_V1 Pattern)

use sha2::{Digest, Sha256};
use std::sync::Arc;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::client::danger::{ServerCertVerifier, HandshakeSignatureValid, ServerCertVerified};
use rustls::{DigitallySignedStruct, Error, SignatureScheme, ServerConfig, ClientConfig};

pub struct LocalTlsIdentity {
    pub cert_der: Vec<u8>,
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

        let mut hasher = Sha256::new();
        hasher.update(&cert_der);
        let hash = hasher.finalize();
        let fingerprint = hash
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect::<Vec<_>>()
            .join(":");

        Ok(Self {
            cert_der,
            key_der,
            fingerprint_sha256: fingerprint,
        })
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

#[derive(Debug)]
pub struct PinnedCertVerifier {
    pub expected_fingerprint_hex: String,
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

        let clean_expected = self.expected_fingerprint_hex.replace(':', "").to_uppercase();
        let clean_actual = actual_hex.replace(':', "").to_uppercase();

        if clean_expected == clean_actual {
            log::info!("[TLS Pinning] Peer certificate fingerprint matched: {}", actual_hex);
            Ok(ServerCertVerified::assertion())
        } else {
            log::error!(
                "[TLS Pinning] Fingerprint mismatch! Expected {}, got {}",
                self.expected_fingerprint_hex, actual_hex
            );
            Err(Error::General(format!(
                "Peer certificate fingerprint mismatch! Expected {}, got {}",
                self.expected_fingerprint_hex, actual_hex
            )))
        }
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::ED25519,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PKCS1_SHA256,
        ]
    }
}

pub fn create_pinned_client_config(expected_fingerprint: &str) -> ClientConfig {
    ensure_crypto_provider();
    let verifier = Arc::new(PinnedCertVerifier {
        expected_fingerprint_hex: expected_fingerprint.to_string(),
    });

    ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth()
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
        let verifier = PinnedCertVerifier {
            expected_fingerprint_hex: identity.fingerprint_sha256.clone(),
        };
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
        let wrong_verifier = PinnedCertVerifier {
            expected_fingerprint_hex: "00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF".to_string(),
        };
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
    fn test_beam_auth_token_format() {
        let token = format_beam_auth_token("secret_pair_tok", "AA:BB:CC:DD");
        assert_eq!(token, "BEAM_AUTH_V1:secret_pair_tok:aabbccdd");
    }
}
