use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, Default)]
pub struct CacheIndex {
    // Keys: remote_url
    pub by_url: HashMap<String, CacheEntry>,
    // Keys: provider:key (e.g., "cdse:uuid")
    pub by_alias: HashMap<String, CacheEntry>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct CacheEntry {
    pub local_path: PathBuf,
    pub etag: Option<String>,
    pub content_length: Option<u64>,
}

pub fn index_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join(".index.json")
}

pub fn load_index(cache_dir: &Path) -> CacheIndex {
    let p = index_path(cache_dir);
    match fs::read(&p) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => CacheIndex::default(),
    }
}

pub fn save_index(cache_dir: &Path, idx: &CacheIndex) {
    let p = index_path(cache_dir);
    if let Ok(bytes) = serde_json::to_vec_pretty(idx) {
        let _ = fs::write(p, bytes);
    }
}

pub fn put_url(idx: &mut CacheIndex, url: &str, entry: CacheEntry) {
    idx.by_url.insert(url.to_string(), entry);
}

pub fn put_alias(idx: &mut CacheIndex, provider: &str, key: &str, entry: CacheEntry) {
    idx.by_alias.insert(format!("{}:{}", provider, key), entry);
}

pub fn get_url<'a>(idx: &'a CacheIndex, url: &str) -> Option<&'a CacheEntry> {
    idx.by_url.get(url)
}

pub fn get_alias<'a>(idx: &'a CacheIndex, provider: &str, key: &str) -> Option<&'a CacheEntry> {
    idx.by_alias.get(&format!("{}:{}", provider, key))
}


