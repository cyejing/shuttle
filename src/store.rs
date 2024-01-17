use std::collections::HashMap;
use std::sync::Arc;

use itertools::Itertools;
use tokio::sync::RwLock;

use crate::config::{RatHole, ServerConfig, Trojan};
use crate::rathole::context::CommandSender;

#[derive(Debug, Clone)]
pub struct ServerStore {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    cmd_map: RwLock<HashMap<String, CommandSender>>,
    trojan: Trojan,
    rathole: RatHole,
}

impl ServerStore {
    pub fn new(trojan: Trojan, rathole: RatHole) -> Self {
        ServerStore {
            inner: Arc::new(Inner {
                cmd_map: RwLock::new(HashMap::new()),
                trojan,
                rathole,
            }),
        }
    }

    pub(crate) fn get_trojan(&self) -> Trojan {
        self.inner.trojan.clone()
    }

    pub(crate) fn get_rahole(&self) -> RatHole {
        self.inner.rathole.clone()
    }

    pub(crate) async fn set_cmd_sender(&self, sender: CommandSender) {
        self.inner
            .cmd_map
            .write()
            .await
            .insert(sender.get_hash(), sender);
    }

    #[allow(dead_code)]
    pub(crate) async fn get_cmd_sender(&self, hash: &String) -> Option<CommandSender> {
        self.inner.cmd_map.read().await.get(hash).cloned()
    }

    #[allow(dead_code)]
    pub(crate) async fn list_cmd_sender(&self) -> Vec<CommandSender> {
        self.inner
            .cmd_map
            .read()
            .await
            .values()
            .cloned()
            .collect_vec()
    }

    pub(crate) async fn remove_cmd_sender(&self, hash: &String) {
        let r = self.inner.cmd_map.write().await.remove(hash);
        drop(r);
    }
}

impl From<&ServerConfig> for ServerStore {
    fn from(sc: &ServerConfig) -> Self {
        ServerStore {
            inner: Arc::new(Inner {
                cmd_map: RwLock::new(HashMap::new()),
                trojan: sc.trojan.clone(),
                rathole: sc.rathole.clone(),
            }),
        }
    }
}
