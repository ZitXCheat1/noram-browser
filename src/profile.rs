use crate::{Entry, Saved};
use anyhow::{Context, Result};
use std::path::Path;
const LIMIT: u64 = 32 * 1024 * 1024;
fn read(path: &Path) -> Result<Saved> {
    anyhow::ensure!(
        std::fs::metadata(path)?.len() <= LIMIT,
        "Profile exceeds 32 MB"
    );
    serde_json::from_slice(&std::fs::read(path)?).context("Invalid profile JSON")
}
pub fn write_backup(path: &Path, saved: &Saved) -> Result<()> {
    let envelope = serde_json::json!({"format":"noram-profile","version":1,"data":saved});
    std::fs::write(path, serde_json::to_vec_pretty(&envelope)?)?;
    Ok(())
}
pub fn load(dir: &Path, read_only: bool) -> Result<(Saved, Option<String>)> {
    let path = dir.join("browser.json");
    match read(&path) {
        Ok(saved) => Ok((saved, None)),
        Err(_) if !path.exists() => Ok((Saved::default(), None)),
        Err(error) => {
            let saved = read(&dir.join("browser.json.bak")).context(format!(
                "{error}. No valid backup; preserve browser.json and use a different --profile"
            ))?;
            if !read_only {
                preserve(&path)?;
            }
            Ok((saved, Some("Recovered preferences and bookmarks from the last valid backup. The damaged profile was preserved.".into())))
        }
    }
}
fn preserve(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    std::fs::copy(
        path,
        path.with_file_name(format!("browser-preserved-{stamp}.json")),
    )?;
    Ok(())
}
pub fn commit(dir: &Path, bytes: &[u8]) -> Result<()> {
    serde_json::from_slice::<Saved>(bytes)?;
    let path = dir.join("browser.json");
    if read(&path).is_ok() {
        let backup = dir.join("browser.json.bak.tmp");
        std::fs::copy(&path, &backup)?;
        std::fs::rename(backup, dir.join("browser.json.bak"))?;
    }
    let tmp = dir.join("browser.json.tmp");
    let mut file = std::fs::File::create(&tmp)?;
    use std::io::Write;
    file.write_all(bytes)?;
    file.sync_all()?;
    std::fs::rename(tmp, path)?;
    Ok(())
}
pub fn stage_restore(dir: &Path, source: &Path) -> Result<()> {
    anyhow::ensure!(
        std::fs::metadata(source)?.len() <= LIMIT,
        "Backup exceeds 32 MB"
    );
    let envelope: serde_json::Value = serde_json::from_slice(&std::fs::read(source)?)?;
    anyhow::ensure!(
        envelope["format"] == "noram-profile" && envelope["version"] == 1,
        "Choose a NoRAM profile backup created by Settings"
    );
    let saved: Saved = serde_json::from_value(envelope["data"].clone())?;
    std::fs::write(
        dir.join("browser.restore.json"),
        serde_json::to_vec_pretty(&saved)?,
    )?;
    Ok(())
}
pub fn apply_restore(dir: &Path) -> Result<()> {
    let staged = dir.join("browser.restore.json");
    if staged.exists() {
        let saved = read(&staged)?;
        preserve(&dir.join("browser.json"))?;
        commit(dir, &serde_json::to_vec_pretty(&saved)?)?;
        std::fs::remove_file(staged)?;
    }
    Ok(())
}
pub fn export_bookmarks(path: &Path, entries: &[Entry]) -> Result<()> {
    let mut html = "<!DOCTYPE NETSCAPE-Bookmark-file-1>\n<META HTTP-EQUIV=\"Content-Type\" CONTENT=\"text/html; charset=UTF-8\">\n<TITLE>Bookmarks</TITLE>\n<H1>NoRAM Bookmarks</H1>\n<DL><p>\n".to_owned();
    for e in entries {
        if url::Url::parse(&e.url).is_ok_and(|u| matches!(u.scheme(), "http" | "https")) {
            html.push_str(&format!(
                "<DT><A HREF=\"{}\">{}</A>\n",
                html_escape::encode_double_quoted_attribute(&e.url),
                html_escape::encode_text(&e.title)
            ));
        }
    }
    html.push_str("</DL><p>\n");
    std::fs::write(path, html)?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recovery_preserves_corrupt_file_and_last_valid_backup() {
        let dir = std::env::temp_dir().join(format!("noram-profile-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut saved = Saved::default();
        saved.bookmarks.push(Entry {
            title: "<&>".into(),
            url: "https://example.org/?a=\"x\"".into(),
        });
        let bytes = serde_json::to_vec(&saved).unwrap();
        commit(&dir, &bytes).unwrap();
        commit(&dir, &bytes).unwrap();
        std::fs::write(dir.join("browser.json"), b"broken").unwrap();
        assert_eq!(load(&dir, false).unwrap().0.bookmarks.len(), 1);
        assert_eq!(std::fs::read(dir.join("browser.json")).unwrap(), b"broken");
        export_bookmarks(&dir.join("export.html"), &saved.bookmarks).unwrap();
        let html = std::fs::read_to_string(dir.join("export.html")).unwrap();
        assert!(html.contains("&lt;&amp;&gt;"));
        assert!(!html.contains("a=\"x\""));
        let count = std::fs::read_dir(&dir).unwrap().count();
        assert_eq!(load(&dir, true).unwrap().0.bookmarks.len(), 1);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), count);
        write_backup(&dir.join("backup.json"), &saved).unwrap();
        stage_restore(&dir, &dir.join("backup.json")).unwrap();
        apply_restore(&dir).unwrap();
        assert_eq!(load(&dir, false).unwrap().0.bookmarks.len(), 1);
        std::fs::write(dir.join("foreign.json"), b"{}").unwrap();
        assert!(stage_restore(&dir, &dir.join("foreign.json")).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
