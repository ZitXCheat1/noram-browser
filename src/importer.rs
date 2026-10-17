use crate::Entry;
use anyhow::{bail, Context, Result};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashSet,
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const LIMIT: usize = 5000;
#[derive(Debug, Clone, Serialize)]
pub struct Source {
    pub name: String,
    pub path: PathBuf,
    pub firefox: bool,
}
#[derive(Clone, Default, Deserialize)]
pub struct Options {
    pub bookmarks: bool,
    pub history: bool,
    pub cookies: bool,
}
#[derive(Clone)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: i32,
    pub expires: Option<f64>,
}
#[derive(Clone, Default)]
pub struct Data {
    pub bookmarks: Vec<Entry>,
    pub history: Vec<Entry>,
    pub cookies: Vec<Cookie>,
    pub skipped: usize,
}
// Cookie values must never enter diagnostics or UI payloads.
impl std::fmt::Debug for Data {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImportData")
            .field("summary", &self.summary())
            .finish()
    }
}
impl Data {
    pub fn summary(&self) -> Value {
        serde_json::json!({"bookmarks":self.bookmarks.len(),"history":self.history.len(),"cookies":self.cookies.len(),"skipped":self.skipped})
    }
}
fn read(path: &Path) -> Result<String> {
    let mut s = String::new();
    std::fs::File::open(path)?
        .take(16 * 1024 * 1024 + 1)
        .read_to_string(&mut s)?;
    anyhow::ensure!(s.len() <= 16 * 1024 * 1024, "File exceeds 16 MB");
    if s.starts_with('\u{feff}') {
        s.remove(0);
    }
    Ok(s)
}
fn entry(title: &str, url: &str) -> Option<Entry> {
    let u = url::Url::parse(url).ok()?;
    if !matches!(u.scheme(), "https" | "http")
        || !u.username().is_empty()
        || u.password().is_some()
        || url.len() > 8192
    {
        return None;
    }
    Some(Entry {
        title: title.chars().take(256).collect(),
        url: u.to_string(),
    })
}
fn push(entries: &mut Vec<Entry>, title: &str, url: &str) {
    if entries.len() < LIMIT {
        if let Some(e) = entry(title, url) {
            if !entries.iter().any(|old| old.url == e.url) {
                entries.push(e)
            }
        }
    }
}
pub fn discover() -> Vec<Source> {
    let mut out = Vec::new();
    if let Some(root) = std::env::var_os("LOCALAPPDATA") {
        for (name, dir) in [
            ("Brave", "BraveSoftware/Brave-Browser/User Data"),
            ("Chrome", "Google/Chrome/User Data"),
            ("Edge", "Microsoft/Edge/User Data"),
            ("Vivaldi", "Vivaldi/User Data"),
            ("Chromium", "Chromium/User Data"),
        ] {
            if let Ok(items) = std::fs::read_dir(PathBuf::from(&root).join(dir)) {
                for item in items.flatten() {
                    let path = item.path();
                    let profile = item.file_name().to_string_lossy().into_owned();
                    if (profile == "Default" || profile.starts_with("Profile "))
                        && (path.join("Bookmarks").exists() || path.join("History").exists())
                    {
                        out.push(Source {
                            name: format!("{name} — {profile}"),
                            path,
                            firefox: false,
                        });
                    }
                }
            }
        }
    }
    if let Some(root) = std::env::var_os("APPDATA") {
        let root = PathBuf::from(root);
        if let Ok(items) = std::fs::read_dir(root.join("Mozilla/Firefox/Profiles")) {
            for item in items.flatten() {
                let path = item.path();
                if path.join("places.sqlite").exists() {
                    out.push(Source {
                        name: format!("Firefox — {}", item.file_name().to_string_lossy()),
                        path,
                        firefox: true,
                    });
                }
            }
        }
        for (name, dir) in [
            ("Opera", "Opera Software/Opera Stable"),
            ("Opera GX", "Opera Software/Opera GX Stable"),
        ] {
            let path = root.join(dir);
            if path.join("Bookmarks").exists() {
                out.push(Source {
                    name: name.into(),
                    path,
                    firefox: false,
                });
            }
        }
    }
    out
}
fn database(path: &Path) -> Result<Connection> {
    let db = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .context("Cannot read browser database; close the source browser and retry")?;
    db.busy_timeout(Duration::from_millis(500))?;
    Ok(db)
}
fn bookmarks_json(v: &Value, out: &mut Vec<Entry>) {
    if let Some(url) = v["url"].as_str() {
        push(out, v["name"].as_str().unwrap_or(url), url)
    }
    if let Some(items) = v["children"].as_array() {
        for item in items {
            bookmarks_json(item, out)
        }
    }
    if let Some(roots) = v["roots"].as_object() {
        for root in roots.values() {
            bookmarks_json(root, out)
        }
    }
}
pub fn profile(path: &Path, options: &Options) -> Result<Data> {
    anyhow::ensure!(path.is_dir(), "Select a browser profile folder");
    let firefox = path.join("places.sqlite").exists();
    let mut data = Data::default();
    anyhow::ensure!(
        options.bookmarks || options.history || options.cookies,
        "Select at least one type of data"
    );
    if options.cookies && !firefox {
        bail!("Brave, Chrome and Edge cookies require an exported cookie file. Their encrypted cookie databases cannot be copied directly.")
    }
    if options.bookmarks {
        if firefox {
            let db = database(&path.join("places.sqlite"))?;
            let mut q=db.prepare("SELECT COALESCE(b.title,p.title,''),p.url FROM moz_bookmarks b JOIN moz_places p ON p.id=b.fk WHERE b.type=1 LIMIT 5000")?;
            for row in q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
                let (t, u) = row?;
                push(&mut data.bookmarks, &t, &u)
            }
        } else {
            bookmarks_json(
                &serde_json::from_str::<Value>(&read(&path.join("Bookmarks"))?)?,
                &mut data.bookmarks,
            )
        }
    }
    if options.history {
        let db = database(&path.join(if firefox { "places.sqlite" } else { "History" }))?;
        let sql = if firefox {
            "SELECT COALESCE(title,''),url FROM moz_places WHERE visit_count>0 ORDER BY last_visit_date DESC LIMIT 2000"
        } else {
            "SELECT COALESCE(title,''),url FROM urls ORDER BY last_visit_time DESC LIMIT 2000"
        };
        let mut q = db.prepare(sql)?;
        for row in q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (t, u) = row?;
            push(&mut data.history, &t, &u)
        }
        data.history.reverse();
    }
    if options.cookies {
        let db = database(&path.join("cookies.sqlite"))?;
        let mut q=db.prepare("SELECT name,value,host,path,isSecure,isHttpOnly,expiry,sameSite,originAttributes FROM moz_cookies LIMIT 5001")?;
        for row in q.query_map([], |r| {
            Ok((
                Cookie {
                    name: r.get(0)?,
                    value: r.get(1)?,
                    domain: r.get(2)?,
                    path: r.get(3)?,
                    secure: r.get(4)?,
                    http_only: r.get(5)?,
                    expires: Some(r.get::<_, i64>(6)? as f64),
                    same_site: r.get(7)?,
                },
                r.get::<_, String>(8)?,
            ))
        })? {
            let (c, attrs) = row?;
            if attrs.is_empty() {
                add_cookie(&mut data, c)
            } else {
                data.skipped += 1
            }
        }
    }
    Ok(data)
}
fn add_cookie(data: &mut Data, c: Cookie) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    let host = c.domain.trim_start_matches('.');
    let valid_host = url::Host::parse(host).is_ok()
        && !host.is_empty()
        && !host.contains(['/', ':', '@', ' ', '\\']);
    let valid = valid_host
        && c.domain.len() < 254
        && c.path.starts_with('/')
        && !c.path.chars().any(char::is_control)
        && !c.name.is_empty()
        && !c
            .name
            .chars()
            .any(|c| c.is_control() || "()<>@,;:\\\"/[]?={} ".contains(c))
        && !c.value.chars().any(char::is_control)
        && c.value.len() < 4096
        && c.expires.is_none_or(|e| e.is_finite() && e > now)
        && (!c.name.starts_with("__Secure-") || c.secure)
        && (!c.name.starts_with("__Host-")
            || (c.secure && c.path == "/" && !c.domain.starts_with('.')))
        && (c.same_site != 0 || c.secure);
    if valid && data.cookies.len() < LIMIT {
        if let Some(i) = data
            .cookies
            .iter()
            .position(|v| v.name == c.name && v.domain == c.domain && v.path == c.path)
        {
            data.cookies[i] = c
        } else {
            data.cookies.push(c)
        }
    } else {
        data.skipped += 1
    }
}
pub fn file(path: &Path, kind: &str) -> Result<Data> {
    let text = read(path)?;
    let mut data = Data::default();
    match kind {
        "bookmarks" => {
            if text.trim_start().starts_with('{') {
                bookmarks_json(&serde_json::from_str::<Value>(&text)?, &mut data.bookmarks)
            } else {
                let re = regex::Regex::new(
                    r#"(?is)<a\b[^>]*\bhref\s*=\s*(?:"([^"]*)"|'([^']*)')[^>]*>(.*?)</a\s*>"#,
                )?;
                let tags = regex::Regex::new("<[^>]*>")?;
                for cap in re.captures_iter(&text) {
                    let url = html_escape::decode_html_entities(
                        cap.get(1).or_else(|| cap.get(2)).unwrap().as_str(),
                    );
                    let raw = tags.replace_all(&cap[3], "");
                    let title = html_escape::decode_html_entities(&raw);
                    push(&mut data.bookmarks, &title, &url)
                }
            }
        }
        "cookies" => {
            if text.trim_start().starts_with(['[', '{']) {
                let v: Value = serde_json::from_str(&text)?;
                let list = v
                    .as_array()
                    .or_else(|| v["cookies"].as_array())
                    .context("Expected an exported cookies array")?;
                anyhow::ensure!(list.len() <= LIMIT, "Cookie file exceeds 5000 entries");
                for v in list {
                    if v.get("partitionKey").is_some_and(|p| !p.is_null())
                        || v["partitioned"].as_bool() == Some(true)
                    {
                        data.skipped += 1;
                        continue;
                    }
                    let Some(name) = v["name"].as_str() else {
                        data.skipped += 1;
                        continue;
                    };
                    let Some(value) = v["value"].as_str() else {
                        data.skipped += 1;
                        continue;
                    };
                    let same_site = match v["sameSite"].as_str().unwrap_or("unspecified") {
                        "strict" => 2,
                        "no_restriction" | "none" => 0,
                        _ => 1,
                    };
                    let expires = if v["session"].as_bool() == Some(true) {
                        None
                    } else {
                        v["expirationDate"]
                            .as_f64()
                            .or_else(|| v["expires"].as_f64())
                    };
                    add_cookie(
                        &mut data,
                        Cookie {
                            name: name.into(),
                            value: value.into(),
                            domain: v["domain"].as_str().unwrap_or("").into(),
                            path: v["path"].as_str().unwrap_or("/").into(),
                            secure: v["secure"].as_bool().unwrap_or(false),
                            http_only: v["httpOnly"].as_bool().unwrap_or(false),
                            same_site,
                            expires,
                        },
                    );
                }
            } else {
                for line in text.lines() {
                    let (line, http_only) = if let Some(l) = line.strip_prefix("#HttpOnly_") {
                        (l, true)
                    } else {
                        if line.starts_with('#') || line.trim().is_empty() {
                            continue;
                        }
                        (line, false)
                    };
                    let cols: Vec<_> = line.splitn(7, '\t').collect();
                    if cols.len() != 7 {
                        data.skipped += 1;
                        continue;
                    }
                    let Ok(expiry) = cols[4].parse::<f64>() else {
                        data.skipped += 1;
                        continue;
                    };
                    let mut domain = cols[0].to_string();
                    if cols[1] == "TRUE" && !domain.starts_with('.') {
                        domain.insert(0, '.')
                    }
                    add_cookie(
                        &mut data,
                        Cookie {
                            name: cols[5].into(),
                            value: cols[6].into(),
                            domain,
                            path: cols[2].into(),
                            secure: cols[3] == "TRUE",
                            http_only,
                            same_site: 1,
                            expires: if expiry == 0. { None } else { Some(expiry) },
                        },
                    )
                }
            }
        }
        _ => bail!("Unknown import format"),
    };
    Ok(data)
}
pub fn merge(target: &mut Vec<Entry>, incoming: Vec<Entry>) -> usize {
    let mut urls: HashSet<String> = target.iter().map(|e| e.url.clone()).collect();
    let before = target.len();
    for e in incoming {
        if urls.insert(e.url.clone()) {
            target.push(e)
        }
    }
    target.len() - before
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unsafe_cookie_and_redacts_debug() {
        let mut d = Data::default();
        let c = Cookie {
            name: "session".into(),
            value: "SENSITIVE".into(),
            domain: "example.com".into(),
            path: "/".into(),
            secure: true,
            http_only: true,
            same_site: 1,
            expires: None,
        };
        add_cookie(&mut d, c.clone());
        assert_eq!(d.cookies.len(), 1);
        assert!(!format!("{d:?}").contains("SENSITIVE"));
        let mut bad = c;
        bad.domain = "example.com/path".into();
        add_cookie(&mut d, bad);
        assert_eq!(d.skipped, 1)
    }
    #[test]
    fn urls_and_duplicates() {
        assert!(entry("x", "javascript:alert(1)").is_none());
        assert!(entry("x", "https://user:pass@example.com").is_none());
        let mut out = vec![];
        push(&mut out, "a", "https://example.com");
        push(&mut out, "b", "https://example.com/");
        assert_eq!(out.len(), 1);
        assert_eq!(
            merge(&mut out, vec![entry("c", "https://example.com").unwrap()]),
            0
        );
    }
}

#[cfg(test)]
mod fixture_tests {
    use super::*;
    #[test]
    fn profile_and_export_formats_preserve_source() {
        let root = std::env::temp_dir().join(format!(
            "noram-import-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let bookmarks = r#"{"roots":{"bookmark_bar":{"children":[{"name":"A","url":"https://example.com/"},{"name":"Unsafe","url":"javascript:alert(1)"}]}}}"#;
        std::fs::write(root.join("Bookmarks"), bookmarks).unwrap();
        let db = Connection::open(root.join("History")).unwrap();
        db.execute_batch("CREATE TABLE urls(title TEXT,url TEXT,last_visit_time INTEGER); INSERT INTO urls VALUES('Old','https://old.example/',1),('New','https://new.example/',2);").unwrap();
        drop(db);
        let before = std::fs::read(root.join("History")).unwrap();
        let data = profile(
            &root,
            &Options {
                bookmarks: true,
                history: true,
                cookies: false,
            },
        )
        .unwrap();
        assert_eq!(data.bookmarks.len(), 1);
        assert_eq!(data.history.len(), 2);
        assert_eq!(data.history[0].title, "Old");
        assert_eq!(std::fs::read(root.join("History")).unwrap(), before);
        assert!(profile(
            &root,
            &Options {
                cookies: true,
                ..Default::default()
            }
        )
        .is_err());
        let html = root.join("bookmarks.html");
        std::fs::write(&html,"<A HREF='https://example.com/?a=1&amp;b=2'>Rock &amp; Roll</A><A HREF=\"javascript:bad\">bad</A>").unwrap();
        let data = file(&html, "bookmarks").unwrap();
        assert_eq!(data.bookmarks.len(), 1);
        assert_eq!(data.bookmarks[0].title, "Rock & Roll");
        let cookie_file = root.join("cookies.json");
        std::fs::write(&cookie_file,r#"{"cookies":[{"name":"session","value":"synthetic","domain":"example.com","path":"/","secure":true,"httpOnly":true,"session":true,"sameSite":"strict"},{"name":"expired","value":"x","domain":"example.com","expirationDate":1},{"name":"partition","value":"x","domain":"example.com","partitionKey":{}},{"name":"bad;name","value":"x","domain":"example.com"}]}"#).unwrap();
        let data = file(&cookie_file, "cookies").unwrap();
        assert_eq!(data.cookies.len(), 1);
        assert_eq!(data.skipped, 3);
        assert!(data.cookies[0].http_only);
        assert_eq!(data.cookies[0].same_site, 2);
        assert!(data.cookies[0].expires.is_none());
        std::fs::write(&cookie_file,"# Netscape HTTP Cookie File\n#HttpOnly_.example.com\tTRUE\t/\tTRUE\t0\tsid\tsynthetic\n").unwrap();
        let data = file(&cookie_file, "cookies").unwrap();
        assert_eq!(data.cookies.len(), 1);
        assert!(data.cookies[0].http_only);
        assert!(data.cookies[0].expires.is_none());
        let db = Connection::open(root.join("places.sqlite")).unwrap();
        db.execute_batch("CREATE TABLE moz_places(id INTEGER,title TEXT,url TEXT,visit_count INTEGER,last_visit_date INTEGER);CREATE TABLE moz_bookmarks(title TEXT,fk INTEGER,type INTEGER);INSERT INTO moz_places VALUES(1,'Page','https://firefox.example/',1,10);INSERT INTO moz_bookmarks VALUES('Bookmark',1,1);").unwrap();
        drop(db);
        let db = Connection::open(root.join("cookies.sqlite")).unwrap();
        db.execute_batch("CREATE TABLE moz_cookies(name TEXT,value TEXT,host TEXT,path TEXT,isSecure INTEGER,isHttpOnly INTEGER,expiry INTEGER,sameSite INTEGER,originAttributes TEXT); INSERT INTO moz_cookies VALUES('sid','synthetic','example.com','/',1,1,4102444800,2,''),('container','synthetic','example.com','/',1,1,4102444800,2,'^userContextId=1');").unwrap();
        drop(db);
        let before = std::fs::read(root.join("cookies.sqlite")).unwrap();
        let data = profile(
            &root,
            &Options {
                bookmarks: true,
                history: true,
                cookies: true,
            },
        )
        .unwrap();
        assert_eq!(data.bookmarks.len(), 1);
        assert_eq!(data.history.len(), 1);
        assert_eq!(data.cookies.len(), 1);
        assert_eq!(data.skipped, 1);
        assert_eq!(std::fs::read(root.join("cookies.sqlite")).unwrap(), before);
        std::fs::remove_dir_all(root).unwrap();
    }
}
