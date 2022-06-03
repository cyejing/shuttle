use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha224};
use tokio_rustls::rustls;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct ServerConfig {
    pub addrs: Vec<Addr>,
    pub trojan: Trojan,
    pub rathole: RatHole,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientConfig {
    pub run_type: String,
    pub remote_addr: String,
    pub password: String,
    #[serde(skip)]
    pub hash: String,
    #[serde(default)]
    pub proxy_addr: String,
    #[serde(default = "default_true")]
    pub ssl_enable: bool,
    #[serde(default)]
    pub holes: Vec<Hole>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Addr {
    pub addr: String,
    pub cert: Option<String>,
    pub key: Option<String>,
    #[serde(skip)]
    pub ssl_enable: bool,
    #[serde(skip)]
    pub cert_loaded: Vec<rustls::Certificate>,
    #[serde(skip)]
    pub key_loaded: Vec<rustls::PrivateKey>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RatHole {
    pub passwords: Vec<String>,
    #[serde(skip)]
    pub password_hash: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trojan {
    pub local_addr: String,
    pub passwords: Vec<String>,
    #[serde(skip)]
    pub password_hash: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hole {
    pub name: String,
    pub remote_addr: String,
    pub local_addr: String,
}

const DEFAULT_SERVER_CONFIG_PATH: [&str; 2] = ["shuttles.yaml", "examples/shuttles.yaml"];

impl ServerConfig {
    pub fn load(path: Option<PathBuf>) -> ServerConfig {
        let file = open_config_file(path, Vec::from(DEFAULT_SERVER_CONFIG_PATH));

        let mut sc: ServerConfig = serde_yaml::from_reader(file)
            .context("Can't serde read config file")
            .unwrap();
        for mut addr in &mut sc.addrs {
            if addr.cert.is_some() && addr.key.is_some() {
                addr.ssl_enable = true;
                addr.cert_loaded = load_certs(addr.cert.as_ref().unwrap());
                addr.key_loaded = vec![load_private_key(addr.key.as_ref().unwrap())];
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
    "shuttlec.yaml",
    "shuttlec-socks.yaml",
    "shuttlec-rathole.yaml",
    "examples/shuttlec.yaml",
    "examples/shuttlec-socks.yaml",
    "examples/shuttlec-rathole.yaml",
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
        info!("config file : {}", path_str);
        File::open(pb.as_path())
            .context(format!("open config file {:?} err", path_str))
            .unwrap()
    } else {
        let mut of: Option<File> = None;
        for path in &default_paths {
            if let Ok(file) = File::open(*path) {
                info!("config file : {}", *path);
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

fn sha224(password: &str) -> String {
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

pub fn load_certs(filename: &str) -> Vec<rustls::Certificate> {
    let cert_file = File::open(filename)
        .context(format!("Can't open certificate file {}", filename))
        .unwrap();
    let mut reader = BufReader::new(cert_file);
    rustls_pemfile::certs(&mut reader)
        .unwrap()
        .iter()
        .map(|v| rustls::Certificate(v.clone()))
        .collect()
}

pub fn load_private_key(filename: &str) -> rustls::PrivateKey {
    let keyfile = File::open(filename)
        .context(format!("Can't open private key file {}", filename))
        .unwrap();
    let mut reader = BufReader::new(keyfile);

    loop {
        match rustls_pemfile::read_one(&mut reader)
            .context("Can't parse private key .pem file")
            .unwrap()
        {
            Some(rustls_pemfile::Item::RSAKey(key)) => return rustls::PrivateKey(key),
            Some(rustls_pemfile::Item::PKCS8Key(key)) => return rustls::PrivateKey(key),
            None => break,
            _ => {}
        }
    }

    panic!(
        "no keys found in {:?} (encrypted keys not supported)",
        filename
    );
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use crate::config::sha224;

    #[test]
    fn test_hash() {
        let hash = sha224("sQtfRnfhcNoZYZh1wY9u");
        assert_eq!(
            "6b34e62f6df92b8e9db961410b4f1a6fca1e2dae73f9c1b4b94f4a33",
            hash
        );
        println!("{}", hash)
    }
}
