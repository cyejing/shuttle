use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::config::{RatHole, ServerConfig, Trojan};
use crate::rathole::dispatcher::{CommandSender, ConnSender};

#[derive(Debug, Clone)]
pub struct ServerStore {
    pub cmd_map: Arc<RwLock<HashMap<String, Arc<CommandSender>>>>,
    pub conn_map: Arc<RwLock<HashMap<String, Arc<ConnSender>>>>,
    pub trojan: Arc<Trojan>,
    pub rathole: Arc<RatHole>,
}

impl ServerStore {
    pub(crate) fn set_cmd_sender(&self, sender: Arc<CommandSender>) {
        self.cmd_map
            .blocking_write()
            .insert(sender.hash.clone(), sender);
    }

    #[allow(dead_code)]
    pub(crate) fn get_cmd_sender(&self, hash: &String) -> Option<Arc<CommandSender>> {
        self.cmd_map.blocking_read().get(hash).cloned()
    }

    #[allow(dead_code)]
    pub(crate) fn set_conn_sender(&self, sender: Arc<ConnSender>) {
        self.conn_map
            .blocking_write()
            .insert(sender.conn_id.clone(), sender);
    }

    pub(crate) fn get_conn_sender(&self, conn_id: &String) -> Option<Arc<ConnSender>> {
        self.conn_map.blocking_read().get(conn_id).cloned()
    }
}

impl Default for ServerStore {
    fn default() -> Self {
        ServerStore {
            cmd_map: Arc::new(RwLock::new(HashMap::new())),
            conn_map: Arc::new(RwLock::new(HashMap::new())),
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

impl From<Rc<ServerConfig>> for ServerStore {
    fn from(sc: Rc<ServerConfig>) -> Self {
        ServerStore {
            cmd_map: Arc::new(RwLock::new(HashMap::new())),
            conn_map: Arc::new(RwLock::new(HashMap::new())),
            trojan: Arc::new(sc.trojan.clone()),
            rathole: Arc::new(sc.rathole.clone()),
        }
    }
}
