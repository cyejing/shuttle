use std::collections::HashMap;

use itertools::Itertools;
use tokio::sync::RwLock;

use crate::config::{RatHole, ServerConfig};
use crate::rathole::context::CommandSender;

pub struct ServerStore {
    cmd_map: RwLock<HashMap<String, CommandSender>>,
    rathole: Option<RatHole>,
}

impl ServerStore {
    pub fn new(rathole: Option<RatHole>) -> Self {
        ServerStore {
            cmd_map: RwLock::new(HashMap::new()),
            rathole,
        }
    }

    pub(crate) fn has_rathole(&self, hash: &str) -> bool {
        match &self.rathole {
            Some(h) => h.password_hash.contains_key(hash),
            None => false,
        }
    }

    pub(crate) async fn set_cmd_sender(&self, sender: CommandSender) {
        self.cmd_map.write().await.insert(sender.get_hash(), sender);
    }

    #[allow(dead_code)]
    pub(crate) async fn get_cmd_sender(&self, hash: &String) -> Option<CommandSender> {
        self.cmd_map.read().await.get(hash).cloned()
    }

    #[allow(dead_code)]
    pub(crate) async fn list_cmd_sender(&self) -> Vec<CommandSender> {
        self.cmd_map.read().await.values().cloned().collect_vec()
    }

    pub(crate) async fn remove_cmd_sender(&self, hash: &String) {
        let _r = self.cmd_map.write().await.remove(hash);
    }
}

impl From<&ServerConfig> for ServerStore {
    fn from(sc: &ServerConfig) -> Self {
        ServerStore {
            cmd_map: RwLock::new(HashMap::new()),
            rathole: sc.rathole.clone(),
        }
    }
}
