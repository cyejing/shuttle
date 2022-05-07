use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::rc::Rc;
use std::sync::Arc;

use crypto::digest::Digest;
use crypto::sha2::Sha224;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio_rustls::rustls;
use tokio_rustls::rustls::{Certificate, PrivateKey};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct ServerConfig {
    pub addrs: Vec<Addr>,
    #[serde(default)]
    pub logfile: String,
    pub trojan: Trojan,
    pub rathole: RatHole,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct ClientConfig {
    pub run_type: String,
    pub name: String,
    pub remote_addr: String,
    pub password: String,
    #[serde(skip)]
    pub hash: String,
    #[serde(default)]
    pub sock_addr: String,
    #[serde(default)]
    pub ssl_enable: bool,
    #[serde(default)]
    pub logfile: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Addr {
    pub addr: String,
    pub cert: Option<String>,
    pub key: Option<String>,
    #[serde(skip)]
    pub ssl_enable: bool,
    #[serde(skip)]
    pub cert_loaded: Vec<Certificate>,
    #[serde(skip)]
    pub key_loaded: Vec<PrivateKey>,
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

#[derive(Clone, Debug)]
pub struct ServerStore {
    pub req_map: Arc<RwLock<HashMap<String, String>>>,
    pub trojan: Arc<Trojan>,
    pub rathole: Arc<RatHole>,
}

impl From<Rc<ServerConfig>> for ServerStore {
    fn from(sc: Rc<ServerConfig>) -> Self {
        ServerStore {
            req_map: Arc::new(RwLock::new(HashMap::new())),
            trojan: Arc::new(sc.trojan.clone()),
            rathole: Arc::new(sc.rathole.clone()),
        }
    }
}

const DEFAULT_SERVER_CONFIG_PATH: [&str; 2] = ["shuttles.yaml", "examples/shuttles.yaml"];

impl ServerConfig {
    pub fn load(file_op: Option<String>) -> ServerConfig {
        let file = open_config_file(file_op, Vec::from(DEFAULT_SERVER_CONFIG_PATH));

        let mut sc: ServerConfig = serde_yaml::from_reader(file)
            .unwrap_or_else(|e| panic!("serde read file failed {}", e));
        for mut addr in &mut sc.addrs {
            if addr.cert.is_some() && addr.key.is_some() {
                addr.ssl_enable = true;
                addr.cert_loaded = load_certs(addr.cert.as_ref().unwrap());
                addr.key_loaded = vec![load_private_key(addr.key.as_ref().unwrap())];
            }
        }
        for password in &sc.trojan.passwords {
            sc.trojan.password_hash.insert(sha224(password), password.clone());
        }
        for password in &sc.rathole.passwords {
            sc.rathole.password_hash.insert(sha224(password), password.clone());
        }
        sc
    }
}

const DEFAULT_CLIENT_CONFIG_PATH: [&str; 2] = ["shuttlec.yaml", "examples/shuttlec.yaml"];

impl ClientConfig {
    pub fn load(file_op: Option<String>) -> ClientConfig {
        let file = open_config_file(file_op, Vec::from(DEFAULT_CLIENT_CONFIG_PATH));

        let mut cc: ClientConfig = serde_yaml::from_reader(file)
            .unwrap_or_else(|e| panic!("serde read file failed {}", e));
        cc.hash = sha224(&cc.password);
        cc
    }
}

fn open_config_file(file_op: Option<String>, default_paths: Vec<&str>) -> File {
    if file_op.is_some() {
        let file_path = file_op.unwrap();
        File::open(&file_path)
            .unwrap_or_else(|e| panic!("open file [{}] failed {}", &file_path, e))
    } else {
        let mut of: Option<File> = Option::None;
        for path in default_paths {
            if let Result::Ok(file) = File::open(path) {
                of = Option::Some(file);
                break;
            }
        };
        of.expect("Can't find the default config file ./shuttles.yaml or ./shuttlec.yaml")
    }
}

fn sha224(password: &String) -> String {
    let mut encoder = Sha224::new();
    encoder.reset();
    encoder.input(password.as_bytes());
    let result = encoder.result_str();
    log::debug!(
            "sha224({}) = {}, length = {}",
            password,
            result,
            result.len()
        );
    result
}


pub fn load_certs(filename: &str) -> Vec<rustls::Certificate> {
    let certfile = File::open(filename).expect("cannot open certificate file");
    let mut reader = BufReader::new(certfile);
    rustls_pemfile::certs(&mut reader)
        .unwrap()
        .iter()
        .map(|v| rustls::Certificate(v.clone()))
        .collect()
}

pub fn load_private_key(filename: &str) -> rustls::PrivateKey {
    let keyfile = File::open(filename).expect("cannot open private key file");
    let mut reader = BufReader::new(keyfile);

    loop {
        match rustls_pemfile::read_one(&mut reader).expect("cannot parse private key .pem file") {
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


#[cfg(test)]
mod tests {}
