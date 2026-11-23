//! GitHub PR operations on the caller's host and repository.
mod merge;
mod merge_reason;
mod provisional;
mod title;

pub(crate) use merge::{MergeArgs, merge};
pub(crate) use provisional::{ProvisionalArgs, provisional_pr};
pub(crate) use title::{TitleArgs, sync_title};

use std::env;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use ureq::tls::{PemItem, RootCerts, TlsConfig};

/// All endpoints are derived from the trusted base host. The HTTP client never
/// follows redirects carrying authorization to another origin.
pub(crate) struct Client {
    agent: ureq::Agent,
    server: String,
    rest: String,
    graphql_url: String,
    repo: String,
    token: String,
}

fn valid_repo(repo: &str) -> bool {
    let Some((owner, name)) = repo.split_once('/') else {
        return false;
    };
    !owner.is_empty()
        && !name.is_empty()
        && name != "."
        && name != ".."
        && owner
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
}

fn valid_host(host: &str) -> bool {
    let name = if let Some((name, port)) = host.rsplit_once(':') {
        if !port.parse::<u16>().is_ok_and(|p| p > 0) {
            return false;
        }
        name
    } else {
        host
    };
    name.split('.').all(|part| {
        !part.is_empty()
            && !part.starts_with('-')
            && !part.ends_with('-')
            && part.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
    })
}

fn credential(host: &str) -> Result<String> {
    let keys = if host == "github.com" || host.ends_with(".ghe.com") {
        ["GH_TOKEN", "GITHUB_TOKEN"]
    } else {
        ["GH_ENTERPRISE_TOKEN", "GITHUB_ENTERPRISE_TOKEN"]
    };
    for key in keys {
        if let Ok(value) = env::var(key)
            && !value.is_empty()
        {
            return Ok(value);
        }
    }
    // Existing gh sessions are a convenience; the released CLI can operate
    // without gh, including with a Git credential helper scoped to this host.
    if let Ok(output) = Command::new("gh")
        .args(["auth", "token", "--hostname", host])
        .output()
        && output.status.success()
    {
        let token = String::from_utf8(output.stdout)?;
        if !token.trim().is_empty() {
            return Ok(token.trim().to_owned());
        }
    }
    let mut child = Command::new("git")
        .args(["-c", "credential.interactive=false", "credential", "fill"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("cannot query Git credentials")?;
    if let Some(mut input) = child.stdin.take() {
        write!(input, "protocol=https\nhost={host}\n\n")?;
    }
    let output = child.wait_with_output()?;
    if output.status.success() {
        let response = String::from_utf8(output.stdout)?;
        if let Some(token) = response.lines().find_map(|l| l.strip_prefix("password="))
            && !token.is_empty()
        {
            return Ok(token.to_owned());
        }
    }
    bail!(
        "authenticate to {host} using {} or a configured credential helper",
        keys[0]
    );
}

fn http_agent() -> Result<ureq::Agent> {
    let roots = if let Some(path) = env::var_os("SSL_CERT_FILE").filter(|p| !p.is_empty()) {
        let contents = std::fs::read(path).context("could not read SSL_CERT_FILE")?;
        let mut certs = Vec::new();
        for item in ureq::tls::parse_pem(&contents) {
            if let PemItem::Certificate(c) = item.context("invalid TLS trust bundle")? {
                certs.push(c);
            }
        }
        ensure!(!certs.is_empty(), "SSL_CERT_FILE contains no certificates");
        RootCerts::new_with_certs(&certs)
    } else {
        RootCerts::PlatformVerifier
    };
    Ok(ureq::Agent::config_builder()
        .https_only(true)
        .max_redirects(0)
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(30)))
        .tls_config(TlsConfig::builder().root_certs(roots).build())
        .build()
        .into())
}

impl Client {
    pub(crate) fn new(host: &str, repo: &str) -> Result<Self> {
        ensure!(valid_host(host), "--hostname must be a GitHub hostname");
        ensure!(valid_repo(repo), "--repo must be OWNER/REPO");
        let server = format!("https://{host}");
        let api = if host == "github.com" || host.ends_with(".ghe.com") {
            format!("https://api.{host}")
        } else {
            format!("{server}/api/v3")
        };
        Self::with_token(&server, &api, repo, credential(host)?)
    }

    fn with_token(server: &str, api: &str, repo: &str, token: String) -> Result<Self> {
        let server = server.trim_end_matches('/');
        let api = api.trim_end_matches('/');
        let host = server
            .strip_prefix("https://")
            .context("GITHUB_SERVER_URL must be an HTTPS origin")?;
        ensure!(
            valid_host(host),
            "GITHUB_SERVER_URL must be an HTTPS origin"
        );
        ensure!(valid_repo(repo), "invalid GitHub repository");
        let graphql_url = if api == format!("{server}/api/v3") {
            format!("{server}/api/graphql")
        } else {
            ensure!(
                api == format!("https://api.{host}"),
                "GITHUB_API_URL must belong to the base repository's GitHub host"
            );
            format!("{api}/graphql")
        };
        ensure!(
            !token.is_empty() && token.bytes().all(|c| c.is_ascii_graphic()),
            "invalid GitHub credential"
        );
        Ok(Self {
            agent: http_agent()?,
            server: server.into(),
            rest: format!("{api}/repos/{repo}"),
            graphql_url,
            repo: repo.into(),
            token,
        })
    }

    fn send(
        &self,
        method: &str,
        url: &str,
        payload: Option<&Value>,
    ) -> Result<ureq::http::Response<ureq::Body>> {
        let auth = format!("Bearer {}", self.token);
        let mut request = ureq::http::Request::builder()
            .method(method)
            .uri(url)
            .header("User-Agent", concat!("gitmoji/", env!("CARGO_PKG_VERSION")))
            .header("Authorization", auth)
            .header("Accept", "application/vnd.github+json");
        let bytes = if let Some(value) = payload {
            request = request.header("Content-Type", "application/json");
            serde_json::to_vec(value)?
        } else {
            Vec::new()
        };
        self.agent
            .run(request.body(&bytes[..])?)
            .with_context(|| format!("GitHub {method} request failed"))
    }

    fn request(&self, method: &str, url: &str, payload: Option<&Value>) -> Result<Value> {
        let mut response = self.send(method, url, payload)?;
        let status = response.status();
        if status.as_u16() == 422 {
            bail!(
                "GitHub rejected the {method} (HTTP 422). Check title length and repository validation rules."
            );
        }
        ensure!(
            status.is_success(),
            "GitHub rejected the {method} (HTTP {})",
            status.as_u16()
        );
        response
            .body_mut()
            .read_json()
            .context("invalid GitHub JSON response")
    }

    pub(crate) fn graphql(&self, query: &str, variables: &Value) -> Result<Value> {
        let value = self.request(
            "POST",
            &self.graphql_url,
            Some(&json!({"query":query,"variables":variables})),
        )?;
        ensure!(
            !value["errors"].is_array() || value["errors"].as_array().is_some_and(Vec::is_empty),
            "GitHub rejected the GraphQL query; check repository access and host support"
        );
        value
            .get("data")
            .filter(|d| d.is_object())
            .cloned()
            .context("missing GitHub GraphQL result")
    }

    pub(crate) fn get_pull(&self, number: u64) -> Result<Value> {
        self.request("GET", &format!("{}/pulls/{number}", self.rest), None)
    }

    pub(crate) fn update_title(&self, number: u64, title: &str) -> Result<Value> {
        self.request(
            "PATCH",
            &format!("{}/pulls/{number}", self.rest),
            Some(&json!({"title":title})),
        )
    }
}

fn valid_pr(number: u64) -> Result<()> {
    ensure!(
        number > 0 && number <= i32::MAX as u64,
        "use a positive GitHub pull request number"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use std::io::{BufRead, BufReader, Read};
    use std::net::TcpListener;

    #[test]
    fn actions_endpoints_must_belong_to_the_base_origin() {
        for (server, api) in [
            ("https://github.com", "https://api.github.com"),
            ("https://a.ghe.com", "https://api.a.ghe.com"),
            (
                "https://example.invalid:8443",
                "https://example.invalid:8443/api/v3",
            ),
        ] {
            let client = Client::with_token(server, api, "octo/repo", "fake".into()).unwrap();
            assert!(client.graphql_url.starts_with(server) || client.graphql_url.starts_with(api));
        }
        for (server, api, repo) in [
            ("https://a.ghe.com", "https://api.github.com", "octo/repo"),
            (
                "https://user@example.org",
                "https://api.user@example.org",
                "octo/repo",
            ),
            (
                "https://example.org/path",
                "https://example.org/path/api/v3",
                "octo/repo",
            ),
            (
                "http://example.org",
                "http://example.org/api/v3",
                "octo/repo",
            ),
            (
                "https://example.org",
                "https://example.org/api/v3",
                "octo/..",
            ),
        ] {
            assert!(Client::with_token(server, api, repo, "fake".into()).is_err());
        }
    }

    #[test]
    fn rest_and_graphql_requests_bind_authentication_and_paths_to_the_configured_host() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let server = format!("http://{}", listener.local_addr().unwrap());
        let other = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        other.set_nonblocking(true).unwrap();
        let redirect = format!("http://{}/different", other.local_addr().unwrap());
        let worker = std::thread::spawn(move || {
            let mut received = Vec::new();
            for (code, response) in [
                (200, r#"{"number":2}"#),
                (200, r#"{"title":"changed"}"#),
                (200, r#"{"data":{"repository":{}}}"#),
                (200, r#"{"errors":[{"message":"denied"}]}"#),
                (302, "{}"),
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut rd = BufReader::new(&stream);
                let mut headline = String::new();
                rd.read_line(&mut headline).unwrap();
                let mut length = 0;
                let mut authorized = false;
                loop {
                    let mut line = String::new();
                    rd.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    if let Some((name, value)) = line.split_once(':') {
                        if name.eq_ignore_ascii_case("content-length") {
                            length = value.trim().parse().unwrap();
                        }
                        if name.eq_ignore_ascii_case("authorization") {
                            authorized = value.trim() == "Bearer fixture";
                        }
                    }
                }
                let mut input = vec![0; length];
                rd.read_exact(&mut input).unwrap();
                assert!(authorized);
                received.push((headline.trim_end().to_string(), input));
                let location = if code == 302 {
                    format!("Location: {redirect}\r\n")
                } else {
                    String::new()
                };
                write!(stream,"HTTP/1.1 {code} Response\r\n{location}Content-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).unwrap();
            }
            received
        });
        let client = Client {
            agent: ureq::Agent::config_builder()
                .proxy(None)
                .max_redirects(0)
                .build()
                .into(),
            rest: format!("{server}/api/v3/repos/octo/repo"),
            graphql_url: format!("{server}/api/graphql"),
            repo: "octo/repo".into(),
            token: "fixture".into(),
            server,
        };
        assert_eq!(client.get_pull(2).unwrap()["number"], 2);
        assert_eq!(
            client.update_title(2, "changed").unwrap()["title"],
            "changed"
        );
        assert_eq!(
            client.graphql("query Data", &json!({"n":2})).unwrap(),
            json!({"repository":{}})
        );
        assert!(client.graphql("query Missing", &json!({})).is_err());
        assert!(client.get_pull(3).is_err());
        assert!(other.accept().is_err());
        let received = worker.join().unwrap();
        assert_eq!(
            received[0].0,
            "GET /api/v3/repos/octo/repo/pulls/2 HTTP/1.1"
        );
        assert_eq!(
            received[1].0,
            "PATCH /api/v3/repos/octo/repo/pulls/2 HTTP/1.1"
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&received[1].1).unwrap(),
            json!({"title":"changed"})
        );
        assert_eq!(received[2].0, "POST /api/graphql HTTP/1.1");
        assert_eq!(
            serde_json::from_slice::<Value>(&received[2].1).unwrap(),
            json!({"query":"query Data","variables":{"n":2}})
        );
    }
}
