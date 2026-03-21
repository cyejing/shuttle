use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;
use std::time::Duration;

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
    #[serde(default = "default_logs")]
    pub logs: PathBuf,
    pub server: String,
    pub proxy: Option<ProxyConfig>,
    pub tls: Option<ClientTlsConfig>,
    pub hole: Option<HoleConfig>,
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout: u64,
    #[serde(default = "default_proxy_list")]
    pub proxy_list: PathBuf,
}

fn default_connect_timeout() -> u64 {
    3
}

fn default_proxy_list() -> PathBuf {
    PathBuf::from("proxy.txt")
}

impl ClientConfig {
    #[must_use]
    pub fn insecure(&self) -> bool {
        self.tls.as_ref().is_none_or(|t| t.insecure)
    }
    #[must_use]
    pub fn get_proxy(&self) -> Option<(String, ProxyMode)> {
        self.proxy
            .as_ref()
            .map(|proxy| (proxy.listen.clone(), proxy.mode.clone()))
    }
    pub fn get_holes(&self) -> &[HoleConfigItem] {
        self.hole.as_ref().map(|h| &h.holes).expect("hole is None")
    }
    pub fn get_hole_auth_hash(&self) -> String {
        self.hole
            .as_ref()
            .and_then(|h| h.auth_hash.as_ref())
            .map(std::string::ToString::to_string)
            .unwrap()
    }

    pub fn get_proxy_auth_hash(&self) -> String {
        self.proxy
            .as_ref()
            .and_then(|h| h.auth_hash.as_ref())
            .map(std::string::ToString::to_string)
            .unwrap()
    }

    pub fn connect_timeout_duration(&self) -> Duration {
        Duration::from_secs(self.connect_timeout)
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

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    #[serde(default = "default_logs")]
    pub logs: PathBuf,
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
            self.rathole = Some(rathole);
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
    let file = open_config_file(path, &DEFAULT_CLIENT_CONFIG_PATH);
    let mut cc: ClientConfig = serde_yaml::from_reader(file)
        .context("Can't serde read config file")
        .unwrap();
    cc.gen_auth_hash();
    cc
}

pub fn load_server_config(path: Option<PathBuf>) -> ServerConfig {
    let file = open_config_file(path, &DEFAULT_SERVER_CONFIG_PATH);
    let mut sc: ServerConfig = serde_yaml::from_reader(file)
        .context("Can't serde read config file")
        .unwrap();
    sc.gen_rathole_hahs();
    sc
}

fn open_config_file(path: Option<PathBuf>, default_paths: &[&str]) -> File {
    if let Some(pb) = path {
        let path_str = pb.to_str().unwrap();
        info!("load config file : {}", path_str);
        File::open(pb.as_path()).unwrap()
    } else {
        let mut of: Option<File> = None;
        for path in default_paths {
            if let Ok(file) = File::open(*path) {
                info!("load config file: {}", *path);
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

fn default_logs() -> PathBuf {
    PathBuf::from("logs")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn load_client_config_populates_defaults_and_hashes() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("client.yaml");
        fs::write(
            &path,
            r#"
server: 127.0.0.1:4982
proxy:
  listen: 127.0.0.1:4082
  auth: proxy-secret
  mode: direct
hole:
  auth: hole-secret
  holes:
    - name: test
      remote_addr: 127.0.0.1:6788
      local_addr: 127.0.0.1:6789
"#,
        )
        .unwrap();

        let config = load_client_config(Some(path));

        assert_eq!(config.logs, PathBuf::from("logs"));
        assert!(config.insecure());
        assert_eq!(
            config.get_proxy(),
            Some(("127.0.0.1:4082".to_string(), ProxyMode::Direct))
        );
        assert_eq!(config.get_proxy_auth_hash(), sha224("proxy-secret"));
        assert_eq!(config.get_hole_auth_hash(), sha224("hole-secret"));
        assert_eq!(config.get_holes().len(), 1);
    }

    #[test]
    fn load_server_config_generates_rathole_hashes() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("server.yaml");
        fs::write(
            &path,
            r#"
listen: 127.0.0.1:4982
auth:
  type: password
  password: server-secret
rathole:
  passwords:
    - hole-secret
    - hole-secret-2
"#,
        )
        .unwrap();

        let config = load_server_config(Some(path));
        let rathole = config.rathole.expect("rathole config should exist");

        assert_eq!(config.logs, PathBuf::from("logs"));
        assert_eq!(rathole.password_hash.len(), 2);
        assert_eq!(rathole.password_hash[0], sha224("hole-secret"));
        assert_eq!(rathole.password_hash[1], sha224("hole-secret-2"));
    }

    #[test]
    fn client_insecure_defaults_to_tls_absent_or_enabled() {
        let config = ClientConfig {
            server: "127.0.0.1:4982".to_string(),
            ..Default::default()
        };
        assert!(config.insecure());

        let config = ClientConfig {
            server: "127.0.0.1:4982".to_string(),
            tls: Some(ClientTlsConfig {
                insecure: false,
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(!config.insecure());
    }
}
