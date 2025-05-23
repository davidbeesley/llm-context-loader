use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Metadata associated with each cache entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMetadata {
    pub timestamp: DateTime<Utc>,
    pub model_version: String,
    pub prompt_version: String,
    pub compression: Option<CompressionInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionInfo {
    pub algorithm: String,
    pub original_size: usize,
    pub compressed_size: usize,
}

/// Cache entry for source code summaries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceCacheEntry {
    pub file_path: String,
    pub content_hash: String,
    pub summary: String,
    pub metadata: CacheMetadata,
}

/// Cache entry for documentation (content-addressed)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocCacheEntry {
    pub content_hash: String, // Primary key - hash of the normalized content
    pub summary: String,
    pub metadata: CacheMetadata,
    // Store source info for debugging/reference
    pub source_url: Option<String>,     // Where this doc came from
    pub source_crate: Option<String>,   // If we can determine it
    pub source_version: Option<String>, // If we can determine it
}

/// Main cache manager
pub struct CacheManager {
    db_path: String,
}

impl CacheManager {
    /// Create a new cache manager
    pub fn new(cache_dir: impl AsRef<Path>) -> Result<Self> {
        let db_path = cache_dir
            .as_ref()
            .join("llm_context_cache.db")
            .to_string_lossy()
            .to_string();

        Ok(Self { db_path })
    }

    /// Initialize the database and create tables if needed
    pub fn initialize(&self) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = std::path::Path::new(&self.db_path).parent() {
            std::fs::create_dir_all(parent).context("Failed to create cache directory")?;
        }

        let conn = Connection::open(&self.db_path).context("Failed to open database connection")?;

        // Enable foreign keys
        conn.execute("PRAGMA foreign_keys = ON", [])
            .context("Failed to enable foreign keys")?;

        // Create source cache table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS source_cache (
                file_path TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                summary TEXT NOT NULL,
                metadata TEXT NOT NULL,
                created_at TEXT NOT NULL,
                accessed_at TEXT NOT NULL,
                access_count INTEGER DEFAULT 1,
                compressed BOOLEAN DEFAULT FALSE,
                PRIMARY KEY (file_path, content_hash)
            )",
            [],
        )
        .context("Failed to create source_cache table")?;

        // Create indices for source cache
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_source_content_hash ON source_cache(content_hash)",
            [],
        )
        .context("Failed to create source cache index")?;

        // Create documentation cache table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS doc_cache (
                content_hash TEXT PRIMARY KEY,
                summary TEXT NOT NULL,
                metadata TEXT NOT NULL,
                source_url TEXT,
                source_crate TEXT,
                source_version TEXT,
                created_at TEXT NOT NULL,
                accessed_at TEXT NOT NULL,
                access_count INTEGER DEFAULT 1,
                compressed BOOLEAN DEFAULT FALSE
            )",
            [],
        )
        .context("Failed to create doc_cache table")?;

        // Create index for doc cache lookups
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_doc_source ON doc_cache(source_crate, source_version)",
            [],
        )
        .context("Failed to create doc cache index")?;

        Ok(())
    }

    /// Calculate SHA256 hash of content
    pub fn hash_content(content: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Insert or update a source code cache entry
    pub fn insert_source_cache(
        &self,
        file_path: &str,
        content: &str,
        summary: &str,
        model_version: &str,
        prompt_version: &str,
    ) -> Result<String> {
        let content_hash = Self::hash_content(content);
        // TODO: Implement database insertion
        Ok(content_hash)
    }

    /// Look up a source cache entry by file path and content hash
    pub fn lookup_source_cache(
        &self,
        file_path: &str,
        content_hash: &str,
    ) -> Result<Option<SourceCacheEntry>> {
        // TODO: Implement database lookup
        Ok(None)
    }

    /// Insert or update a documentation cache entry (content-addressed)
    pub fn insert_doc_cache(
        &self,
        normalized_content: &str,
        summary: &str,
        model_version: &str,
        prompt_version: &str,
        source_url: Option<&str>,
        source_crate: Option<&str>,
        source_version: Option<&str>,
    ) -> Result<String> {
        let content_hash = Self::hash_content(normalized_content);
        // TODO: Implement database insertion
        Ok(content_hash)
    }

    /// Look up documentation cache by content hash
    pub fn lookup_doc_cache(&self, content_hash: &str) -> Result<Option<DocCacheEntry>> {
        // TODO: Implement database lookup
        Ok(None)
    }

    /// Check if we have a cached summary for this content
    pub fn has_doc_summary(&self, normalized_content: &str) -> Result<bool> {
        let content_hash = Self::hash_content(normalized_content);
        Ok(self.lookup_doc_cache(&content_hash)?.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_hash_content() {
        let content = "Hello, world!";
        let hash = CacheManager::hash_content(content);
        assert_eq!(hash.len(), 64); // SHA256 produces 64 hex characters

        // Same content should produce same hash
        let hash2 = CacheManager::hash_content(content);
        assert_eq!(hash, hash2);

        // Different content should produce different hash
        let hash3 = CacheManager::hash_content("Different content");
        assert_ne!(hash, hash3);
    }

    #[test]
    fn test_database_initialization() {
        let temp_dir = TempDir::new().unwrap();
        let cache = CacheManager::new(temp_dir.path()).unwrap();

        // Should initialize without error
        cache.initialize().unwrap();

        // Should be idempotent - can initialize again
        cache.initialize().unwrap();

        // Database file should exist
        let db_path = temp_dir.path().join("llm_context_cache.db");
        assert!(db_path.exists());
    }

    #[test]
    fn test_hash_edge_cases() {
        // Empty string
        let hash_empty = CacheManager::hash_content("");
        assert_eq!(hash_empty.len(), 64);

        // Very long string
        let long_content = "a".repeat(1_000_000);
        let hash_long = CacheManager::hash_content(&long_content);
        assert_eq!(hash_long.len(), 64);

        // Unicode content
        let unicode_content = "Hello 世界 🌍 Здравствуй мир";
        let hash_unicode = CacheManager::hash_content(unicode_content);
        assert_eq!(hash_unicode.len(), 64);

        // Whitespace differences matter
        let hash1 = CacheManager::hash_content("hello world");
        let hash2 = CacheManager::hash_content("hello  world");
        assert_ne!(hash1, hash2);

        // Newline differences matter
        let hash3 = CacheManager::hash_content("line1\nline2");
        let hash4 = CacheManager::hash_content("line1\r\nline2");
        assert_ne!(hash3, hash4);
    }

    #[test]
    fn test_database_schema_verification() {
        let temp_dir = TempDir::new().unwrap();
        let cache = CacheManager::new(temp_dir.path()).unwrap();
        cache.initialize().unwrap();

        // Verify tables exist with correct schema
        let conn = Connection::open(temp_dir.path().join("llm_context_cache.db")).unwrap();

        // Check source_cache table
        let mut stmt = conn
            .prepare("SELECT sql FROM sqlite_master WHERE type='table' AND name='source_cache'")
            .unwrap();
        let sql: String = stmt.query_row([], |row| row.get(0)).unwrap();
        assert!(sql.contains("file_path TEXT NOT NULL"));
        assert!(sql.contains("content_hash TEXT NOT NULL"));
        assert!(sql.contains("summary TEXT NOT NULL"));
        assert!(sql.contains("PRIMARY KEY (file_path, content_hash)"));

        // Check doc_cache table
        let mut stmt = conn
            .prepare("SELECT sql FROM sqlite_master WHERE type='table' AND name='doc_cache'")
            .unwrap();
        let sql: String = stmt.query_row([], |row| row.get(0)).unwrap();
        assert!(sql.contains("content_hash TEXT PRIMARY KEY"));
        assert!(sql.contains("summary TEXT NOT NULL"));

        // Verify indices exist
        let mut stmt = conn.prepare("SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_source_content_hash'").unwrap();
        let count: i64 = stmt.query_row([], |row| row.get(0)).unwrap();
        assert_eq!(count, 1);

        let mut stmt = conn
            .prepare(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_doc_source'",
            )
            .unwrap();
        let count: i64 = stmt.query_row([], |row| row.get(0)).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_cache_directory_creation() {
        let temp_dir = TempDir::new().unwrap();
        let nested_path = temp_dir.path().join("nested").join("cache").join("dir");

        // Directory doesn't exist yet
        assert!(!nested_path.exists());

        // CacheManager should handle non-existent directories gracefully
        let cache = CacheManager::new(&nested_path).unwrap();
        cache.initialize().unwrap();

        // Database should be created, which means parent directories were created
        assert!(nested_path.join("llm_context_cache.db").exists());
    }

    #[test]
    fn test_known_hash_values() {
        // Test against known SHA256 values to ensure correctness
        let hash = CacheManager::hash_content("The quick brown fox jumps over the lazy dog");
        assert_eq!(
            hash,
            "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592"
        );

        let hash = CacheManager::hash_content("Hello, World!");
        assert_eq!(
            hash,
            "dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f"
        );
    }

    #[test]
    fn test_utf8_byte_consistency() {
        // Ensure we're hashing bytes, not characters
        let content1 = "café"; // 4 characters, 5 bytes (é is 2 bytes in UTF-8)
        let content2 = "cafe"; // 4 characters, 4 bytes

        let hash1 = CacheManager::hash_content(content1);
        let hash2 = CacheManager::hash_content(content2);
        assert_ne!(hash1, hash2);

        // Verify that the same UTF-8 string always produces the same hash
        let emoji_content = "🦀 Rust 🚀";
        let hash3 = CacheManager::hash_content(emoji_content);
        let hash4 = CacheManager::hash_content(emoji_content);
        assert_eq!(hash3, hash4);

        // Test that byte-identical strings produce same hash
        let byte_test = "Hello\x00World"; // Null byte in middle
        let hash5 = CacheManager::hash_content(byte_test);
        let hash6 = CacheManager::hash_content(byte_test);
        assert_eq!(hash5, hash6);
    }
}
