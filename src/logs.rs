use time::macros::{format_description, offset};
use tracing::metadata::LevelFilter;
use tracing_subscriber::{
    fmt::{layer, time::OffsetTime},
    prelude::__tracing_subscriber_SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter, Layer,
};

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
