//! HTTP GET abstraction. [`Http`] is the seam: the real [`ReqwestHttp`] hits
//! the network; tests substitute a fake that returns canned bodies, so all the
//! per-registry parsing is tested offline.

use crate::error::DownstatError;

/// A minimal HTTP GET. `Ok(Some(body))` on 2xx, `Ok(None)` on 404 (not
/// published), `Err` on anything else.
pub trait Http: Sync {
    fn get(&self, url: &str) -> Result<Option<String>, DownstatError>;
}

/// The real client: reqwest blocking with rustls.
pub struct ReqwestHttp {
    client: reqwest::blocking::Client,
    github_token: Option<String>,
}

impl ReqwestHttp {
    pub fn new() -> Result<Self, DownstatError> {
        let client = reqwest::blocking::Client::builder()
            .user_agent(concat!("downstat/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| DownstatError::Http {
                message: e.to_string(),
            })?;
        // A token lifts GitHub's 60 req/hr unauthenticated limit; optional.
        let github_token = std::env::var("GITHUB_TOKEN")
            .ok()
            .or_else(|| std::env::var("GH_TOKEN").ok())
            .filter(|t| !t.is_empty());
        Ok(Self {
            client,
            github_token,
        })
    }
}

impl Http for ReqwestHttp {
    fn get(&self, url: &str) -> Result<Option<String>, DownstatError> {
        let mut req = self.client.get(url);
        if url.contains("api.github.com")
            && let Some(token) = &self.github_token
        {
            req = req.bearer_auth(token);
        }
        let resp = req.send().map_err(|e| DownstatError::Http {
            message: e.to_string(),
        })?;
        let status = resp.status().as_u16();
        if status == 404 {
            return Ok(None);
        }
        if !(200..300).contains(&status) {
            return Err(DownstatError::Http {
                message: format!("{url} returned HTTP {status}"),
            });
        }
        resp.text().map(Some).map_err(|e| DownstatError::Http {
            message: e.to_string(),
        })
    }
}
