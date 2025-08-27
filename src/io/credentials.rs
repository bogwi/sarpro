use std::collections::HashMap;
use std::time::{Duration, Instant};

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


