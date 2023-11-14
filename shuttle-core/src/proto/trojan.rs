use socks5_proto::Address;

#[derive(Clone, Debug)]
pub struct Request {
    pub hash: String,
    pub address: Address,
}
