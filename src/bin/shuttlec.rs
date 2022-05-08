use std::sync::Arc;

use shuttle::config::ClientConfig;
use shuttle::logs::init_log;
use shuttle::socks::{Socks, TrojanDial};
use clap::Parser;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args{
    /// Config Path
    #[clap(short, long)]
    config_path: Option<String>,
}

#[tokio::main]
async fn main() {
    init_log();

    let args: Args= Args::parse();

    let cc = ClientConfig::load(args.config_path);

    let dial = Arc::new(TrojanDial::new(cc.remote_addr.clone(),
                                        cc.hash.clone(),
                                        cc.ssl_enable));
    Socks::new(cc, dial).start().await
}
