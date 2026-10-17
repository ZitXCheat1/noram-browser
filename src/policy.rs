use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use url::Url;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub shields: bool,
    pub https_only: bool,
    pub strip_tracking: bool,
    pub max_live_tabs: usize,
    pub exceptions: HashSet<String>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            shields: true,
            https_only: true,
            strip_tracking: true,
            max_live_tabs: 4,
            exceptions: HashSet::new(),
        }
    }
}

pub fn host_matches(host: &str, rule: &str) -> bool {
    host == rule || host.strip_suffix(rule).is_some_and(|p| p.ends_with('.'))
}

pub struct Blocker {
    domains: HashSet<String>,
}
impl Default for Blocker {
    fn default() -> Self {
        Self {
            domains: include_str!("../assets/blocklist.txt")
                .lines()
                .map(str::trim)
                .filter(|s| !s.is_empty() && !s.starts_with('#'))
                .map(str::to_owned)
                .collect(),
        }
    }
}
impl Blocker {
    pub fn blocks(&self, input: &str) -> bool {
        let Ok(url) = Url::parse(input) else {
            return false;
        };
        let Some(mut host) = url.host_str() else {
            return false;
        };
        loop {
            if self.domains.contains(host) {
                return true;
            }
            match host.split_once('.') {
                Some((_, rest)) => host = rest,
                None => return false,
            }
        }
    }
    pub fn len(&self) -> usize {
        self.domains.len()
    }
}

pub fn local_host(url: &Url) -> bool {
    matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
}

pub fn clean_url(input: &str, settings: &Settings) -> Result<String, String> {
    let mut url = Url::parse(input).map_err(|_| "Invalid address".to_owned())?;
    if !matches!(url.scheme(), "https" | "http") {
        return Err("Only HTTP and HTTPS addresses are supported".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Addresses containing credentials are not allowed".into());
    }
    if settings.https_only && url.scheme() == "http" && !local_host(&url) {
        url.set_scheme("https").map_err(|_| "Cannot upgrade URL")?;
    }
    if settings.strip_tracking {
        let pairs: Vec<(String, String)> = url
            .query_pairs()
            .filter(|(key, _)| {
                let key = key.to_ascii_lowercase();
                !(key.starts_with("utm_")
                    || matches!(
                        key.as_str(),
                        "fbclid" | "gclid" | "msclkid" | "dclid" | "mc_cid" | "mc_eid"
                    ))
            })
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        if url.query().is_some() {
            url.set_query(None);
            if !pairs.is_empty() {
                url.query_pairs_mut().extend_pairs(pairs);
            }
        }
    }
    Ok(url.into())
}

pub fn address(input: &str, settings: &Settings) -> Result<String, String> {
    let text = input.trim();
    if text.is_empty() || text == "noram://newtab" {
        return Ok("noram://newtab".into());
    }
    if text.contains("://")
        || text.starts_with("javascript:")
        || text.starts_with("data:")
        || text.starts_with("file:")
    {
        return clean_url(text, settings);
    }
    if !text.chars().any(char::is_whitespace)
        && (text.contains('.') || text.starts_with("localhost"))
    {
        return clean_url(
            &format!(
                "{}://{text}",
                if text.starts_with("localhost") || text.starts_with("127.0.0.1") {
                    "http"
                } else {
                    "https"
                }
            ),
            settings,
        );
    }
    let mut url = Url::parse("https://duckduckgo.com/").unwrap();
    url.query_pairs_mut().append_pair("q", text);
    Ok(url.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blocks_subdomains_without_overmatching() {
        let b = Blocker::default();
        assert!(b.blocks("https://a.doubleclick.net/ad.js"));
        assert!(!b.blocks("https://notdoubleclick.net/"));
        assert!(!b.blocks("https://example.org/doubleclick.net"));
    }
    #[test]
    fn tracking_removal_preserves_functional_parameters() {
        assert_eq!(
            clean_url(
                "http://example.org/?q=a%26b&utm_source=x&fbclid=y#part",
                &Settings::default()
            )
            .unwrap(),
            "https://example.org/?q=a%26b#part"
        );
    }
    #[test]
    fn rejects_active_and_local_schemes() {
        for input in [
            "javascript:alert(1)",
            "file:///C:/secret",
            "data:text/html,hi",
            "https://user:password@example.org",
        ] {
            assert!(address(input, &Settings::default()).is_err());
        }
    }
    #[test]
    fn searches_and_allows_local_development() {
        let s = Settings::default();
        assert_eq!(
            address("rust browser", &s).unwrap(),
            "https://duckduckgo.com/?q=rust+browser"
        );
        assert_eq!(
            address("localhost:8080", &s).unwrap(),
            "http://localhost:8080/"
        );
        assert!(host_matches("news.example.org", "example.org"));
        assert!(!host_matches("badexample.org", "example.org"));
    }
}
