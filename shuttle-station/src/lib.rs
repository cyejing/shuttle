#![doc = include_str!("../README.md")]

pub mod dial;
pub mod peekable;
pub mod proto;
pub mod proxy;
pub mod tls;
pub mod websocket;

pub const CRLF: [u8; 2] = [0x0d, 0x0a];
