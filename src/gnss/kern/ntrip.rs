//! NTRIP client (Rev1/Rev2 over plain TCP) for SAPOS HEPS.
//! VRS mountpoints require the rover position as NMEA GGA upload.
//!
//! Hardware: simpleRTK2B (u-blox ZED-F9P)
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/ntrip.rs), eigenständig gepflegt in Feldbuch-NT.

use anyhow::{Context, Result, bail};
use base64::Engine;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct NtripConfig {
    pub host: String,
    pub port: u16,
    pub mountpoint: String,
    pub user: String,
    pub password: String,
    pub rev2: bool,
}

impl NtripConfig {
    fn auth(&self) -> String {
        base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", self.user, self.password))
    }

    fn connect_tcp(&self) -> Result<TcpStream> {
        let addr = (self.host.as_str(), self.port)
            .to_socket_addrs()?
            .next()
            .context("DNS: no address")?;
        let s = TcpStream::connect_timeout(&addr, Duration::from_secs(10))?;
        s.set_read_timeout(Some(Duration::from_secs(30)))?;
        s.set_nodelay(true)?;
        Ok(s)
    }

    fn request(&self, path: &str, gga: Option<&str>) -> String {
        let mut r = format!(
            "GET /{path} HTTP/1.{}\r\nHost: {}:{}\r\nUser-Agent: NTRIP gnss-vermessung/{}\r\nAuthorization: Basic {}\r\n",
            if self.rev2 { 1 } else { 0 },
            self.host,
            self.port,
            env!("CARGO_PKG_VERSION"),
            self.auth()
        );
        if self.rev2 {
            r.push_str("Ntrip-Version: Ntrip/2.0\r\n");
            if let Some(g) = gga {
                r.push_str(&format!("Ntrip-GGA: {g}\r\n"));
            }
        }
        r.push_str("Connection: close\r\n\r\n");
        r
    }

    /// Fetch the caster's source table (list of mountpoints).
    pub fn sourcetable(&self) -> Result<Vec<String>> {
        let mut s = self.connect_tcp()?;
        s.write_all(self.request("", None).as_bytes())?;
        let mut reader = BufReader::new(s);
        let mut status = String::new();
        reader.read_line(&mut status)?;
        if status.contains("401") {
            bail!(
                "NTRIP: Authentifizierung fehlgeschlagen ({})",
                status.trim()
            );
        }
        // Rev1 antwortet mit "SOURCETABLE 200 OK", Rev2 mit HTTP-Kopf (ggf. chunked)
        let chunked = read_header(&mut reader, &mut Vec::new())?;
        let mut body = Vec::new();
        // bei Abbruch oder Zeitüberschreitung die bis dahin empfangenen Zeilen verwenden
        Body::new(reader, chunked).read_to_end(&mut body).ok();
        Ok(String::from_utf8_lossy(&body)
            .lines()
            .filter(|l| l.starts_with("STR;"))
            .map(str::to_string)
            .collect())
    }

    /// Open the correction stream. For Rev1, the initial GGA is sent after the header.
    /// `log` erhält die gesendete Anfrage (ohne Zugangsdaten) und den Antwortkopf.
    pub fn open(
        &self,
        gga: Option<&str>,
        log: &mut Vec<(Direction, String)>,
    ) -> Result<NtripStream> {
        log.push((
            Direction::Info,
            format!("TCP-Verbindung zu {}:{}", self.host, self.port),
        ));
        let mut s = self.connect_tcp()?;
        let request = self.request(&self.mountpoint, gga);
        log.extend(masked(&request).into_iter().map(|l| (Direction::Sent, l)));
        s.write_all(request.as_bytes())?;

        let mut reader = BufReader::new(s.try_clone()?);
        let mut status = String::new();
        reader.read_line(&mut status)?;
        log.push((Direction::Received, status.trim_end().to_string()));
        let ok = status.starts_with("ICY 200")
            || status.starts_with("HTTP/1.1 200")
            || status.starts_with("HTTP/1.0 200");
        if !ok {
            if status.contains("401") {
                bail!(
                    "NTRIP: Authentifizierung fehlgeschlagen ({})",
                    status.trim()
                );
            }
            bail!("NTRIP: unerwartete Antwort: {}", status.trim());
        }
        // skip remaining header (Rev2 or HTTP-style)
        let mut header = Vec::new();
        let chunked = !status.starts_with("ICY") && read_header(&mut reader, &mut header)?;
        log.extend(header.into_iter().map(|l| (Direction::Received, l)));
        let mut st = NtripStream {
            reader: Body::new(reader, chunked),
            writer: s,
        };
        if !self.rev2 {
            if let Some(g) = gga {
                st.send_gga(g)?;
                log.push((Direction::Sent, g.trim().to_string()));
            }
        }
        Ok(st)
    }
}

/// Richtung eines Eintrags im Kommunikationsprotokoll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Sent,
    Received,
    Info,
}

/// Zeilen der Anfrage; die Zugangsdaten werden unkenntlich gemacht.
fn masked(request: &str) -> Vec<String> {
    request
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            if l.starts_with("Authorization:") {
                "Authorization: Basic ******** (verborgen)".to_string()
            } else {
                l.to_string()
            }
        })
        .collect()
}

/// Liest den Antwortkopf bis zur Leerzeile; `true`, wenn die Daten chunked folgen.
fn read_header(reader: &mut impl BufRead, lines: &mut Vec<String>) -> std::io::Result<bool> {
    let mut chunked = false;
    loop {
        let mut l = String::new();
        if reader.read_line(&mut l)? == 0 || l == "\r\n" || l == "\n" {
            return Ok(chunked);
        }
        lines.push(l.trim_end().to_string());
        let l = l.to_ascii_lowercase();
        if l.starts_with("transfer-encoding:") && l.contains("chunked") {
            chunked = true;
        }
    }
}

/// Nutzdaten der Antwort, bei Bedarf aus der HTTP-Chunked-Kodierung ausgepackt.
struct Body<R> {
    inner: R,
    chunked: bool,
    /// noch nicht gelesene Bytes des laufenden Chunks
    remaining: usize,
    finished: bool,
}

impl<R: BufRead> Body<R> {
    fn new(inner: R, chunked: bool) -> Self {
        Body {
            inner,
            chunked,
            remaining: 0,
            finished: false,
        }
    }

    /// Liest die Größenzeile des nächsten Chunks (hexadezimal, Erweiterungen nach ';').
    fn next_chunk_size(&mut self) -> std::io::Result<usize> {
        loop {
            let mut l = String::new();
            if self.inner.read_line(&mut l)? == 0 {
                return Ok(0);
            }
            let size = l.split(';').next().unwrap_or("").trim();
            // Leerzeile = Abschluss des vorherigen Chunks
            if size.is_empty() {
                continue;
            }
            return usize::from_str_radix(size, 16).map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "ungültige Chunk-Größe")
            });
        }
    }
}

impl<R: BufRead> Read for Body<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if !self.chunked {
            return self.inner.read(buf);
        }
        if self.finished || buf.is_empty() {
            return Ok(0);
        }
        if self.remaining == 0 {
            self.remaining = self.next_chunk_size()?;
            if self.remaining == 0 {
                self.finished = true;
                return Ok(0);
            }
        }
        let max = buf.len().min(self.remaining);
        let n = self.inner.read(&mut buf[..max])?;
        self.remaining -= n;
        Ok(n)
    }
}

pub struct NtripStream {
    reader: Body<BufReader<TcpStream>>,
    writer: TcpStream,
}

impl NtripStream {
    pub fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.reader.read(buf)
    }

    pub fn send_gga(&mut self, gga: &str) -> Result<()> {
        self.writer.write_all(gga.trim().as_bytes())?;
        self.writer.write_all(b"\r\n")?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn writer_clone(&self) -> Result<TcpStream> {
        Ok(self.writer.try_clone()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunked_body() {
        let raw: &[u8] = b"Transfer-Encoding: chunked\r\n\r\n4\r\n\xd3\x00\r\n\r\nA;x=1\r\n0123456789\r\n0\r\n\r\n";
        let mut r = BufReader::with_capacity(3, raw);
        assert!(read_header(&mut r, &mut Vec::new()).unwrap());
        let mut out = Vec::new();
        Body::new(r, true).read_to_end(&mut out).unwrap();
        assert_eq!(out, b"\xd3\x00\r\n0123456789");
    }

    #[test]
    fn credentials_are_masked() {
        let cfg = NtripConfig {
            host: "h".into(),
            port: 1,
            mountpoint: "m".into(),
            user: "benutzer".into(),
            password: "geheim".into(),
            rev2: true,
        };
        let lines = masked(&cfg.request("m", None)).join("\n");
        assert!(lines.contains("verborgen"));
        assert!(!lines.contains(&cfg.auth()) && !lines.contains("geheim"));
    }

    #[test]
    fn plain_body() {
        let raw: &[u8] = b"Content-Type: gnss/data\r\n\r\n4\r\nabc";
        let mut r = BufReader::new(raw);
        assert!(!read_header(&mut r, &mut Vec::new()).unwrap());
        let mut out = Vec::new();
        Body::new(r, false).read_to_end(&mut out).unwrap();
        assert_eq!(out, b"4\r\nabc");
    }
}
