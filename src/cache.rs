use std::path::PathBuf;
use std::fs::{create_dir_all};
use std::error::Error;
use sha2::{Sha256, Digest};
use dirs;

const CACHE_DIR: &str = ".rnp/cache";

pub struct PackageCache {
    cache_dir: PathBuf,
}

impl PackageCache {
    pub fn new() -> Result<Self, Box<dyn Error + Send + Sync>> {
        let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
        let cache_dir = home_dir.join(CACHE_DIR);
        
        // Create cache directory if it doesn't exist
        if !cache_dir.exists() {
            create_dir_all(&cache_dir)?;
        }
        
        Ok(Self { cache_dir })
    }

    // Generate a cache key for a package
    pub fn cache_key(package_name: &str, version: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(format!("{}@{}", package_name, version));
        let result = hasher.finalize();
        format!("{:x}", result)
    }

    // Get the path to a cached tarball
    pub fn tarball_path(&self, package_name: &str, version: &str) -> PathBuf {
        let key = Self::cache_key(package_name, version);
        self.cache_dir.join(format!("{}.tgz", key))
    }

    // Save tarball data to cache
    pub fn save_tarball(&self, package_name: &str, version: &str, data: &[u8]) -> Result<(), Box<dyn Error + Send + Sync>> {
        let path = self.tarball_path(package_name, version);
        std::fs::write(path, data)?;
        Ok(())
    }

    // Read tarball data from cache
    pub fn get_tarball(&self, package_name: &str, version: &str) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
        let path = self.tarball_path(package_name, version);
        Ok(std::fs::read(path)?)
    }
}
