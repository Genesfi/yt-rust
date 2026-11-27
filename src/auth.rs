use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

const SERVICE: &str = "yt-rust";
const ACCOUNT_KEY: &str = "google_session";
const FALLBACK_FILE: &str = "session.json";
const ACCOUNTS_ORIGIN: &str = "https://accounts.youtube.com";

#[derive(Debug)]
pub struct RevokedError;

impl std::fmt::Display for RevokedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Google session revoked or expired")
    }
}

impl std::error::Error for RevokedError {}

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

    /// Merge `Set-Cookie` headers from Google/YouTube responses into a fresh session copy.
    /// Returns `Some(updated)` only when at least one relevant cookie changed or was added.
    pub fn updated<'a>(&self, set_cookies: impl IntoIterator<Item = &'a str>) -> Option<Self> {
        let mut jar: Vec<(String, String)> = self
            .cookie
            .split(';')
            .filter_map(|kv| {
                let (k, v) = kv.trim().split_once('=')?;
                Some((k.trim().to_owned(), v.trim().to_owned()))
            })
            .collect();
        let mut changed = false;

        for line in set_cookies {
            let mut parts = line.split(';');
            let Some((name, value)) = parts
                .next()
                .and_then(|kv| kv.trim().split_once('='))
                .map(|(k, v)| (k.trim(), v.trim()))
            else {
                continue;
            };
            if name.is_empty() || value.is_empty() {
                continue;
            }
            let attr = |key: &str| {
                parts.clone().find_map(|a| {
                    a.trim()
                        .split_once('=')
                        .filter(|(k, _)| k.trim().eq_ignore_ascii_case(key))
                        .map(|(_, v)| v.trim())
                })
            };
            let deleted = attr("max-age").is_some_and(|a| a.parse::<i64>().is_ok_and(|n| n <= 0));
            if deleted {
                continue;
            }
            let domain_ok = match attr("domain") {
                None => true,
                Some(d) => {
                    let d = d.trim_start_matches('.').to_ascii_lowercase();
                    d == "youtube.com"
                        || d.ends_with(".youtube.com")
                        || d == "google.com"
                        || d.ends_with(".google.com")
                }
            };
            if !domain_ok {
                continue;
            }

            match jar.iter_mut().find(|(k, _)| k == name) {
                Some((_, existing)) if existing == value => {}
                Some((_, existing)) => {
                    *existing = value.to_owned();
                    changed = true;
                }
                None => {
                    jar.push((name.to_owned(), value.to_owned()));
                    changed = true;
                }
            }
        }

        if !changed {
            return None;
        }
        let cookie = jar
            .into_iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("; ");
        Some(Self {
            cookie,
            ..self.clone()
        })
    }

    /// Ask Google's RotateCookies endpoint for refreshed session cookies (`__Secure-1PSIDTS`, `SIDCC`, ...).
    pub async fn rotate(&self, http: &reqwest::Client) -> anyhow::Result<Vec<String>> {
        let resp = http
            .post(format!("{ACCOUNTS_ORIGIN}/RotateCookies"))
            .header("User-Agent", crate::innertube::WEB_UA)
            .header("Origin", ACCOUNTS_ORIGIN)
            .header("Referer", format!("{ACCOUNTS_ORIGIN}/"))
            .header("Content-Type", "application/json")
            .header("Cookie", &self.cookie)
            .header("X-Goog-AuthUser", self.session_index.to_string())
            .body(r#"[000,"-0000000000000000000"]"#)
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(RevokedError.into());
        }
        let resp = resp.error_for_status()?;
        Ok(resp
            .headers()
            .get_all("set-cookie")
            .iter()
            .filter_map(|v| v.to_str().ok().map(str::to_owned))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::Session;

    #[test]
    fn updates_session_cookies_safely() {
        let mut s = Session::from_cookie("SAPISID=abc; __Secure-1PSIDTS=old; OTHER=1".to_owned());
        s.account_name = Some("Tester".to_owned());

        // Unchanged cookies or foreign domains return None
        assert!(
            s.updated([
                "__Secure-1PSIDTS=old; Domain=.youtube.com; Path=/",
                "FOREIGN=xyz; Domain=example.com; Path=/",
                "SIDCC=gone; Domain=.youtube.com; Max-Age=0",
            ])
            .is_none()
        );

        let rotated = s
            .updated([
                "__Secure-1PSIDTS=new_ts; Domain=.youtube.com; Path=/; Secure",
                "SIDCC=fresh_sidcc; Domain=.youtube.com; Path=/",
            ])
            .unwrap();
        assert_eq!(rotated.account_name.as_deref(), Some("Tester"));
        assert!(rotated.cookie.contains("__Secure-1PSIDTS=new_ts"));
        assert!(rotated.cookie.contains("SIDCC=fresh_sidcc"));
        assert!(rotated.cookie.contains("SAPISID=abc"));
    }
}
