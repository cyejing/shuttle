use std::collections::HashMap;
use std::sync::Arc;

use itertools::Itertools;
use tokio::sync::RwLock;

use crate::config::{RatHole, ServerConfig, Trojan};
use crate::rathole::context::CommandSender;

#[derive(Debug, Clone)]
pub struct ServerStore {
    pub cmd_map: Arc<RwLock<HashMap<String, Arc<CommandSender>>>>,
    pub trojan: Arc<Trojan>,
    pub rathole: Arc<RatHole>,
}

impl ServerStore {
    pub fn new(trojan: Trojan, rathole: RatHole) -> Self {
        ServerStore {
            cmd_map: Arc::new(RwLock::new(HashMap::new())),
            trojan: Arc::new(trojan),
            rathole: Arc::new(rathole),
        }
    }

    pub(crate) async fn set_cmd_sender(&self, sender: Arc<CommandSender>) {
        self.cmd_map
            .write()
            .await
            .insert(sender.hash.clone(), sender);
    }

    #[allow(dead_code)]
    pub(crate) async fn get_cmd_sender(&self, hash: &String) -> Option<Arc<CommandSender>> {
        self.cmd_map.read().await.get(hash).cloned()
    }

    pub(crate) async fn list_cmd_sender(&self) -> Vec<Arc<CommandSender>> {
        self.cmd_map.read().await.values().cloned().collect_vec()
    }

    pub(crate) async fn remove_cmd_sender(&self, hash: &String) {
        let r = self.cmd_map.write().await.remove(hash);
        drop(r);
    }
}

impl Default for ServerStore {
    fn default() -> Self {
        ServerStore {
            cmd_map: Arc::new(RwLock::new(HashMap::new())),
            trojan: Arc::new(Trojan {
                local_addr: "".to_string(),
                passwords: Vec::new(),
                password_hash: HashMap::new(),
            }),
            rathole: Arc::new(RatHole {
                passwords: Vec::new(),
                password_hash: HashMap::new(),
            }),
        }
    }
}

impl From<&ServerConfig> for ServerStore {
    fn from(sc: &ServerConfig) -> Self {
        ServerStore {
            cmd_map: Arc::new(RwLock::new(HashMap::new())),
            trojan: Arc::new(sc.trojan.clone()),
            rathole: Arc::new(sc.rathole.clone()),
        }
    }
}
