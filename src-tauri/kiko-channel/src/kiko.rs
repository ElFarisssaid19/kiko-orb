//! Talks to Kiko's local server: a tiny HTTP/1.1 client, enough for JSON on 127.0.0.1.

use std::{
    fmt, fs,
    io::{self, Read, Write},
    net::{SocketAddr, TcpStream},
    path::Path,
    time::Duration,
};

use serde::Serialize;

use crate::protocol::{Connection, Endpoint, TOKEN_HEADER};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub enum KikoError {
    /// Kiko is closed, or didn't answer in time.
    Unreachable(io::Error),
    /// Kiko answered with an HTTP error, e.g. 401 (wrong token), 404 (unknown channel) or 409 (no
    /// session for this channel right now).
    Status(u16),
}

impl fmt::Display for KikoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KikoError::Unreachable(_) => write!(f, "Kiko is not running"),
            KikoError::Status(401) => write!(f, "Kiko refused the channel's token"),
            KikoError::Status(409) => write!(f, "Kiko doesn't know which session this channel belongs to yet"),
            KikoError::Status(status) => write!(f, "Kiko answered HTTP {status}"),
        }
    }
}

/// Kiko's server address and token, from Kiko's config.json.
#[derive(Clone, Debug, PartialEq)]
pub struct Kiko {
    port: u16,
    token: String,
}

impl Kiko {
    pub fn new(port: u16, token: impl Into<String>) -> Self {
        Self {
            port,
            token: token.into(),
        }
    }

    /// Reads the port and token from Kiko's config.json.
    pub fn load(config: &Path) -> io::Result<Self> {
        let connection: Connection =
            serde_json::from_str(&fs::read_to_string(config)?).map_err(io::Error::other)?;
        Ok(Self::new(connection.port, connection.token))
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// POSTs `body` as JSON and returns the response body of a 2xx answer.
    pub fn post<T: Serialize>(
        &self,
        endpoint: Endpoint,
        body: &T,
        timeout: Duration,
    ) -> Result<Vec<u8>, KikoError> {
        let body = serde_json::to_vec(body).map_err(|e| KikoError::Unreachable(e.into()))?;
        let response = self
            .exchange(&endpoint.url_path(), &body, timeout)
            .map_err(KikoError::Unreachable)?;
        let (status, body) = parse_response(&response).ok_or_else(|| {
            KikoError::Unreachable(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid HTTP answer",
            ))
        })?;
        if (200..300).contains(&status) {
            Ok(body)
        } else {
            Err(KikoError::Status(status))
        }
    }

    fn exchange(&self, path: &str, body: &[u8], timeout: Duration) -> io::Result<Vec<u8>> {
        let address = SocketAddr::from(([127, 0, 0, 1], self.port));
        let mut stream = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT)?;
        stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
        stream.set_read_timeout(Some(timeout))?;
        let head = format!(
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{TOKEN_HEADER}: {token}\r\n\
             Content-Type: application/json\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n",
            port = self.port,
            token = self.token,
            length = body.len()
        );
        stream.write_all(head.as_bytes())?;
        stream.write_all(body)?;
        let mut response = Vec::new();
        stream.read_to_end(&mut response)?;
        Ok(response)
    }
}

/// (status, body) of a complete HTTP/1.1 response.
fn parse_response(response: &[u8]) -> Option<(u16, Vec<u8>)> {
    let split = response.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&response[..split]).ok()?;
    let body = &response[split + 4..];
    let mut lines = head.split("\r\n");
    let status = lines.next()?.split_whitespace().nth(1)?.parse().ok()?;
    let header = |name: &str| {
        head.split("\r\n").skip(1).find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.trim().eq_ignore_ascii_case(name).then(|| value.trim().to_string())
        })
    };
    if header("Transfer-Encoding").is_some_and(|v| v.eq_ignore_ascii_case("chunked")) {
        return Some((status, dechunk(body)?));
    }
    let body = match header("Content-Length").and_then(|v| v.parse::<usize>().ok()) {
        Some(length) => body.get(..length)?.to_vec(),
        None => body.to_vec(),
    };
    Some((status, body))
}

fn dechunk(mut body: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let line_end = body.windows(2).position(|w| w == b"\r\n")?;
        let size_text = std::str::from_utf8(&body[..line_end]).ok()?;
        let size = usize::from_str_radix(size_text.split(';').next()?.trim(), 16).ok()?;
        body = &body[line_end + 2..];
        if size == 0 {
            return Some(out);
        }
        out.extend_from_slice(body.get(..size)?);
        body = body.get(size + 2..)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responses_with_a_length() {
        let response = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}";
        assert_eq!(parse_response(response), Some((200, b"{}".to_vec())));
        assert_eq!(
            parse_response(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n"),
            Some((404, Vec::new()))
        );
    }

    #[test]
    fn chunked_responses() {
        let response = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\n{\"a\"\r\n3\r\n:1}\r\n0\r\n\r\n";
        assert_eq!(parse_response(response), Some((200, b"{\"a\":1}".to_vec())));
    }

    #[test]
    fn truncated_responses_are_rejected() {
        assert_eq!(parse_response(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\n{}"), None);
        assert_eq!(parse_response(b"HTTP/1.1 200"), None);
    }

    #[test]
    fn a_closed_kiko_is_unreachable() {
        // Bind then drop: nothing listens on that port any more.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let error = Kiko::new(port, "t")
            .post(Endpoint::Register, &serde_json::json!({}), Duration::from_secs(1))
            .unwrap_err();
        assert!(matches!(error, KikoError::Unreachable(_)));
        assert_eq!(error.to_string(), "Kiko is not running");
    }
}
