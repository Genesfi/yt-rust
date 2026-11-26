use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

const SERVICE: &str = "yt-rust";
const ACCOUNT_KEY: &str = "google_session";
const FALLBACK_FILE: &str = "session.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    pub cookie: String,
    #[serde(default)]
    pub session_index: u32,
    #[serde(default)]
    pub visitor_data: Option<String>,
    #[serde(default)]
    pub account_name: Option<String>,
    #[serde(default)]
    pub account_avatar: Option<String>,
}

impl Session {
    pub fn from_cookie(cookie: String) -> Self {
        Self {
            cookie,
            session_index: 0,
            visitor_data: None,
            account_name: None,
            account_avatar: None,
        }
    }

    pub fn load(data_dir: &Path) -> Option<Self> {
        if let Ok(entry) = keyring::Entry::new(SERVICE, ACCOUNT_KEY) {
            if let Ok(json) = entry.get_password() {
                if let Ok(s) = serde_json::from_str::<Self>(&json) {
                    return Some(s);
                }
            }
        }
        let path = data_dir.join(FALLBACK_FILE);
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
    }

    pub fn save(&self, data_dir: &Path) {
        if let Ok(json) = serde_json::to_string(self) {
            if let Ok(entry) = keyring::Entry::new(SERVICE, ACCOUNT_KEY) {
                if entry.set_password(&json).is_ok() {
                    let _ = std::fs::remove_file(data_dir.join(FALLBACK_FILE));
                    return;
                }
            }
            let _ = std::fs::create_dir_all(data_dir);
            let _ = std::fs::write(data_dir.join(FALLBACK_FILE), json);
        }
    }

    pub fn clear(data_dir: &Path) {
        if let Ok(entry) = keyring::Entry::new(SERVICE, ACCOUNT_KEY) {
            let _ = entry.delete_credential();
        }
        let _ = std::fs::remove_file(data_dir.join(FALLBACK_FILE));
    }

    fn cookie_value(&self, name: &str) -> Option<&str> {
        self.cookie
            .split(';')
            .find_map(|kv| kv.trim().strip_prefix(name)?.strip_prefix('='))
    }

    pub fn authorization(&self, origin: &str) -> Option<String> {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
        let parts: Vec<String> = [
            ("SAPISIDHASH", "SAPISID"),
            ("SAPISID1PHASH", "__Secure-1PAPISID"),
            ("SAPISID3PHASH", "__Secure-3PAPISID"),
        ]
        .iter()
        .filter_map(|(scheme, cookie)| {
            let sid = self.cookie_value(cookie)?;
            let hash = Sha1::digest(format!("{ts} {sid} {origin}"));
            Some(format!("{scheme} {ts}_{hash:x}"))
        })
        .collect();
        (!parts.is_empty()).then(|| parts.join(" "))
    }

    pub fn sign(&self, req: reqwest::RequestBuilder, origin: &str) -> reqwest::RequestBuilder {
        let mut r = req
            .header("Cookie", &self.cookie)
            .header("X-Goog-AuthUser", self.session_index.to_string());
        if let Some(auth) = self.authorization(origin) {
            r = r.header("Authorization", auth);
        }
        r
    }
}
