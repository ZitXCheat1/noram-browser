//! Conservative, event-driven tab protection. A failed probe never authorizes unloading.
use serde_json::Value;
use std::{cell::Cell, rc::Rc};
#[derive(Default)]
pub struct Activity {
    pub audio: Cell<bool>,
    pub downloads: Cell<usize>,
    pub capture: Cell<bool>,
    pub permissions: Cell<usize>,
    pub probing: Cell<bool>,
    pub generation: Cell<u64>,
}
impl Activity {
    pub fn protected(&self) -> bool {
        self.audio.get()
            || self.downloads.get() > 0
            || self.capture.get()
            || self.permissions.get() > 0
    }
}
pub type SharedActivity = Rc<Activity>;
// Protect editable documents, embedded documents we cannot inspect, and media.
// Websites get no command bridge; only this fixed script's result is read natively.
pub const PROBE: &str = r#"(()=>({safe:!document.querySelector('input:not([type=hidden]):not([type=button]):not([type=submit]),textarea,select,[contenteditable]:not([contenteditable=false]),iframe,frame,audio,video')&&!document.fullscreenElement,x:scrollX,y:scrollY,url:location.href}))()"#;
pub fn snapshot(raw: &str, expected_url: &str) -> Option<(f64, f64)> {
    let v: Value = serde_json::from_str(raw).ok()?;
    if v["safe"].as_bool() != Some(true) || v["url"].as_str() != Some(expected_url) {
        return None;
    }
    let (x, y) = (v["x"].as_f64()?, v["y"].as_f64()?);
    (x.is_finite() && y.is_finite() && x >= 0. && y >= 0.).then_some((x, y))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsafe_or_stale_probes_cannot_discard_work() {
        for raw in [
            "null",
            "{}",
            r#"{"safe":false,"url":"https://a/","x":0,"y":0}"#,
            r#"{"safe":true,"url":"https://b/","x":0,"y":0}"#,
        ] {
            assert!(snapshot(raw, "https://a/").is_none());
        }
        assert_eq!(
            snapshot(
                r#"{"safe":true,"url":"https://a/","x":0,"y":120}"#,
                "https://a/"
            ),
            Some((0., 120.))
        );
    }
}
