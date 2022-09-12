use crate::rathole::cmd::Command;
use crate::rathole::{CommandChannel, ReqChannel};
use anyhow::anyhow;
use bytes::Bytes;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};

#[derive(Debug, Clone)]
pub struct Context {
    pub notify_shutdown: broadcast::Sender<()>,
    pub command_sender: Arc<CommandSender>,
    pub conn_map: Arc<Mutex<HashMap<u64, Arc<ConnSender>>>>,
    pub req_map: Arc<Mutex<HashMap<u64, ReqChannel>>>,
    pub current_req_id: Option<u64>,
    pub current_conn_id: Option<u64>,
}

impl Context {
    pub fn new(command_sender: Arc<CommandSender>) -> Self {
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
            self.current_req_id,
            req_channel
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

    pub(crate) async fn set_conn_sender(&self, sender: Arc<ConnSender>) {
        trace!("Context set conn sender {:?}", sender);
        self.conn_map.lock().await.insert(sender.conn_id, sender);
    }

    pub(crate) async fn get_conn_sender(&self) -> Option<Arc<ConnSender>> {
        trace!("Context get conn sender {:?}", self.current_conn_id);
        match &self.current_conn_id {
            Some(conn_id) => self.conn_map.lock().await.get(conn_id).cloned(),
            None => panic!("context current conn id is empty"),
        }
    }

    pub(crate) async fn remove_conn_sender(&self) {
        trace!("Context remove conn sender {:?}", self.current_conn_id);
        match &self.current_conn_id {
            Some(conn_id) => {
                let discard = self.conn_map.lock().await.remove(conn_id);
                drop(discard);
            }
            None => panic!("context current conn id is empty"),
        }
    }

    pub(crate) fn get_conn_id(&self) -> u64 {
        match self.current_conn_id {
            Some(conn_id) => conn_id,
            None => panic!("context current conn id is empty"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConnSender {
    pub conn_id: u64,
    pub sender: mpsc::Sender<Bytes>,
}

impl ConnSender {
    pub fn new(conn_id: u64, sender: mpsc::Sender<Bytes>) -> Self {
        ConnSender { conn_id, sender }
    }

    pub async fn send(&self, byte: Bytes) -> anyhow::Result<()> {
        use anyhow::Context;

        self.sender
            .send(byte)
            .await
            .context("Can't send byte to conn channel")
    }
}

/// command sender
#[derive(Debug, Clone)]
pub struct CommandSender {
    pub hash: String,
    pub sender: mpsc::Sender<CommandChannel>,
    id_adder: IdAdder,
}

impl CommandSender {
    pub fn new(hash: String, sender: mpsc::Sender<CommandChannel>) -> Self {
        CommandSender {
            hash,
            sender,
            id_adder: IdAdder::default(),
        }
    }

    pub async fn send_with_id(&self, req_id: u64, cmd: Command) -> anyhow::Result<()> {
        Ok(self.sender.send((req_id, cmd, None)).await?)
    }

    pub async fn send_sync(&self, cmd: Command) -> anyhow::Result<()> {
        let (tx, rx) = oneshot::channel();
        let req_id = self.id_adder.add_and_get().await;
        self.sender.send((req_id, cmd, Some(tx))).await?;
        trace!("send_sync {} send", req_id);
        match rx.await? {
            Ok(_) => {
                trace!("send_sync {} recv resp", req_id);
                Ok(())
            }
            Err(e) => {
                error!("send_sync {} await resp err {}", req_id, e);
                Err(anyhow!(format!("await resp err {}", e)))
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
mod tests {}
