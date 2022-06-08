use std::path::PathBuf;

use clap::Parser;

use shuttle::client::{start_proxy, start_rathole};
use shuttle::config::ClientConfig;
use shuttle::logs::init_log;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Config Path
    #[clap(parse(from_os_str), name = "CONFIG_PATH")]
    config_path: Option<PathBuf>,
    #[clap(short, long, default_value = "trojan")]
    proxy_mode: String,
}

#[tokio::main]
async fn main() {
    init_log();

    let args: Args = Args::parse();

    let cc = ClientConfig::load(args.config_path);
    match cc.run_type.as_str() {
        "proxy" => start_proxy(cc, args.proxy_mode).await,
        "rathole" => start_rathole(cc).await,
        _ => panic!("unknown run type : {}", cc.run_type),
    }

    tokio::signal::ctrl_c().await.expect("shut down");
}
