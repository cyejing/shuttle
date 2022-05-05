use std::sync::Arc;

use shuttle::config::ClientConfig;
use shuttle::logs::init_log;
use shuttle::socks::{Socks, TrojanDial};

#[tokio::main]
async fn main() -> shuttle::Result<()> {
    init_log();

    let cc = ClientConfig::load(String::from("examples/shuttlec.yaml"));

    let dial = Arc::new(TrojanDial::new(cc.remote_addr.clone(),
                                        cc.hash.clone(),
                                        cc.ssl_enable));
    Socks::new(cc, dial).start().await
}
