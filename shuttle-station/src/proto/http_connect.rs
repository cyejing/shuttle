use std::{str::Split, sync::Arc};

use anyhow::anyhow;
use log::debug;
use socks5_proto::Address;
use tokio::io::{AsyncRead, AsyncReadExt};

const MAX_HTTP_REQUEST_SIZE: usize = 16384;

pub struct HttpConnectRequest {
    pub addr: Address,
    pub nugget: Option<Nugget>,
}

#[derive(Eq, PartialEq, Debug, Clone)]
pub struct Nugget {
    data: Arc<Vec<u8>>,
}

pub async fn read_http_request_end<T: AsyncRead + Unpin>(r: &mut T) -> anyhow::Result<Vec<u8>> {
    let mut buf = Vec::new();
    for _i in 0..MAX_HTTP_REQUEST_SIZE {
        let u1 = r.read_u8().await?;
        buf.push(u1);
        if u1 == b'\r' {
            let [u2, u3, u4] = {
                let mut x = [0u8; 3];
                r.read_exact(&mut x).await.map(|_| x)
            }?;
            buf.push(u2);
            buf.push(u3);
            buf.push(u4);
            if u2 == b'\n' && u3 == b'\r' && u4 == b'\n' {
                break;
            }
        }
    }
    Ok(buf)
}

impl HttpConnectRequest {
    pub fn parse(http_request: &[u8]) -> anyhow::Result<Self> {
        HttpConnectRequest::precondition_size(http_request)?;
        HttpConnectRequest::precondition_legal_characters(http_request)?;

        let http_request_as_string =
            String::from_utf8(http_request.to_vec()).expect("Contains only ASCII");

        let mut lines = http_request_as_string.split("\r\n");

        let request_line = HttpConnectRequest::parse_request_line(
            lines
                .next()
                .expect("At least a single line is present at this point"),
        )?;

        let has_nugget = request_line.3;

        let (host, nugget) = if has_nugget {
            (
                HttpConnectRequest::extract_destination_host(&mut lines, request_line.1)
                    .unwrap_or_else(|| request_line.1.to_string()),
                Some(Nugget::new(http_request)),
            )
        } else {
            (request_line.1.to_string(), None)
        };

        Ok(Self {
            addr: Self::host_to_address(host)?,
            nugget,
        })
    }

    fn host_to_address(host: String) -> anyhow::Result<Address> {
        let mut split: Vec<&str> = host.split(':').collect();
        if split.len() == 2 {
            let port = split.pop().expect("host_to_address split pop failed");
            let domain = split.pop().expect("host_to_address split pop failed");
            Ok(Address::DomainAddress(
                domain.as_bytes().to_vec(),
                port.parse()?,
            ))
        } else {
            Err(anyhow::anyhow!(format!(
                "http connect host to adddress failed: {} ",
                host
            )))
        }
    }

    fn extract_destination_host(lines: &mut Split<&str>, endpoint: &str) -> Option<String> {
        const HOST_HEADER: &str = "host:";

        lines
            .find(|line| line.to_ascii_lowercase().starts_with(HOST_HEADER))
            .map(|line| line[HOST_HEADER.len()..].trim())
            .map(|host| {
                let mut host = String::from(host);
                if host.rfind(':').is_none() {
                    let default_port = if endpoint.to_ascii_lowercase().starts_with("https://") {
                        ":443"
                    } else {
                        ":80"
                    };
                    host.push_str(default_port);
                }
                host
            })
    }

    fn parse_request_line(request_line: &str) -> anyhow::Result<(&str, &str, &str, bool)> {
        let request_line_items = request_line.split(' ').collect::<Vec<&str>>();
        HttpConnectRequest::precondition_well_formed(request_line, &request_line_items)?;

        let method = request_line_items[0];
        let uri = request_line_items[1];
        let version = request_line_items[2];

        let has_nugget = HttpConnectRequest::check_method(method)?;
        HttpConnectRequest::check_version(version)?;

        Ok((method, uri, version, has_nugget))
    }

    fn precondition_well_formed(
        request_line: &str,
        request_line_items: &[&str],
    ) -> anyhow::Result<()> {
        if request_line_items.len() != 3 {
            debug!("Bad request line: `{:?}`", request_line,);
            Err(anyhow!("BadRequest"))
        } else {
            Ok(())
        }
    }

    fn check_version(version: &str) -> anyhow::Result<()> {
        if version != "HTTP/1.1" {
            debug!("Bad version {}", version);
            Err(anyhow!("BadRequest"))
        } else {
            Ok(())
        }
    }

    fn check_method(method: &str) -> anyhow::Result<bool> {
        Ok(method != "CONNECT")
    }

    fn precondition_legal_characters(http_request: &[u8]) -> anyhow::Result<()> {
        for b in http_request {
            match b {
                // non-ascii characters don't make sense in this context
                32..=126 | 9 | 10 | 13 => {}
                _ => {
                    debug!("Bad request header. Illegal character: {:#04x}", b);
                    return Err(anyhow!("BadRequest"));
                }
            }
        }
        Ok(())
    }

    fn precondition_size(http_request: &[u8]) -> anyhow::Result<()> {
        if http_request.len() >= MAX_HTTP_REQUEST_SIZE {
            debug!(
                "Bad request header. Size {} exceeds limit {}",
                http_request.len(),
                MAX_HTTP_REQUEST_SIZE
            );
            Err(anyhow!("BadRequest"))
        } else {
            Ok(())
        }
    }
}

impl Nugget {
    pub fn new<T: Into<Vec<u8>>>(v: T) -> Self {
        Self {
            data: Arc::new(v.into()),
        }
    }

    pub fn data(&self) -> Arc<Vec<u8>> {
        self.data.clone()
    }
}
