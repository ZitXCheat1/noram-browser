use crate::lifecycle::SharedActivity;
use std::{cell::RefCell, rc::Rc};
use webview2_com::Microsoft::Web::WebView2::Win32::*;
pub struct Request {
    pub id: u64,
    pub tab: u64,
    pub origin: String,
    pub kind: String,
    pub generation: u64,
    pub activity: SharedActivity,
    pub args: ICoreWebView2PermissionRequestedEventArgs,
    pub deferral: ICoreWebView2Deferral,
}
impl Drop for Request {
    fn drop(&mut self) {
        self.activity
            .permissions
            .set(self.activity.permissions.get().saturating_sub(1));
        unsafe {
            let _ = self.deferral.Complete();
        }
    }
}
pub type Queue = Rc<RefCell<Vec<Request>>>;
pub fn origin(uri: &str) -> Option<String> {
    let url = url::Url::parse(uri).ok()?;
    (url.scheme() == "https" || (url.scheme() == "http" && crate::policy::local_host(&url)))
        .then(|| url.origin().ascii_serialization())
}
pub fn name(kind: COREWEBVIEW2_PERMISSION_KIND) -> Option<&'static str> {
    match kind {
        COREWEBVIEW2_PERMISSION_KIND_MICROPHONE => Some("microphone"),
        COREWEBVIEW2_PERMISSION_KIND_CAMERA => Some("camera"),
        COREWEBVIEW2_PERMISSION_KIND_GEOLOCATION => Some("location"),
        COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS => Some("notifications"),
        COREWEBVIEW2_PERMISSION_KIND_CLIPBOARD_READ => Some("clipboard"),
        _ => None,
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn permissions_are_scoped_to_secure_exact_origins() {
        assert_eq!(
            super::origin("https://example.org:8443/a"),
            Some("https://example.org:8443".into())
        );
        assert_eq!(super::origin("http://example.org"), None);
        assert_eq!(super::origin("file:///a"), None);
        assert_eq!(
            super::origin("https://example.org/path"),
            Some("https://example.org".into())
        );
    }
}
