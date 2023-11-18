use time::macros::{format_description, offset};
use tracing::metadata::LevelFilter;
use tracing_subscriber::{
    fmt::{layer, time::OffsetTime},
    prelude::__tracing_subscriber_SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter, Layer,
};
use uuid::Uuid;

extern crate core;
#[macro_use]
extern crate log;

pub mod client;
pub mod config;
pub mod rathole;
pub mod server;
pub mod store;

pub const CRLF: [u8; 2] = [0x0d, 0x0a];

pub fn init_log() {
    let timer = OffsetTime::new(
            offset!(+8),
            format_description!(
                "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]+[offset_hour][offset_minute]"
            ),
        );
    let stdout = layer()
        .with_timer(timer.clone())
        .with_line_number(true)
        .with_filter(default_env_filter());

    tracing_subscriber::registry()
        .with(stdout)
        .try_init()
        .unwrap();
}

fn default_env_filter() -> EnvFilter {
    EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy()
}

pub fn gen_traceid() -> String {
    let (_high, low) = Uuid::new_v4().as_u64_pair();
    format!("{:016x}", low)
}
