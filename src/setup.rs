use rolling_file::{BasicRollingFileAppender, RollingConditionBasic};
use std::path::Path;
use std::{fs, mem::forget};
use uuid::Uuid;

use time::macros::{format_description, offset};
use tracing::{Level, level_filters::LevelFilter};
use tracing_log::LogTracer;

use tracing_subscriber::{
    EnvFilter, Layer as _,
    filter::filter_fn,
    fmt::{self, layer, time::OffsetTime},
    layer::SubscriberExt as _,
    util::SubscriberInitExt as _,
};

pub fn setup_log(log_path: impl AsRef<Path>) -> anyhow::Result<()> {
    let log_path = log_path.as_ref();
    let app_log = log_path.join("app.log");
    let error_log = log_path.join("error.log");
    let timer = OffsetTime::new(
        offset!(+8),
        format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]+[offset_hour][offset_minute]"
        ),
    );
    if fs::metadata(log_path).is_err() {
        fs::create_dir_all(log_path)?;
    }

    let (app_appender, g1) = tracing_appender::non_blocking(BasicRollingFileAppender::new(
        app_log,
        RollingConditionBasic::new()
            .daily()
            .max_size(1024 * 1024 * 1024),
        9,
    )?);
    let (error_appender, g2) = tracing_appender::non_blocking(BasicRollingFileAppender::new(
        error_log,
        RollingConditionBasic::new()
            .daily()
            .max_size(1024 * 1024 * 512),
        19,
    )?);

    let mut layers = Vec::new();
    let event_format = fmt::format()
        .compact()
        .with_level(true)
        .with_target(false)
        .with_file(false)
        .with_line_number(false)
        .with_thread_ids(false)
        .with_thread_names(false);

    let app = layer()
        .event_format(event_format.clone())
        .with_writer(app_appender)
        .with_ansi(false)
        .with_timer(timer.clone())
        .with_filter(default_env_filter())
        .boxed();
    layers.push(app);

    let error = layer()
        .event_format(event_format.clone())
        .with_writer(error_appender)
        .with_ansi(false)
        .with_timer(timer.clone())
        .with_filter(default_env_filter())
        .with_filter(filter_fn(|m| {
            m.name().starts_with("tracing")
                || m.level() == &Level::ERROR
                || m.level() == &Level::WARN
        }))
        .boxed();
    layers.push(error);

    let stdout = layer()
        .event_format(event_format)
        .with_timer(timer.clone())
        .with_filter(default_env_filter())
        .boxed();
    layers.push(stdout);

    let _ = LogTracer::init();
    tracing_subscriber::registry().with(layers).try_init()?;

    forget(g1);
    forget(g2);
    Ok(())
}

fn default_env_filter() -> EnvFilter {
    EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env()
        .unwrap()
}
#[must_use]
pub fn gen_traceid() -> String {
    let (_high, low) = Uuid::new_v4().as_u64_pair();
    format!("{low:016x}")
}

#[cfg(test)]
mod tests {
    use super::gen_traceid;

    #[test]
    fn gen_traceid_returns_16_char_hex_string() {
        let trace_id = gen_traceid();

        assert_eq!(trace_id.len(), 16);
        assert!(trace_id.chars().all(|ch| ch.is_ascii_hexdigit()));
    }

    #[test]
    fn gen_traceid_has_low_collision_risk() {
        let first = gen_traceid();
        let second = gen_traceid();

        assert_ne!(first, second);
    }
}
