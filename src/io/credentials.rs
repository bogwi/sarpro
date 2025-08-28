use std::collections::HashMap;
use std::time::{Duration, Instant};
use std::path::PathBuf;
use gdal::config::set_config_option;

#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
    #[error("missing environment variable: {0}")] 
    MissingEnv(&'static str),
    #[error("http error: {0}")]
    Http(String),
}

pub trait CredentialProvider {
    fn prepare(&mut self) -> Result<(), CredentialError>;
    fn headers_for(&self, _url: &str) -> Result<HashMap<String, String>, CredentialError> { Ok(HashMap::new()) }
    fn cookiejar_paths(&self) -> Option<(PathBuf, PathBuf)> { None }
}

pub struct CdseProvider {
    client_id: Option<String>,
    client_secret: Option<String>,
    token: Option<String>,
    expires_at: Option<Instant>,
    http_timeout: Duration,
}

impl CdseProvider {
    pub fn new(http_timeout_s: u64) -> Self {
        Self {
            client_id: std::env::var("CDSE_CLIENT_ID").ok(),
            client_secret: std::env::var("CDSE_CLIENT_SECRET").ok(),
            token: None,
            expires_at: None,
            http_timeout: Duration::from_secs(http_timeout_s),
        }
    }

    fn needs_refresh(&self) -> bool {
        match (self.token.as_ref(), self.expires_at) {
            (Some(_), Some(t)) => Instant::now() + Duration::from_secs(30) >= t,
            _ => true,
        }
    }
}

impl CredentialProvider for CdseProvider {
    fn prepare(&mut self) -> Result<(), CredentialError> {
        if !self.needs_refresh() { return Ok(()); }
        let client_id = self.client_id.clone().ok_or(CredentialError::MissingEnv("CDSE_CLIENT_ID"))?;
        let client_secret = self.client_secret.clone().ok_or(CredentialError::MissingEnv("CDSE_CLIENT_SECRET"))?;

        let client = reqwest::blocking::Client::builder()
            .timeout(self.http_timeout)
            .build()
            .map_err(|e| CredentialError::Http(format!("client: {}", e)))?;

        let form = [
            ("grant_type", "client_credentials"),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
        ];
        let token_url = "https://identity.dataspace.copernicus.eu/auth/realms/CDSE/protocol/openid-connect/token";
        let resp = client
            .post(token_url)
            .form(&form)
            .send()
            .map_err(|e| CredentialError::Http(format!("send: {}", e)))?;
        if !resp.status().is_success() {
            return Err(CredentialError::Http(format!("token status {}", resp.status())));
        }
        let value: serde_json::Value = resp.json().map_err(|e| CredentialError::Http(format!("json: {}", e)))?;
        let access = value.get("access_token").and_then(|v| v.as_str()).ok_or_else(|| CredentialError::Http("missing access_token".to_string()))?;
        let expires = value.get("expires_in").and_then(|v| v.as_u64()).unwrap_or(300);
        self.token = Some(access.to_string());
        self.expires_at = Some(Instant::now() + Duration::from_secs(expires));
        Ok(())
    }

    fn headers_for(&self, _url: &str) -> Result<HashMap<String, String>, CredentialError> {
        let mut h = HashMap::new();
        if let Some(ref t) = self.token {
            h.insert("Authorization".to_string(), format!("Bearer {}", t));
        }
        Ok(h)
    }
}

pub struct AsfProvider {
    pub netrc_path: PathBuf,
    pub cookie_file: PathBuf,
    pub cookie_jar: PathBuf,
    http_timeout: Duration,
}

impl AsfProvider {
    pub fn new(http_timeout_s: u64) -> Self {
        // Resolve netrc path precedence: SARPRO_NETRC env → repo-local → ~/.netrc
        let netrc_env = std::env::var("SARPRO_NETRC").ok();
        let repo_netrc = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".netrc");
        let home_netrc = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")) .join(".netrc");
        let netrc_path = if let Some(p) = netrc_env {
            PathBuf::from(p)
        } else if repo_netrc.exists() {
            repo_netrc
        } else {
            home_netrc
        };

        // Default cookie storage under ~/.sarpro
        let cookie_root = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/"))
            .join(".sarpro");
        let _ = std::fs::create_dir_all(&cookie_root);
        let cookie_file = cookie_root.join("asf_cookies.txt");
        let cookie_jar = cookie_root.join("asf_cookies.txt");

        Self {
            netrc_path,
            cookie_file,
            cookie_jar,
            http_timeout: Duration::from_secs(http_timeout_s),
        }
    }

    fn ensure_netrc_perms(&self) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(&self.netrc_path) {
                let mode = meta.permissions().mode() & 0o777;
                if mode != 0o600 {
                    let _ = std::fs::set_permissions(&self.netrc_path, std::fs::Permissions::from_mode(0o600));
                }
            }
        }
    }
}

impl CredentialProvider for AsfProvider {
    fn prepare(&mut self) -> Result<(), CredentialError> {
        // Validate netrc presence
        if !self.netrc_path.exists() {
            return Err(CredentialError::Http(format!(
                "ASF: .netrc not found at {:?}. Set SARPRO_NETRC or create sarpro/.netrc",
                self.netrc_path
            )));
        }
        self.ensure_netrc_perms();
        // Ensure cookie files exist
        if let Some(parent) = self.cookie_file.parent() { let _ = std::fs::create_dir_all(parent); }
        if !self.cookie_file.exists() { let _ = std::fs::File::create(&self.cookie_file); }
        if let Some(parent) = self.cookie_jar.parent() { let _ = std::fs::create_dir_all(parent); }
        if !self.cookie_jar.exists() { let _ = std::fs::File::create(&self.cookie_jar); }

        // Apply GDAL/CPL configuration for cookie handling, netrc and timeouts
        let _ = set_config_option("GDAL_HTTP_COOKIEFILE", &self.cookie_file.to_string_lossy());
        let _ = set_config_option("GDAL_HTTP_COOKIEJAR", &self.cookie_jar.to_string_lossy());
        let _ = set_config_option("GDAL_HTTP_TIMEOUT", &format!("{}", self.http_timeout.as_secs()));
        let _ = set_config_option("CPL_VSIL_CURL_USE_CACHE", "YES");
        let _ = set_config_option("GDAL_HTTP_NETRC", "YES");
        let _ = set_config_option("GDAL_HTTP_NETRC_FILE", &self.netrc_path.to_string_lossy());
        Ok(())
    }

    fn headers_for(&self, _url: &str) -> Result<HashMap<String, String>, CredentialError> {
        // Primary path uses .netrc + cookie jar via GDAL/curl. No extra headers needed.
        Ok(HashMap::new())
    }

    fn cookiejar_paths(&self) -> Option<(PathBuf, PathBuf)> {
        Some((self.cookie_file.clone(), self.cookie_jar.clone()))
    }
}

