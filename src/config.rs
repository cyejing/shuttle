use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::io::BufReader;
use crypto::digest::Digest;
use crypto::sha2::Sha224;

use rustls_pemfile::{certs, rsa_private_keys};
use serde::{Deserialize, Serialize};
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
    #[serde(skip)]
    pub cert_vec: Vec<Certificate>,
    pub cert: Option<String>,
    #[serde(skip)]
    pub key_vec: Vec<PrivateKey>,
    pub key: Option<String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct RatHole {
    passwords: Vec<String>,
    #[serde(skip)]
    password_hash: HashMap<String, String>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Trojan {
    passwords: Vec<String>,
    #[serde(skip)]
    password_hash: HashMap<String, String>,
}

impl ServerConfig {
    pub fn load(file: String) -> ServerConfig {
        let mut sc: ServerConfig = serde_yaml::from_reader(File::open(file).unwrap()).unwrap();
        for mut addr in &mut sc.addrs {
            if addr.cert.is_some() && addr.key.is_some() {
                addr.cert_vec = load_certs(addr.cert.as_ref().unwrap()).unwrap();
                addr.key_vec = load_keys(addr.key.as_ref().unwrap()).unwrap();
            }
        }
        sc
    }
}

impl ClientConfig {
    pub fn load(file: String) -> ClientConfig {
        let mut cc: ClientConfig = serde_yaml::from_reader(File::open(file).unwrap()).unwrap();
        cc.hash = sha224(&cc.password);
        cc
    }
}

fn sha224(password: &String) -> String {
    let mut encoder = Sha224::new();
    encoder.reset();
    encoder.input(password.as_bytes());
    let result = encoder.result_str();
    log::info!(
            "sha224({}) = {}, length = {}",
            password,
            result,
            result.len()
        );
    result
}


fn load_certs(path: &String) -> io::Result<Vec<Certificate>> {
    certs(&mut BufReader::new(File::open(path)?))
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid cert"))
        .map(|mut certs| certs.drain(..).map(Certificate).collect())
}

fn load_keys(path: &String) -> io::Result<Vec<PrivateKey>> {
    rsa_private_keys(&mut BufReader::new(File::open(path)?))
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid key"))
        .map(|mut keys| keys.drain(..).map(PrivateKey).collect())
}


#[cfg(test)]
mod tests {
    use crate::config::ServerConfig;

    #[test]
    fn test_load_config() {
        let sc = ServerConfig::load(String::from("examples/shuttles.yaml"));
        println!("{:?}", sc);
    }

    fn test_sha224() {

    }
}
