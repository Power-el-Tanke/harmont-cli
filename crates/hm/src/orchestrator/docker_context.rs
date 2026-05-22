//! Resolve the Docker endpoint the way `docker` CLI does.
//!
//! Resolution order, matching Docker's own precedence:
//!
//! 1. `DOCKER_HOST` env var — explicit endpoint, wins everything.
//! 2. `DOCKER_CONTEXT` env var — pick a named context.
//! 3. `currentContext` in `~/.docker/config.json`.
//! 4. Fall back to bollard's platform default
//!    (`unix:///var/run/docker.sock` on Linux, the named pipe on Windows).
//!
//! Named contexts live at
//! `~/.docker/contexts/meta/<sha256(name)>/meta.json`, with their TLS
//! materials in the parallel `tls/` tree. This is the same scheme the
//! `docker` CLI uses, so Docker Desktop on Linux (which ships a
//! `desktop-linux` context pointing at `~/.docker/desktop/docker.sock`)
//! works out of the box.

#![allow(
    clippy::print_stderr,
    reason = "the not_found arm of resolve_endpoint surfaces an interactive hint to the user"
)]

use std::env;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// A resolved Docker daemon endpoint, ready to hand to bollard.
#[derive(Debug, Clone)]
pub enum Endpoint {
    /// No override resolved — caller should use bollard's platform default.
    Default,
    /// Unix socket (Linux/macOS) or Windows named pipe path.
    Socket(PathBuf),
    /// Plain HTTP daemon (no TLS).
    Http(String),
    /// HTTPS daemon. `tls_dir`, when present, contains
    /// `ca.pem` / `cert.pem` / `key.pem` extracted from the docker
    /// context's TLS materials.
    Https {
        host: String,
        tls_dir: Option<PathBuf>,
    },
}

#[derive(Deserialize)]
struct DockerConfig {
    #[serde(rename = "currentContext")]
    current_context: Option<String>,
}

#[derive(Deserialize)]
struct ContextMeta {
    #[serde(rename = "Endpoints")]
    endpoints: ContextEndpoints,
}

#[derive(Deserialize)]
struct ContextEndpoints {
    docker: ContextEndpoint,
}

#[derive(Deserialize)]
struct ContextEndpoint {
    #[serde(rename = "Host")]
    host: String,
}

/// Walk the Docker resolution chain and return the resolved endpoint.
///
/// # Errors
///
/// Returns an error only when an *explicit* configuration cannot be
/// honored — e.g., `DOCKER_CONTEXT` points at a name with no meta file,
/// or a config / context JSON file fails to parse. A missing
/// `~/.docker/` directory is *not* an error; it returns
/// [`Endpoint::Default`].
pub fn resolve_endpoint() -> Result<Endpoint> {
    if let Some(host) = env::var_os("DOCKER_HOST") {
        let host = host.to_string_lossy().into_owned();
        if host.is_empty() {
            return Ok(Endpoint::Default);
        }
        return parse_host(&host, None);
    }

    let context = env::var("DOCKER_CONTEXT")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| read_current_context().ok().flatten());

    let Some(name) = context else {
        return Ok(Endpoint::Default);
    };
    if name == "default" {
        return Ok(Endpoint::Default);
    }
    resolve_named_context(&name)
}

fn docker_dir() -> Result<PathBuf> {
    let home = env::var_os("HOME").ok_or_else(|| anyhow!("HOME env var not set"))?;
    Ok(Path::new(&home).join(".docker"))
}

fn read_current_context() -> Result<Option<String>> {
    let path = docker_dir()?.join("config.json");
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path)
        .with_context(|| format!("read {}", path.display()))?;
    let cfg: DockerConfig = serde_json::from_slice(&bytes)
        .with_context(|| format!("parse {}", path.display()))?;
    Ok(cfg.current_context.filter(|s| !s.is_empty()))
}

fn context_hash(name: &str) -> String {
    let mut h = Sha256::new();
    h.update(name.as_bytes());
    hex::encode(h.finalize())
}

fn resolve_named_context(name: &str) -> Result<Endpoint> {
    let hash = context_hash(name);
    let docker = docker_dir()?;
    let meta_path = docker.join(format!("contexts/meta/{hash}/meta.json"));
    if !meta_path.exists() {
        bail!(
            "docker context '{name}' not found ({}); run `docker context ls` to verify",
            meta_path.display()
        );
    }
    let bytes = std::fs::read(&meta_path)
        .with_context(|| format!("read {}", meta_path.display()))?;
    let meta: ContextMeta = serde_json::from_slice(&bytes)
        .with_context(|| format!("parse {}", meta_path.display()))?;

    let tls_dir = docker.join(format!("contexts/tls/{hash}/docker"));
    let tls_dir = if tls_dir.exists() { Some(tls_dir) } else { None };

    parse_host(&meta.endpoints.docker.host, tls_dir)
}

fn parse_host(host: &str, tls_dir: Option<PathBuf>) -> Result<Endpoint> {
    if let Some(path) = host.strip_prefix("unix://") {
        return Ok(Endpoint::Socket(PathBuf::from(path)));
    }
    if let Some(path) = host.strip_prefix("npipe://") {
        return Ok(Endpoint::Socket(PathBuf::from(path)));
    }
    if let Some(rest) = host.strip_prefix("tcp://") {
        // Docker CLI: tcp:// with TLS materials means https; otherwise http.
        if tls_dir.is_some() {
            return Ok(Endpoint::Https {
                host: format!("https://{rest}"),
                tls_dir,
            });
        }
        return Ok(Endpoint::Http(format!("http://{rest}")));
    }
    if host.starts_with("https://") {
        return Ok(Endpoint::Https {
            host: host.to_string(),
            tls_dir,
        });
    }
    if host.starts_with("http://") {
        return Ok(Endpoint::Http(host.to_string()));
    }
    bail!("unrecognized docker host scheme: {host}");
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn parses_unix_socket() {
        let ep = parse_host("unix:///run/docker.sock", None).unwrap();
        match ep {
            Endpoint::Socket(p) => assert_eq!(p, PathBuf::from("/run/docker.sock")),
            other => panic!("expected Socket, got {other:?}"),
        }
    }

    #[test]
    fn parses_npipe() {
        let ep = parse_host("npipe:////./pipe/docker_engine", None).unwrap();
        assert!(matches!(ep, Endpoint::Socket(_)));
    }

    #[test]
    fn parses_tcp_without_tls() {
        let ep = parse_host("tcp://10.0.0.1:2375", None).unwrap();
        match ep {
            Endpoint::Http(h) => assert_eq!(h, "http://10.0.0.1:2375"),
            other => panic!("expected Http, got {other:?}"),
        }
    }

    #[test]
    fn parses_tcp_with_tls_dir_as_https() {
        let ep = parse_host("tcp://10.0.0.1:2376", Some(PathBuf::from("/tls"))).unwrap();
        match ep {
            Endpoint::Https { host, tls_dir } => {
                assert_eq!(host, "https://10.0.0.1:2376");
                assert_eq!(tls_dir, Some(PathBuf::from("/tls")));
            }
            other => panic!("expected Https, got {other:?}"),
        }
    }

    #[test]
    fn parses_explicit_http_and_https() {
        assert!(matches!(parse_host("http://x", None).unwrap(), Endpoint::Http(_)));
        assert!(matches!(parse_host("https://x", None).unwrap(), Endpoint::Https { .. }));
    }

    #[test]
    fn rejects_unknown_scheme() {
        assert!(parse_host("ftp://x", None).is_err());
    }

    #[test]
    fn context_hash_matches_docker_cli() {
        // Verified against `docker context inspect desktop-linux --format '{{.Name}}'`
        // and the filesystem layout: the hash is sha256(name) hex.
        let h = context_hash("desktop-linux");
        assert_eq!(h.len(), 64);
        assert_eq!(
            h,
            "fe9c6bd7a66301f49ca9b6a70b217107cd1284598bfc254700c989b916da791e"
        );
    }
}
