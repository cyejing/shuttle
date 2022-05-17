extern crate core;
#[macro_use]
extern crate log;

pub mod common;
pub mod config;
pub mod rathole;
pub mod server;
pub mod socks;
pub mod store;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

pub type Result<T> = std::result::Result<T, Error>;

pub mod logs {
    use log::LevelFilter;

    pub fn init_log() {
        env_logger::Builder::from_default_env()
            .filter_level(LevelFilter::Info)
            .parse_default_env()
            .init();
    }
}

pub mod tls {
    use std::sync::Arc;

    use tokio_rustls::rustls;
    use tokio_rustls::rustls::{Certificate, OwnedTrustAnchor, PrivateKey};

    pub fn make_tls_acceptor(
        certs: Vec<Certificate>,
        key: PrivateKey,
    ) -> tokio_rustls::TlsAcceptor {
        let config = rustls::ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .expect("bad certificates/private key");

        tokio_rustls::TlsAcceptor::from(Arc::new(config))
    }

    pub fn make_tls_connector() -> tokio_rustls::TlsConnector {
        let mut root_cert_store = rustls::RootCertStore::empty();
        root_cert_store.add_server_trust_anchors(webpki_roots::TLS_SERVER_ROOTS.0.iter().map(
            |ta| {
                OwnedTrustAnchor::from_subject_spki_name_constraints(
                    ta.subject,
                    ta.spki,
                    ta.name_constraints,
                )
            },
        ));
        let config = rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(root_cert_store)
            .with_no_client_auth();
        tokio_rustls::TlsConnector::from(Arc::new(config))
    }

    pub fn make_server_name(domain: &str) -> crate::Result<rustls::ServerName> {
        rustls::ServerName::try_from(domain)
            .map_err(|e| format!("try from domain [{}] to server name err : {}", &domain, e).into())
    }
}
