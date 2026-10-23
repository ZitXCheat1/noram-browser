use adblock::{
    lists::{FilterSet, ParseOptions},
    request::Request,
    Engine,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use url::Url;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub shields: bool,
    pub https_only: bool,
    pub strip_tracking: bool,
    pub max_live_tabs: usize,
    pub exceptions: HashSet<String>,
    pub search_engine: String,
    pub theme: String,
    pub bookmarks_bar: bool,
    pub cosmetic_filtering: bool,
    pub permissions: HashMap<String, HashMap<String, bool>>,
    pub auto_filter_updates: bool,
    pub auto_app_updates: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            shields: true,
            https_only: true,
            strip_tracking: true,
            max_live_tabs: 4,
            exceptions: HashSet::new(),
            search_engine: "duckduckgo".into(),
            theme: "system".into(),
            bookmarks_bar: false,
            cosmetic_filtering: true,
            permissions: HashMap::new(),
            auto_filter_updates: true,
            auto_app_updates: true,
        }
    }
}

pub fn host_matches(host: &str, rule: &str) -> bool {
    host == rule || host.strip_suffix(rule).is_some_and(|p| p.ends_with('.'))
}

pub struct Blocker {
    engine: Engine,
    rules: usize,
}
impl Default for Blocker {
    fn default() -> Self {
        let domains = include_str!("../assets/blocklist.txt")
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty() && !s.starts_with('#'))
            .map(|s| format!("||{s}^"))
            .collect::<Vec<_>>()
            .join("\n");
        Self::from_lists(&[
            include_str!("../assets/filters/easylist.txt"),
            include_str!("../assets/filters/easyprivacy.txt"),
            &domains,
        ])
    }
}
impl Blocker {
    pub fn load(profile: &std::path::Path, private: bool) -> Self {
        if let Some(lists) = crate::updates::cached_lists(profile) {
            let domains = include_str!("../assets/blocklist.txt")
                .lines()
                .map(str::trim)
                .filter(|s| !s.is_empty() && !s.starts_with('#'))
                .map(|s| format!("||{s}^"))
                .collect::<Vec<_>>()
                .join("\n");
            return Self::from_lists(&[&lists.easylist, &lists.easyprivacy, &domains]);
        }
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        for source in [
            include_str!("../assets/filters/easylist.txt"),
            include_str!("../assets/filters/easyprivacy.txt"),
            include_str!("../assets/blocklist.txt"),
            "adblock-0.13.3",
        ] {
            source.hash(&mut hash);
        }
        let cache = profile.join(format!("filters-{:016x}.dat", hash.finish()));
        if std::fs::metadata(&cache).is_ok_and(|m| m.len() < 32 * 1024 * 1024) {
            if let Ok(bytes) = std::fs::read(&cache) {
                let mut engine = Engine::new_with_filter_set(FilterSet::new(false));
                if engine.deserialize(&bytes).is_ok() {
                    let rules = [
                        include_str!("../assets/filters/easylist.txt"),
                        include_str!("../assets/filters/easyprivacy.txt"),
                    ]
                    .iter()
                    .map(|list| {
                        list.lines()
                            .filter(|line| {
                                !line.is_empty() && !line.starts_with('!') && !line.starts_with('[')
                            })
                            .count()
                    })
                    .sum::<usize>()
                        + include_str!("../assets/blocklist.txt")
                            .lines()
                            .filter(|s| !s.is_empty() && !s.starts_with('#'))
                            .count();
                    return Self { engine, rules };
                }
            }
        }
        let blocker = Self::default();
        if !private {
            let tmp = cache.with_extension("tmp");
            if std::fs::write(&tmp, blocker.engine.serialize()).is_ok() {
                let _ = std::fs::rename(tmp, cache);
            }
        }
        blocker
    }
    pub fn from_lists(lists: &[&str]) -> Self {
        let mut set = FilterSet::new(false);
        let mut rules = 0;
        for list in lists {
            rules += list
                .lines()
                .filter(|line| !line.is_empty() && !line.starts_with('!') && !line.starts_with('['))
                .count();
            set.add_filter_list((*list).to_owned(), ParseOptions::default());
        }
        Self {
            engine: Engine::new_with_filter_set(set),
            rules,
        }
    }
    #[cfg(test)]
    pub fn blocks(&self, input: &str, source: &str, kind: &str) -> bool {
        self.network_request(input, source, kind, "GET")
    }
    pub fn network_request(&self, input: &str, source: &str, kind: &str, method: &str) -> bool {
        Request::new(input, source, kind, method)
            .is_ok_and(|request| self.engine.check_network_request(&request).should_block())
    }
    pub fn cosmetic_script(&self, url: &str) -> String {
        let resources = self.engine.url_cosmetic_resources(url);
        let selectors: Vec<_> = resources.hide_selectors.into_iter().collect();
        let json = serde_json::to_string(&selectors).unwrap_or_else(|_| "[]".into());
        format!(
            r#"(()=>{{const selectors={json};if(!selectors.length)return;let style=document.getElementById('__noram_shields');if(!style){{style=document.createElement('style');style.id='__noram_shields';(document.head||document.documentElement).append(style);}}for(const s of selectors){{try{{style.sheet.insertRule(s+'{{display:none!important}}',style.sheet.cssRules.length)}}catch(_e){{}}}}}})()"#
        )
    }
    pub fn len(&self) -> usize {
        self.rules
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
    let mut engine = settings.search_engine.as_str();
    let mut query = text;
    if let Some((bang, rest)) = text.split_once(' ') {
        if let Some(selected) = match bang {
            "!g" => Some("google"),
            "!b" => Some("brave"),
            "!d" => Some("duckduckgo"),
            "!w" => Some("wikipedia"),
            "!s" => Some("startpage"),
            "!bi" => Some("bing"),
            "!e" => Some("ecosia"),
            _ => None,
        } {
            engine = selected;
            query = rest.trim();
        }
    }
    let (base, key) = match engine {
        "google" => ("https://www.google.com/search", "q"),
        "brave" => ("https://search.brave.com/search", "q"),
        "bing" => ("https://www.bing.com/search", "q"),
        "startpage" => ("https://www.startpage.com/sp/search", "query"),
        "ecosia" => ("https://www.ecosia.org/search", "q"),
        "wikipedia" => ("https://en.wikipedia.org/w/index.php", "search"),
        _ => ("https://duckduckgo.com/", "q"),
    };
    let mut url = Url::parse(base).unwrap();
    url.query_pairs_mut().append_pair(key, query);
    Ok(url.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blocks_subdomains_without_overmatching() {
        let b = Blocker::from_lists(&["||doubleclick.net^"]);
        assert!(b.blocks(
            "https://a.doubleclick.net/ad.js",
            "https://example.org",
            "script"
        ));
        assert!(!b.blocks(
            "https://notdoubleclick.net/",
            "https://example.org",
            "script"
        ));
        assert!(!b.blocks(
            "https://example.org/doubleclick.net",
            "https://example.org",
            "script"
        ));
    }
    #[test]
    fn brave_engine_honors_types_and_exceptions() {
        let b = Blocker::from_lists(&["||ads.example^$script\n@@||ads.example/allowed.js$script"]);
        assert!(b.blocks(
            "https://ads.example/banner.js",
            "https://site.example",
            "script"
        ));
        assert!(!b.blocks(
            "https://ads.example/banner.png",
            "https://site.example",
            "image"
        ));
        assert!(!b.blocks(
            "https://ads.example/allowed.js",
            "https://site.example",
            "script"
        ));
    }
    #[test]
    fn search_selection_and_bang_override() {
        let mut settings = Settings {
            search_engine: "brave".into(),
            ..Settings::default()
        };
        assert_eq!(
            address("rust browser", &settings).unwrap(),
            "https://search.brave.com/search?q=rust+browser"
        );
        assert_eq!(
            address("!g a&b", &settings).unwrap(),
            "https://www.google.com/search?q=a%26b"
        );
        settings.search_engine = "wikipedia".into();
        assert_eq!(
            address("Rust language", &settings).unwrap(),
            "https://en.wikipedia.org/w/index.php?search=Rust+language"
        );
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
