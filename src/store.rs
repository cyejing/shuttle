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
    pub(crate) fn has_rathole(&self, hash: &str) -> bool {
        self.rathole
            .as_ref()
            .is_some_and(|rathole| rathole.password_hash.iter().any(|item| item == hash))
    }

    pub(crate) async fn set_cmd_sender(&self, sender: CommandSender) {
        self.cmd_map.write().await.insert(sender.get_hash(), sender);
    }

    #[allow(dead_code)]
    pub(crate) async fn get_cmd_sender(&self, hash: &str) -> Option<CommandSender> {
        self.cmd_map.read().await.get(hash).cloned()
    }

    #[allow(dead_code)]
    pub(crate) async fn list_cmd_sender(&self) -> Vec<CommandSender> {
        self.cmd_map.read().await.values().cloned().collect_vec()
    }

    pub(crate) async fn remove_cmd_sender(&self, hash: &str) {
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

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc;

    use super::*;
    use crate::config::{AuthConfig, AuthType};
    use crate::rathole::context::CommandSender;

    fn server_config() -> ServerConfig {
        ServerConfig {
            logs: "logs".into(),
            listen: "127.0.0.1:4982".to_string(),
            tls: None,
            auth: AuthConfig {
                auth_type: AuthType::Password,
                password: Some("secret".to_string()),
                userpass: None,
                http: None,
                command: None,
            },
            websocket: None,
            traffic_stats: None,
            masquerade: None,
            rathole: Some(RatHole {
                passwords: vec!["rathole-secret".to_string()],
                password_hash: vec![crate::auth::sha224("rathole-secret")],
            }),
        }
    }

    #[tokio::test]
    async fn store_tracks_command_senders() {
        let config = server_config();
        let store = ServerStore::from(&config);
        let (tx, _rx) = mpsc::channel(1);
        let sender = CommandSender::new("hash-1".to_string(), tx);

        store.set_cmd_sender(sender.clone()).await;

        assert!(store.has_rathole(&crate::auth::sha224("rathole-secret")));
        assert_eq!(
            store
                .get_cmd_sender("hash-1")
                .await
                .expect("sender should be present")
                .get_hash(),
            "hash-1"
        );
        assert_eq!(store.list_cmd_sender().await.len(), 1);

        store.remove_cmd_sender("hash-1").await;

        assert!(store.get_cmd_sender("hash-1").await.is_none());
        assert!(!store.has_rathole("missing"));
    }
}
