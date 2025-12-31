use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

use crate::auth::sha224;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct ArgsConfig {
    // running mode
    #[command(subcommand)]
    pub mode: Mode,

    /// config path
    #[arg(short = 'c', long, global = true)]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum Mode {
    Server,
    Client,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    pub server: String,
    pub proxy: Option<ProxyConfig>,
    pub tls: Option<ClientTlsConfig>,
    pub hole: Option<HoleConfig>,
}

impl ClientConfig {
    pub fn insecure(&self) -> bool {
        self.tls.as_ref().map(|t| t.insecure).unwrap_or(true)
    }
    pub fn get_proxy(&self) -> Option<(String, ProxyMode)> {
        match self.proxy.clone() {
            Some(p) => Some((p.listen, p.mode)),
            None => None,
        }
    }
    pub fn get_holes(&self) -> &Vec<HoleConfigItem> {
        self.hole.as_ref().map(|h| &h.holes).expect("hole is None")
    }
    pub fn get_hole_auth_hash(&self) -> String {
        self.hole
            .as_ref()
            .and_then(|h| h.auth_hash.as_ref())
            .map(|h| h.to_string())
            .unwrap()
    }

    pub fn get_proxy_auth_hash(&self) -> String {
        self.proxy
            .as_ref()
            .and_then(|h| h.auth_hash.as_ref())
            .map(|h| h.to_string())
            .unwrap()
    }

    pub fn gen_auth_hash(&mut self) {
        if let Some(mut proxy) = self.proxy.take() {
            proxy.auth_hash = Some(sha224(&proxy.auth));
            self.proxy = Some(proxy);
        }
        if let Some(mut hole) = self.hole.take() {
            hole.auth_hash = Some(sha224(&hole.auth));
            self.hole = Some(hole);
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub listen: String,
    pub auth: String,
    #[serde(skip)]
    pub auth_hash: Option<String>,
    pub mode: ProxyMode,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub enum ProxyMode {
    #[serde(alias = "direct")]
    Direct,
    #[default]
    #[serde(alias = "trojan")]
    Trojan,
    #[serde(alias = "websocket")]
    Websocket,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct ClientTlsConfig {
    pub sni: Option<String>,
    #[serde(default = "default_true")]
    pub insecure: bool,
    pub pin_sha256: Option<String>,
    pub ca: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ServerConfig {
    pub listen: String,
    pub tls: Option<ServerTlsConfig>,
    pub auth: AuthConfig,
    pub websocket: Option<WebSocketConfig>,
    pub traffic_stats: Option<TrafficStstsConfig>,
    pub masquerade: Option<MasqueradeConfig>,

    pub rathole: Option<RatHole>,
}

impl ServerConfig {
    pub fn gen_rathole_hahs(&mut self) {
        if let Some(mut rathole) = self.rathole.take() {
            let password_hash = rathole.passwords.iter().map(|p| sha224(p)).collect();
            rathole.password_hash = password_hash;
            self.rathole = Some(rathole)
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct ServerTlsConfig {
    pub cert: String,
    pub key: String,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    #[serde(alias = "type")]
    pub auth_type: AuthType,
    pub password: Option<String>,
    pub userpass: Option<HashMap<String, String>>,
    pub http: Option<AuthHttpConfig>,
    pub command: Option<String>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketConfig {
    pub listen: String,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub enum AuthType {
    #[serde(alias = "password")]
    #[default]
    Password,
    #[serde(alias = "userpass")]
    Userpass,
    #[serde(alias = "http")]
    Http,
    #[serde(alias = "command")]
    Command,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct AuthHttpConfig {
    pub url: String,
    #[serde(default = "default_true")]
    pub insecure: bool,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct TrafficStstsConfig {
    pub listen: String,
    pub secret: String,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct MasqueradeConfig {
    #[serde(alias = "type")]
    pub masquerade_type: String,
    pub file: Option<MasqueradeFileConfig>,
    pub proxy: Option<MasqueradeProxyConfig>,
    pub body: Option<String>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct MasqueradeFileConfig {
    pub dir: String,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct MasqueradeProxyConfig {
    pub url: String,
    pub rewrite_host: Option<String>,
    #[serde(default = "default_true")]
    pub insecure: bool,
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RatHole {
    pub passwords: Vec<String>,
    #[serde(skip)]
    pub password_hash: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoleConfig {
    pub auth: String,
    #[serde(skip)]
    pub auth_hash: Option<String>,
    pub holes: Vec<HoleConfigItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoleConfigItem {
    pub name: String,
    pub remote_addr: String,
    pub local_addr: String,
}

const DEFAULT_CLIENT_CONFIG_PATH: [&str; 2] = ["client.yaml", "examples/client.yaml"];
const DEFAULT_SERVER_CONFIG_PATH: [&str; 2] = ["server.yaml", "examples/server.yaml"];

pub fn load_client_config(path: Option<PathBuf>) -> ClientConfig {
    let file = open_config_file(path, Vec::from(DEFAULT_CLIENT_CONFIG_PATH));
    let mut cc: ClientConfig = serde_yaml::from_reader(file)
        .context("Can't serde read config file")
        .unwrap();
    cc.gen_auth_hash();
    cc
}

pub fn load_server_config(path: Option<PathBuf>) -> ServerConfig {
    let file = open_config_file(path, Vec::from(DEFAULT_SERVER_CONFIG_PATH));
    let mut sc: ServerConfig = serde_yaml::from_reader(file)
        .context("Can't serde read config file")
        .unwrap();
    sc.gen_rathole_hahs();
    sc
}

fn open_config_file(path: Option<PathBuf>, default_paths: Vec<&str>) -> File {
    if let Some(pb) = path {
        let path_str = pb.to_str().unwrap();
        info!("load config file : {}", path_str);
        File::open(pb.as_path())
            .context(format!("load config file {path_str:?} failed"))
            .unwrap()
    } else {
        let mut of: Option<File> = None;
        for path in &default_paths {
            if let Ok(file) = File::open(*path) {
                info!("load config file : {}", *path);
                of = Some(file);
                break;
            }
        }

        of.context(format!(
            "load default config file [{default_paths:?}] failed",
        ))
        .unwrap()
    }
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {}
