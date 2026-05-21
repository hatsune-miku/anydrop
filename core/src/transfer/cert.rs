//! Self-signed certificate generation + TLS config builders.
//!
//! LAN-only "trust everyone" model: server presents a fresh self-signed cert,
//! client accepts any cert without verification. Suitable only for local
//! networks where you accept the risk of MITM.

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use std::sync::Arc;
use std::sync::Once;

use super::protocol::ALPN;

static INSTALL_CRYPTO: Once = Once::new();

/// Install the default rustls crypto provider exactly once per process.
/// Safe to call repeatedly.
pub fn ensure_crypto_installed() {
    INSTALL_CRYPTO.call_once(|| {
        // Returns Err if already installed by something else — that's fine.
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// Generate a fresh self-signed certificate + private key for the QUIC server.
pub fn generate_self_signed() -> Result<(CertificateDer<'static>, PrivateKeyDer<'static>), String> {
    let cert = rcgen::generate_simple_self_signed(vec!["anydrop.local".to_string()])
        .map_err(|e| format!("rcgen: {}", e))?;
    let cert_der = cert.cert.der().clone();
    let key_der = PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der());
    Ok((cert_der, PrivateKeyDer::Pkcs8(key_der)))
}

/// Build a quinn ServerConfig with the given cert + key.
pub fn server_config(
    cert: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
) -> Result<quinn::ServerConfig, String> {
    ensure_crypto_installed();
    let mut crypto = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)
        .map_err(|e| format!("rustls server cfg: {}", e))?;
    crypto.alpn_protocols = vec![ALPN.to_vec()];

    let server_crypto = quinn::crypto::rustls::QuicServerConfig::try_from(crypto)
        .map_err(|e| format!("quinn server crypto: {}", e))?;
    let mut cfg = quinn::ServerConfig::with_crypto(Arc::new(server_crypto));

    // Allow long-lived idle connections (file transfers may be slow).
    let mut transport = quinn::TransportConfig::default();
    transport.max_idle_timeout(Some(
        std::time::Duration::from_secs(60).try_into().unwrap(),
    ));
    transport.keep_alive_interval(Some(std::time::Duration::from_secs(15)));
    cfg.transport_config(Arc::new(transport));
    Ok(cfg)
}

/// Build a quinn ClientConfig that accepts any server cert (LAN-only).
pub fn client_config() -> Result<quinn::ClientConfig, String> {
    ensure_crypto_installed();
    let mut crypto = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(SkipVerify))
        .with_no_client_auth();
    crypto.alpn_protocols = vec![ALPN.to_vec()];

    let client_crypto = quinn::crypto::rustls::QuicClientConfig::try_from(crypto)
        .map_err(|e| format!("quinn client crypto: {}", e))?;
    let mut cfg = quinn::ClientConfig::new(Arc::new(client_crypto));

    let mut transport = quinn::TransportConfig::default();
    transport.max_idle_timeout(Some(
        std::time::Duration::from_secs(60).try_into().unwrap(),
    ));
    transport.keep_alive_interval(Some(std::time::Duration::from_secs(15)));
    cfg.transport_config(Arc::new(transport));
    Ok(cfg)
}

/// Trivial cert verifier that accepts everything. LAN-only.
#[derive(Debug)]
struct SkipVerify;

impl rustls::client::danger::ServerCertVerifier for SkipVerify {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
            SignatureScheme::ED448,
        ]
    }
}
