use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;

use anyhow::Context;
use borer_core::tls::{load_certs, load_private_key};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha224};
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};

#[derive(Debug, Serialize, Deserialize)]
pub struct ServerConfig {
    pub addrs: Vec<Addr>,
    pub stats_addr: Option<String>,
    pub stats_secret: Option<String>,
    pub trojan: Trojan,
    pub rathole: RatHole,
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientConfig {
    ///  proxy, rathole
    pub run_type: String,
    #[serde(default = "default_proxy_mode")]
    pub proxy_mode: ProxyMode,
    pub remote_addr: String,
    pub password: String,
    #[serde(skip)]
    pub hash: String,
    #[serde(default)]
    pub proxy_addr: String,
    #[serde(default = "default_true")]
    pub ssl_enable: bool,
    #[serde(default = "default_false")]
    pub invalid_certs: bool,
    #[serde(default = "default_true")]
    pub padding: bool,
    #[serde(default)]
    pub holes: Vec<Hole>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Addr {
    pub addr: String,
    pub cert: Option<String>,
    pub key: Option<String>,
    #[serde(skip)]
    pub ssl_enable: bool,
    #[serde(skip)]
    pub cert_loaded: Option<Vec<CertificateDer<'static>>>,
    #[serde(skip)]
    pub key_loaded: Option<PrivateKeyDer<'static>>,
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RatHole {
    pub passwords: Vec<String>,
    #[serde(skip)]
    pub password_hash: HashMap<String, String>,
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trojan {
    pub local_addr: Option<String>,
    pub passwords: Vec<String>,
    #[serde(skip)]
    pub password_hash: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hole {
    pub name: String,
    pub remote_addr: String,
    pub local_addr: String,
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProxyMode {
    #[serde(alias = "direct")]
    Direct,
    #[serde(alias = "trojan")]
    #[default]
    Trojan,
    #[serde(alias = "websocket")]
    Websocket,
}

const DEFAULT_SERVER_CONFIG_PATH: [&str; 2] = ["server.yaml", "examples/server.yaml"];

impl ServerConfig {
    pub fn load(path: Option<PathBuf>) -> ServerConfig {
        let file = open_config_file(path, Vec::from(DEFAULT_SERVER_CONFIG_PATH));

        let mut sc: ServerConfig = serde_yaml::from_reader(file)
            .context("Can't serde read config file")
            .unwrap();
        for addr in &mut sc.addrs {
            if let (Some(key), Some(cert)) = (&addr.key, &addr.cert) {
                addr.ssl_enable = true;

                addr.cert_loaded =
                    Some(load_certs(PathBuf::from(cert)).expect("load_certs failed"));
                addr.key_loaded =
                    Some(load_private_key(PathBuf::from(key)).expect("load_private_key falied"));
            }
        }
        for password in &sc.trojan.passwords {
            sc.trojan
                .password_hash
                .insert(sha224(password), password.clone());
        }
        for password in &sc.rathole.passwords {
            sc.rathole
                .password_hash
                .insert(sha224(password), password.clone());
        }
        sc
    }
}

const DEFAULT_CLIENT_CONFIG_PATH: [&str; 6] = [
    "client.yaml",
    "client-proxy.yaml",
    "client-rathole.yaml",
    "examples/client.yaml",
    "examples/client-proxy.yaml",
    "examples/client-rathole.yaml",
];

impl ClientConfig {
    pub fn load(path: Option<PathBuf>) -> ClientConfig {
        let file = open_config_file(path, Vec::from(DEFAULT_CLIENT_CONFIG_PATH));

        let mut cc: ClientConfig = serde_yaml::from_reader(file)
            .context("Can't serde read config file")
            .unwrap();
        cc.hash = sha224(&cc.password);
        cc
    }
}

fn open_config_file(path: Option<PathBuf>, default_paths: Vec<&str>) -> File {
    if let Some(pb) = path {
        let path_str = pb.to_str().unwrap();
        info!("Load config file : {}", path_str);
        File::open(pb.as_path())
            .context(format!("Can't load config file {:?}", path_str))
            .unwrap()
    } else {
        let mut of: Option<File> = None;
        for path in &default_paths {
            if let Ok(file) = File::open(*path) {
                info!("Load config file : {}", *path);
                of = Some(file);
                break;
            }
        }

        of.context(format!(
            "Can't find default config file [{:?}]",
            &default_paths
        ))
        .unwrap()
    }
}

impl Trojan {
    pub fn push(&mut self, pwd: &str) {
        self.passwords.push(pwd.to_string());
        self.password_hash.insert(sha224(pwd), pwd.to_string());
    }
}
impl RatHole {
    pub fn push(&mut self, pwd: &str) {
        self.passwords.push(pwd.to_string());
        self.password_hash.insert(sha224(pwd), pwd.to_string());
    }
}

pub fn sha224(password: &str) -> String {
    let mut hasher = Sha224::new();
    hasher.update(password.as_bytes());
    let hash = hasher.finalize();
    let result = base16ct::lower::encode_string(&hash);
    log::debug!(
        "sha224({}) = {}, length = {}",
        password,
        result,
        result.len()
    );
    result
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

fn default_proxy_mode() -> ProxyMode {
    ProxyMode::Trojan
}

#[cfg(test)]
mod tests {
    use crate::config::sha224;

    #[test]
    fn test_hash() {
        assert_eq!(
            sha224("sQtfRnfhcNoZYZh1wY9u"),
            "6b34e62f6df92b8e9db961410b4f1a6fca1e2dae73f9c1b4b94f4a33",
        );
        assert_eq!(
            sha224("cyj22334400!"),
            "3af1c305cd8ec7eebaf03bab42e42dd686e2ef5db27a7c7176350eb0"
        );
    }
}
