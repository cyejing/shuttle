use anyhow::{anyhow, Context};
use log::debug;
use rustls::client::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::ServerName;
use rustls::{DigitallySignedStruct, Error as TLSError};
use std::sync::Arc;

use tokio_rustls::rustls;
use tokio_rustls::rustls::{Certificate, OwnedTrustAnchor, PrivateKey};

pub fn make_tls_acceptor(certs: Vec<Certificate>, key: PrivateKey) -> tokio_rustls::TlsAcceptor {
    let config = rustls::ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .context("Bad certificates/private key")
        .unwrap();

    tokio_rustls::TlsAcceptor::from(Arc::new(config))
}

pub fn make_tls_connector(invalid_certs: bool) -> tokio_rustls::TlsConnector {
    let mut root_cert_store = rustls::RootCertStore::empty();
    root_cert_store.add_trust_anchors(webpki_roots::TLS_SERVER_ROOTS.iter().map(|ta| {
        OwnedTrustAnchor::from_subject_spki_name_constraints(
            ta.subject,
            ta.spki,
            ta.name_constraints,
        )
    }));
    let mut config = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(root_cert_store)
        .with_no_client_auth();

    if invalid_certs {
        config
            .dangerous()
            .set_certificate_verifier(Arc::new(NoVerifier));
    }

    tokio_rustls::TlsConnector::from(Arc::new(config))
}

pub fn make_server_name(domain: &str) -> anyhow::Result<rustls::ServerName> {
    let domain = domain
        .split(':')
        .next()
        .ok_or_else(|| anyhow!("domain parse error : {}", domain))?;
    debug!("Server name parse is : {}", domain);
    rustls::ServerName::try_from(domain)
        .map_err(|e| anyhow!("try from domain [{}] to server name err : {}", &domain, e))
}

pub(crate) struct NoVerifier;

impl ServerCertVerifier for NoVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::Certificate,
        _intermediates: &[rustls::Certificate],
        _server_name: &ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        _now: std::time::SystemTime,
    ) -> Result<ServerCertVerified, TLSError> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::Certificate,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TLSError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::Certificate,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TLSError> {
        Ok(HandshakeSignatureValid::assertion())
    }
}
