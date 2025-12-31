extern crate core;
#[macro_use]
extern crate log;

pub mod auth;
pub mod client;
pub mod config;
pub mod rathole;
pub mod server;
pub mod setup;
pub mod store;
pub mod websocket;

pub const CRLF: [u8; 2] = [0x0d, 0x0a];
