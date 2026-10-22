//! Server address parsing and the tiny HTTP/1.1 client the SDK needs for map
//! chunks and match creation. One request per connection; no TLS (the server
//! speaks plain HTTP on a trusted network).
use crate::error::{Result, SdkError};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::{Duration, timeout},
};

const MAX_BODY: usize = 8 * 1024 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// `host:port` of a server, from any of `http://h:p`, `ws://h:p/game/ws` or `h:p`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Endpoint {
    pub authority: String,
}

impl Endpoint {
    pub fn parse(url: &str) -> Result<Self> {
        let rest = ["http://", "ws://"]
            .iter()
            .find_map(|scheme| url.strip_prefix(scheme))
            .unwrap_or(url);
        let authority = rest.split('/').next().unwrap_or("");
        let valid_host_port = authority
            .rsplit_once(':')
            .is_some_and(|(host, port)| !host.is_empty() && port.parse::<u16>().is_ok());
        if !valid_host_port
            || authority.contains(['@', ' ', '?', '#'])
            || url.starts_with("https://")
            || url.starts_with("wss://")
        {
            return Err(SdkError::InvalidUrl(url.to_owned()));
        }
        Ok(Self {
            authority: authority.to_owned(),
        })
    }

    pub fn websocket_url(&self) -> String {
        format!("ws://{}/game/ws", self.authority)
    }

    /// Sends one request and returns `(status, body)`.
    pub async fn request(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: Option<&[u8]>,
    ) -> Result<(u16, Vec<u8>)> {
        self.request_within(REQUEST_TIMEOUT, method, path, headers, body)
            .await
    }

    /// Like [`Endpoint::request`] with a caller-chosen deadline, for requests
    /// that do real work on the server (starting a large match).
    pub async fn request_within(
        &self,
        limit: Duration,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: Option<&[u8]>,
    ) -> Result<(u16, Vec<u8>)> {
        timeout(limit, self.exchange(method, path, headers, body))
            .await
            .map_err(|_| SdkError::Timeout)?
    }

    async fn exchange(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: Option<&[u8]>,
    ) -> Result<(u16, Vec<u8>)> {
        let mut stream = TcpStream::connect(&self.authority).await?;
        let mut head = format!(
            "{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n",
            self.authority
        );
        for (name, value) in headers {
            if name.contains(['\r', '\n']) || value.contains(['\r', '\n']) {
                return Err(SdkError::InvalidUrl("header injection".to_owned()));
            }
            head.push_str(&format!("{name}: {value}\r\n"));
        }
        if let Some(body) = body {
            head.push_str(&format!(
                "Content-Type: application/json\r\nContent-Length: {}\r\n",
                body.len()
            ));
        }
        head.push_str("\r\n");
        stream.write_all(head.as_bytes()).await?;
        if let Some(body) = body {
            stream.write_all(body).await?;
        }
        let mut raw = Vec::new();
        (&mut stream)
            .take(MAX_BODY as u64 + 8192)
            .read_to_end(&mut raw)
            .await?;
        parse_response(&raw)
    }

    pub async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let (status, body) = self.request("GET", path, &[], None).await?;
        check_status(status, &body)?;
        Ok(serde_json::from_slice(&body)?)
    }
}

pub(crate) fn check_status(status: u16, body: &[u8]) -> Result<()> {
    if (200..300).contains(&status) {
        Ok(())
    } else {
        Err(SdkError::Http {
            status,
            body: String::from_utf8_lossy(&body[..body.len().min(512)]).into_owned(),
        })
    }
}

fn parse_response(raw: &[u8]) -> Result<(u16, Vec<u8>)> {
    let split = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| SdkError::Closed("truncated http response".to_owned()))?;
    let head = String::from_utf8_lossy(&raw[..split]).into_owned();
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| SdkError::Closed("malformed http status".to_owned()))?;
    let chunked = head.lines().any(|line| {
        let line = line.to_ascii_lowercase();
        line.starts_with("transfer-encoding:") && line.contains("chunked")
    });
    let body = &raw[split + 4..];
    let body = if chunked {
        dechunk(body)?
    } else {
        body.to_vec()
    };
    if body.len() > MAX_BODY {
        return Err(SdkError::Closed("http body too large".to_owned()));
    }
    Ok((status, body))
}

fn dechunk(mut data: &[u8]) -> Result<Vec<u8>> {
    let bad = || SdkError::Closed("malformed chunked body".to_owned());
    let mut out = Vec::new();
    loop {
        let eol = data
            .windows(2)
            .position(|window| window == b"\r\n")
            .ok_or_else(bad)?;
        let size = std::str::from_utf8(&data[..eol])
            .ok()
            .and_then(|text| usize::from_str_radix(text.split(';').next()?.trim(), 16).ok())
            .ok_or_else(bad)?;
        data = &data[eol + 2..];
        if size == 0 {
            return Ok(out);
        }
        if data.len() < size + 2 {
            return Err(bad());
        }
        out.extend_from_slice(&data[..size]);
        data = &data[size + 2..];
    }
}
