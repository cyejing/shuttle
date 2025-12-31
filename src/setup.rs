use rolling_file::{BasicRollingFileAppender, RollingConditionBasic};
use std::{fs, mem::forget};
use uuid::Uuid;

use time::macros::{format_description, offset};
use tracing::{Level, level_filters::LevelFilter};

use tracing_subscriber::{
    EnvFilter, Layer as _,
    filter::filter_fn,
    fmt::{layer, time::OffsetTime},
    layer::SubscriberExt as _,
    util::SubscriberInitExt as _,
};

pub fn setup_log() {
    let timer = OffsetTime::new(
        offset!(+8),
        format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]+[offset_hour][offset_minute]"
        ),
    );
    if fs::metadata("logs").is_err() {
        fs::create_dir_all("logs").expect("create logs dir failed");
    }

    let (app_aped, g1) = tracing_appender::non_blocking(
        BasicRollingFileAppender::new(
            "logs/app.log",
            RollingConditionBasic::new()
                .daily()
                .max_size(1024 * 1024 * 1024),
            9,
        )
        .expect("rolling file failed, mabey logs dir"),
    );
    let (error_aped, g2) = tracing_appender::non_blocking(
        BasicRollingFileAppender::new(
            "logs/error.log",
            RollingConditionBasic::new()
                .daily()
                .max_size(1024 * 1024 * 512),
            19,
        )
        .expect("rolling file failed, mabey logs dir"),
    );

    let mut layers = Vec::new();

    let app = layer()
        .with_writer(app_aped)
        .with_timer(timer.clone())
        .with_line_number(true)
        .with_filter(default_env_filter())
        .boxed();
    layers.push(app);

    let error = layer()
        .with_writer(error_aped)
        .with_ansi(false)
        .with_timer(timer.clone())
        .with_line_number(true)
        .with_filter(default_env_filter())
        .with_filter(filter_fn(|m| {
            m.name().starts_with("tracing")
                || m.level() == &Level::ERROR
                || m.level() == &Level::WARN
        }))
        .boxed();
    layers.push(error);

    let stdout = layer()
        .with_timer(timer.clone())
        .with_line_number(true)
        .with_filter(default_env_filter())
        .boxed();
    layers.push(stdout);

    tracing_subscriber::registry().with(layers).try_init().ok();

    forget(g1);
    forget(g2);
}

fn default_env_filter() -> EnvFilter {
    EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env()
        .unwrap()
}
pub fn gen_traceid() -> String {
    let (_high, low) = Uuid::new_v4().as_u64_pair();
    format!("{:016x}", low)
}
