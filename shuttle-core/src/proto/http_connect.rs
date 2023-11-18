use std::{str::Split, sync::Arc};

use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt};

#[allow(dead_code)]
const REQUEST_END_MARKER: &[u8] = b"\r\n\r\n";
/// A reasonable value to limit possible header size.
#[allow(dead_code)]
const MAX_HTTP_REQUEST_SIZE: usize = 16384;

#[derive(Debug, Clone, Error)]
pub enum HttpResult {
    #[error("HttpConnectRequest parse failed")]
    BadRequest,
}

#[allow(dead_code)]
pub struct HttpConnectRequest {
    pub host: String,
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

#[allow(dead_code)]
impl HttpConnectRequest {
    pub fn parse(http_request: &[u8]) -> Result<Self, HttpResult> {
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

        if has_nugget {
            Ok(Self {
                host: HttpConnectRequest::extract_destination_host(&mut lines, request_line.1)
                    .unwrap_or_else(|| request_line.1.to_string()),
                nugget: Some(Nugget::new(http_request)),
            })
        } else {
            Ok(Self {
                host: request_line.1.to_string(),
                nugget: None,
            })
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

    fn parse_request_line(request_line: &str) -> Result<(&str, &str, &str, bool), HttpResult> {
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
    ) -> Result<(), HttpResult> {
        if request_line_items.len() != 3 {
            debug!("Bad request line: `{:?}`", request_line,);
            Err(HttpResult::BadRequest)
        } else {
            Ok(())
        }
    }

    fn check_version(version: &str) -> Result<(), HttpResult> {
        if version != "HTTP/1.1" {
            debug!("Bad version {}", version);
            Err(HttpResult::BadRequest)
        } else {
            Ok(())
        }
    }

    fn check_method(method: &str) -> Result<bool, HttpResult> {
        Ok(method != "CONNECT")
    }

    fn precondition_legal_characters(http_request: &[u8]) -> Result<(), HttpResult> {
        for b in http_request {
            match b {
                // non-ascii characters don't make sense in this context
                32..=126 | 9 | 10 | 13 => {}
                _ => {
                    debug!("Bad request header. Illegal character: {:#04x}", b);
                    return Err(HttpResult::BadRequest);
                }
            }
        }
        Ok(())
    }

    fn precondition_size(http_request: &[u8]) -> Result<(), HttpResult> {
        if http_request.len() >= MAX_HTTP_REQUEST_SIZE {
            debug!(
                "Bad request header. Size {} exceeds limit {}",
                http_request.len(),
                MAX_HTTP_REQUEST_SIZE
            );
            Err(HttpResult::BadRequest)
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
