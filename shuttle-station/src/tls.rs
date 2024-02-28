use anyhow::{anyhow, Context};
use log::debug;
use std::{fs::File, io::BufReader, path::PathBuf, sync::Arc};
use tokio_rustls::rustls::{
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, PrivateKeyDer, ServerName},
    ClientConfig, RootCertStore, ServerConfig, SignatureScheme,
};

pub fn make_tls_acceptor(
    certs: Option<Vec<CertificateDer<'static>>>,
    key: Option<&PrivateKeyDer<'static>>,
) -> Option<tokio_rustls::TlsAcceptor> {
    if let (Some(key), Some(certs)) = (key, certs) {
        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key.clone_key())
            .context("Bad certificates/private key")
            .unwrap();

        Some(tokio_rustls::TlsAcceptor::from(Arc::new(config)))
    } else {
        None
    }
}

pub fn make_tls_connector(invalid_certs: bool) -> tokio_rustls::TlsConnector {
    let mut root_cert_store = RootCertStore::empty();
    root_cert_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let mut config = ClientConfig::builder()
        .with_root_certificates(root_cert_store)
        .with_no_client_auth();

    if invalid_certs {
        config
            .dangerous()
            .set_certificate_verifier(Arc::new(NoVerifier));
    }

    tokio_rustls::TlsConnector::from(Arc::new(config))
}

pub fn make_server_name<'a>(domain: &str) -> anyhow::Result<ServerName<'a>> {
    let domain = domain
        .split(':')
        .next()
        .ok_or_else(|| anyhow!("domain parse error : {}", domain))?;
    debug!("Server name parse is : {}", domain);
    let server_name = ServerName::try_from(domain)
        .map_err(|e| anyhow!("try from domain [{}] to server name err : {}", &domain, e))?
        .to_owned();
    Ok(server_name)
}

pub fn load_certs(path: PathBuf) -> std::io::Result<Vec<CertificateDer<'static>>> {
    let cert_file = File::open(path.as_path()).expect("Can't open certificate file");
    let mut reader = BufReader::new(cert_file);
    rustls_pemfile::certs(&mut reader).collect()
}

pub fn load_private_key(path: PathBuf) -> std::io::Result<PrivateKeyDer<'static>> {
    let keyfile = File::open(path.as_path()).expect("Can't open private key file");
    let mut reader = BufReader::new(keyfile);
    let x = rustls_pemfile::rsa_private_keys(&mut reader)
        .next()
        .unwrap()
        .map(Into::into);
    x
}

#[derive(Debug)]
pub(crate) struct NoVerifier;

impl ServerCertVerifier for NoVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: tokio_rustls::rustls::pki_types::UnixTime,
    ) -> Result<tokio_rustls::rustls::client::danger::ServerCertVerified, tokio_rustls::rustls::Error>
    {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &tokio_rustls::rustls::DigitallySignedStruct,
    ) -> Result<
        tokio_rustls::rustls::client::danger::HandshakeSignatureValid,
        tokio_rustls::rustls::Error,
    > {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &tokio_rustls::rustls::DigitallySignedStruct,
    ) -> Result<
        tokio_rustls::rustls::client::danger::HandshakeSignatureValid,
        tokio_rustls::rustls::Error,
    > {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<tokio_rustls::rustls::SignatureScheme> {
        vec![
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ED25519,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA256,
        ]
    }
}
