use tracing::Subscriber;
use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use shuttle::config::ClientConfig;

use shuttle::socks::Socks;

#[tokio::main]
async fn main() -> shuttle::Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .init();

    let cc = ClientConfig::load(String::from("examples/shuttlec.yaml"));

    Socks::new(cc).start().await
}
