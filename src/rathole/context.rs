use crate::rathole::cmd::Command;
use crate::rathole::{CommandChannel, ReqChannel};
use bytes::Bytes;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};

#[derive(Debug, Clone)]
pub struct Context {
    pub command_sender: Arc<CommandSender>,
    pub conn_map: Arc<Mutex<HashMap<u64, Arc<ConnSender>>>>,
    pub req_map: Arc<Mutex<HashMap<u64, ReqChannel>>>,
    pub current_req_id: Option<u64>,
    pub current_conn_id: Option<u64>,
}

impl Context {
    pub fn new(command_sender: Arc<CommandSender>) -> Self {
        Context {
            command_sender,
            current_req_id: None,
            current_conn_id: None,
            conn_map: Arc::new(Mutex::new(HashMap::new())),
            req_map: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(crate) fn with_req_id(&mut self, req_id: u64) {
        self.current_req_id = Some(req_id);
    }

    pub(crate) fn with_conn_id(&mut self, conn_id: u64) {
        self.current_conn_id = Some(conn_id);
    }

    pub(crate) async fn set_req(&self, req_channel: ReqChannel) {
        if let Some(req_id) = self.current_req_id {
            if req_channel.is_some(){
                self.req_map.lock().await.insert(req_id, req_channel);
            }
        }
    }

    pub(crate) async fn get_req(&self) -> Option<ReqChannel> {
        match &self.current_req_id {
            Some(req_id) => {
                return self.req_map.lock().await.remove(req_id);
            }
            None => None,
        }
    }

    pub(crate) async fn set_conn_sender(&self, sender: Arc<ConnSender>) {
        self.conn_map.lock().await.insert(sender.conn_id, sender);
    }

    pub(crate) async fn get_conn_sender(&self) -> Option<Arc<ConnSender>> {
        match &self.current_conn_id {
            Some(conn_id) => self.conn_map.lock().await.get(conn_id).cloned(),
            None => panic!("context current conn id is empty"),
        }
    }

    pub(crate) async fn remove_conn_sender(&self) -> Option<Arc<ConnSender>>{
        match &self.current_conn_id {
            Some(conn_id) => self.conn_map.lock().await.remove(conn_id),
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

    pub async fn send(&self, byte: Bytes) -> crate::Result<()> {
        Ok(self.sender.send(byte).await?)
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

    pub async fn send(&self, cmd: Command) -> crate::Result<()> {
        let req_id = self.id_adder.add_and_get().await;
        Ok(self.sender.send((req_id, cmd, None)).await?)
    }

    pub async fn send_with_id(&self, req_id: u64, cmd: Command) -> crate::Result<()> {
        Ok(self.sender.send((req_id, cmd, None)).await?)
    }

    pub async fn send_sync(&self, cmd: Command) -> crate::Result<()> {
        let (tx, rx) = oneshot::channel();
        let req_id = self.id_adder.add_and_get().await;
        self.sender.send((req_id, cmd, Some(tx))).await?;
        rx.await?
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
