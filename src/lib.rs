extern crate core;


pub mod config;
pub mod store;
pub mod server;
pub mod socks;
pub mod common;
pub mod rathole;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

pub type Result<T> = std::result::Result<T, Error>;

pub mod logs {
    use tracing_subscriber::{EnvFilter, fmt};
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    pub fn init_log() {
        let env_filter = EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into());
        // 输出到控制台中
        let formatting_layer = fmt::layer()
            .with_thread_ids(true)
            .with_writer(std::io::stdout);

        // 输出到文件中
        let file_appender = tracing_appender::rolling::never("logs", "shuttle.log");
        // let (non_blocking_appender, _guard) = tracing_appender::non_blocking(file_appender);
        let file_layer = fmt::layer()
            .with_ansi(false)
            .with_writer(file_appender);


        tracing_subscriber::registry()
            .with(env_filter)
            .with(formatting_layer)
            .with(file_layer)
            .init();
    }
}

pub mod tls {
    use std::sync::Arc;

    use tokio_rustls::rustls;
    use tokio_rustls::rustls::{Certificate, OwnedTrustAnchor, PrivateKey};

    pub fn make_tls_acceptor(certs: Vec<Certificate>, key: PrivateKey) -> tokio_rustls::TlsAcceptor {
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
