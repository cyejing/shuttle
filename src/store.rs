use std::borrow::{Borrow, BorrowMut};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;
use crate::config::{RatHole, ServerConfig, Trojan};
use crate::rathole::connection::CmdSender;

#[derive(Debug, Clone)]
pub struct ServerStore {
    pub cmd_sender_map: Arc<RwLock<HashMap<String, Arc<CmdSender>>>>,
    pub trojan: Arc<Trojan>,
    pub rathole: Arc<RatHole>,
}

impl ServerStore {
    pub(crate) fn set_sender(&self, sender: CmdSender) {
        self.cmd_sender_map.blocking_write().insert(sender.hash.clone(), Arc::new(sender));
    }

    pub(crate) fn get_sender(&self,hash: &String) -> Option<Arc<CmdSender>>{
        self.cmd_sender_map.blocking_read().get(hash).map(|e|e.clone())
    }
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