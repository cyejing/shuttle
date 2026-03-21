use crate::rathole::cmd::Command;
use crate::rathole::{CommandChannel, ReqChannel};
use anyhow::{Context as _, anyhow};
use bytes::Bytes;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast, mpsc, oneshot};
use tracing::{error, trace};

#[derive(Debug, Clone)]
pub struct Context {
    pub notify_shutdown: broadcast::Sender<()>,
    pub command_sender: CommandSender,
    pub conn_map: Arc<Mutex<HashMap<u64, ConnSender>>>,
    pub req_map: Arc<Mutex<HashMap<u64, ReqChannel>>>,
    pub current_req_id: Option<u64>,
    pub current_conn_id: Option<u64>,
}

impl Context {
    pub fn new(command_sender: CommandSender) -> Self {
        let (notify_shutdown, _) = broadcast::channel(1);

        Context {
            notify_shutdown,
            command_sender,
            current_req_id: None,
            current_conn_id: None,
            conn_map: Arc::new(Mutex::new(HashMap::new())),
            req_map: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(crate) fn with_req_id(&mut self, req_id: u64) -> &Self {
        self.current_req_id = Some(req_id);
        self
    }

    pub(crate) fn with_conn_id(&mut self, conn_id: u64) {
        self.current_conn_id = Some(conn_id);
    }

    pub(crate) async fn set_req(&self, req_channel: ReqChannel) {
        trace!(
            "Context set req {:?} {:?}",
            self.current_req_id, req_channel
        );
        if let Some(req_id) = self.current_req_id {
            if req_channel.is_some() {
                self.req_map.lock().await.insert(req_id, req_channel);
            }
        }
    }

    pub(crate) async fn get_req(&self) -> Option<ReqChannel> {
        trace!("Context get req {:?}", self.current_req_id);
        match &self.current_req_id {
            Some(req_id) => self.req_map.lock().await.remove(req_id),
            None => None,
        }
    }

    pub(crate) async fn set_conn_sender(&self, sender: ConnSender) {
        trace!("Context set conn sender {:?}", sender);
        self.conn_map
            .lock()
            .await
            .insert(sender.get_conn_id(), sender);
    }

    pub(crate) async fn get_conn_sender(&self) -> Option<ConnSender> {
        trace!("Context get conn sender {:?}", self.current_conn_id);
        if let Some(conn_id) = self.current_conn_id {
            self.conn_map.lock().await.get(&conn_id).cloned()
        } else {
            None
        }
    }

    pub(crate) async fn remove_conn_sender(&self) {
        trace!("Context remove conn sender {:?}", self.current_conn_id);
        if let Some(conn_id) = self.current_conn_id {
            let _discard = self.conn_map.lock().await.remove(&conn_id);
        }
    }

    pub(crate) fn get_conn_id(&self) -> anyhow::Result<u64> {
        self.current_conn_id
            .context("context current conn id is empty")
    }
}

#[derive(Debug, Clone)]
pub struct ConnSender {
    inner: Arc<ConnInner>,
}

#[derive(Debug)]
struct ConnInner {
    pub conn_id: u64,
    pub sender: mpsc::Sender<Bytes>,
}

impl ConnSender {
    pub fn new(conn_id: u64, sender: mpsc::Sender<Bytes>) -> Self {
        ConnSender {
            inner: Arc::new(ConnInner { conn_id, sender }),
        }
    }

    pub fn get_conn_id(&self) -> u64 {
        self.inner.conn_id
    }

    pub async fn send(&self, byte: Bytes) -> anyhow::Result<()> {
        use anyhow::Context;

        self.inner
            .sender
            .send(byte)
            .await
            .context("Can't send byte to conn channel")
    }
}

/// command sender
#[derive(Debug, Clone)]
pub struct CommandSender {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    pub hash: String,
    pub sender: mpsc::Sender<CommandChannel>,
    id_adder: IdAdder,
}

impl CommandSender {
    pub fn new(hash: String, sender: mpsc::Sender<CommandChannel>) -> Self {
        CommandSender {
            inner: Arc::new(Inner {
                hash,
                sender,
                id_adder: IdAdder::default(),
            }),
        }
    }

    pub fn get_hash(&self) -> String {
        self.inner.hash.clone()
    }

    pub async fn send_with_id(&self, req_id: u64, cmd: Command) -> anyhow::Result<()> {
        Ok(self.inner.sender.send((req_id, cmd, None)).await?)
    }

    pub async fn send_sync(&self, cmd: Command) -> anyhow::Result<()> {
        let (tx, rx) = oneshot::channel();
        let req_id = self.inner.id_adder.add_and_get().await;
        self.inner.sender.send((req_id, cmd, Some(tx))).await?;
        trace!("send_sync {} send", req_id);
        match rx.await? {
            Ok(_) => {
                trace!("send_sync {} recv resp", req_id);
                Ok(())
            }
            Err(e) => {
                error!(req_id, error = %e, "send_sync response failed");
                Err(anyhow!("send_sync resp err {e}"))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct IdAdder {
    inner: Arc<Mutex<u64>>,
}

impl Default for IdAdder {
    fn default() -> Self {
        IdAdder {
            inner: Arc::new(Mutex::new(0)),
        }
    }
}

impl IdAdder {
    pub async fn add_and_get(&self) -> u64 {
        let mut i = self.inner.lock().await;
        *i += 1;
        *i
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use tokio::sync::{mpsc, oneshot};

    use super::*;

    #[tokio::test]
    async fn test_context_new_creates_shutdown_channel() {
        let (tx, _rx) = mpsc::channel(1);
        let sender = CommandSender::new("hash".to_string(), tx);
        let context = Context::new(sender);

        let mut rx = context.notify_shutdown.subscribe();
        context.notify_shutdown.send(()).ok();

        assert!(rx.recv().await.is_ok());
    }

    #[tokio::test]
    async fn test_context_set_and_get_req() {
        let (tx, _rx) = mpsc::channel(1);
        let sender = CommandSender::new("hash".to_string(), tx);
        let mut context = Context::new(sender);

        let (req_tx, _req_rx) = oneshot::channel::<anyhow::Result<()>>();
        context.current_req_id = Some(1);
        context.set_req(Some(req_tx)).await;

        context.current_req_id = Some(1);
        let retrieved = context.get_req().await;
        assert!(retrieved.is_some());
    }

    #[tokio::test]
    async fn test_context_set_and_get_conn_sender() {
        let (tx, _rx) = mpsc::channel(1);
        let sender = CommandSender::new("hash".to_string(), tx);
        let context = Context::new(sender);

        let (conn_tx, _conn_rx) = mpsc::channel(1);
        let conn_sender = ConnSender::new(1, conn_tx);

        let mut ctx = context.clone();
        ctx.current_conn_id = Some(1);
        ctx.set_conn_sender(conn_sender).await;

        let mut ctx2 = context.clone();
        ctx2.current_conn_id = Some(1);
        let retrieved = ctx2.get_conn_sender().await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().get_conn_id(), 1);
    }

    #[tokio::test]
    async fn test_context_remove_conn_sender() {
        let (tx, _rx) = mpsc::channel(1);
        let sender = CommandSender::new("hash".to_string(), tx);
        let context = Context::new(sender);

        let (conn_tx, _conn_rx) = mpsc::channel(1);
        let conn_sender = ConnSender::new(1, conn_tx);

        let mut ctx = context.clone();
        ctx.set_conn_sender(conn_sender).await;

        ctx.current_conn_id = Some(1);
        ctx.remove_conn_sender().await;

        let mut ctx2 = context.clone();
        ctx2.current_conn_id = Some(1);
        let retrieved = ctx2.get_conn_sender().await;
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_command_sender_get_hash() {
        let (rx, _) = mpsc::channel::<CommandChannel>(1);
        let sender = CommandSender::new("test_hash".to_string(), rx);

        assert_eq!(sender.get_hash(), "test_hash");
    }

    #[tokio::test]
    async fn test_id_adder_increments() {
        let adder = IdAdder::default();

        let id1 = adder.add_and_get().await;
        let id2 = adder.add_and_get().await;
        let id3 = adder.add_and_get().await;

        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(id3, 3);
    }

    #[tokio::test]
    async fn test_conn_sender_send_bytes() {
        let (tx, mut rx) = mpsc::channel(1);
        let sender = ConnSender::new(1, tx);

        let result = sender.send(Bytes::from("test data")).await;
        assert!(result.is_ok());

        let received = rx.recv().await;
        assert!(received.is_some());
        assert_eq!(received.unwrap(), Bytes::from("test data"));
    }

    #[tokio::test]
    async fn test_conn_sender_get_conn_id() {
        let (tx, _) = mpsc::channel(1);
        let sender = ConnSender::new(42, tx);

        assert_eq!(sender.get_conn_id(), 42);
    }

    #[tokio::test]
    async fn test_context_get_conn_id_success() {
        let (tx, _rx) = mpsc::channel(1);
        let sender = CommandSender::new("hash".to_string(), tx);
        let mut context = Context::new(sender);

        context.current_conn_id = Some(42);

        let result = context.get_conn_id();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_context_get_conn_id_failure() {
        let (tx, _rx) = mpsc::channel(1);
        let sender = CommandSender::new("hash".to_string(), tx);
        let context = Context::new(sender);

        let result = context.get_conn_id();
        assert!(result.is_err());
    }
}
