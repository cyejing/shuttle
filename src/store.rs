use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::config::{RatHole, ServerConfig, Trojan};
use crate::rathole::connection::CmdSender;

#[derive(Debug, Clone)]
pub struct ServerStore {
    pub cmd_sender_map: Arc<RwLock<HashMap<String, CmdSender>>>,
    pub trojan: Arc<Trojan>,
    pub rathole: Arc<RatHole>,
}

impl From<Rc<ServerConfig>> for ServerStore {
    fn from(sc: Rc<ServerConfig>) -> Self {
        ServerStore {
            cmd_sender_map: Arc::new(RwLock::new(HashMap::new())),
            trojan: Arc::new(sc.trojan.clone()),
            rathole: Arc::new(sc.rathole.clone()),
        }
    }
}
