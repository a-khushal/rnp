use sha1::{Digest as Sha1Digest, Sha1};
use sha2::Sha256;
use std::error::Error;
use std::fs::create_dir_all;
use std::path::PathBuf;
use std::time::Duration;

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
    pub fn save_tarball(
        &self,
        package_name: &str,
        version: &str,
        data: &[u8],
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let path = self.tarball_path(package_name, version);
        std::fs::write(path, data)?;
        Ok(())
    }

    // Try to read a cache entry that is still fresh and valid
    pub fn get_valid_tarball(
        &self,
        package_name: &str,
        version: &str,
        expected_sha1: Option<&str>,
        max_age: Duration,
    ) -> Result<Option<Vec<u8>>, Box<dyn Error + Send + Sync>> {
        let path = self.tarball_path(package_name, version);
        if !path.exists() {
            return Ok(None);
        }

        if !Self::is_fresh(&path, max_age)? {
            self.invalidate_tarball(package_name, version)?;
            return Ok(None);
        }

        let data = std::fs::read(&path)?;
        if let Some(expected) = expected_sha1
            && !Self::verify_sha1_checksum(&data, expected)
        {
            self.invalidate_tarball(package_name, version)?;
            return Ok(None);
        }

        Ok(Some(data))
    }

    // Remove a cached tarball if it exists
    pub fn invalidate_tarball(
        &self,
        package_name: &str,
        version: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let path = self.tarball_path(package_name, version);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    pub fn verify_sha1_checksum(data: &[u8], expected_sha1: &str) -> bool {
        let mut hasher = Sha1::new();
        hasher.update(data);
        let actual = format!("{:x}", hasher.finalize());
        actual.eq_ignore_ascii_case(expected_sha1)
    }

    fn is_fresh(
        path: &std::path::Path,
        max_age: Duration,
    ) -> Result<bool, Box<dyn Error + Send + Sync>> {
        let modified = std::fs::metadata(path)?.modified()?;
        let age = modified.elapsed()?;
        Ok(age <= max_age)
    }
}
