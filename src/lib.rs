extern crate core;
extern crate tokio;
extern crate tokio_rustls;

pub mod config;
pub mod server;
pub mod socks;
pub mod common;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

pub type Result<T> = std::result::Result<T, Error>;
