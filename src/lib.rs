extern crate core;
#[macro_use]
extern crate log;

pub mod admin;
pub mod client;
pub mod config;
pub mod logs;
pub mod proxy;
pub mod rathole;
pub mod server;
pub mod store;
pub mod tls;

pub const CRLF: [u8; 2] = [0x0d, 0x0a];

#[macro_export]
macro_rules! read_exact {
    ($stream: expr, $array: expr) => {{
        let mut x = $array;
        //        $stream
        //            .read_exact(&mut x)
        //            .await
        //            .map_err(|_| io_err("lol"))?;
        $stream.read_exact(&mut x).await.map(|_| x)
    }};
}
