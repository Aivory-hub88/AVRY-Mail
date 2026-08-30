use anyhow::Result;
use async_trait::async_trait;
use std::path::PathBuf;
use tokio::fs;
use tracing::info;

#[async_trait]
pub trait ObjectStore: Send + Sync {
    async fn put(&self, key: &str, data: Vec<u8>, content_type: &str) -> Result<String>;
    async fn get(&self, key: &str) -> Result<Vec<u8>>;
    async fn delete(&self, key: &str) -> Result<()>;
    async fn exists(&self, key: &str) -> Result<bool>;
}

// ── Local filesystem (VPS fallback) ──
#[derive(Clone)]
pub struct LocalStore { base: PathBuf }

impl LocalStore {
    pub fn new(base: impl Into<PathBuf>) -> Self { Self { base: base.into() } }
}

#[async_trait]
impl ObjectStore for LocalStore {
    async fn put(&self, key: &str, data: Vec<u8>, _ct: &str) -> Result<String> {
        let path = self.base.join(key);
        if let Some(parent) = path.parent() { fs::create_dir_all(parent).await?; }
        fs::write(&path, data).await?;
        info!("local store put {}", key);
        Ok(key.to_string())
    }
    async fn get(&self, key: &str) -> Result<Vec<u8>> {
        Ok(fs::read(self.base.join(key)).await?)
    }
    async fn delete(&self, key: &str) -> Result<()> {
        fs::remove_file(self.base.join(key)).await?; Ok(())
    }
    async fn exists(&self, key: &str) -> Result<bool> {
        Ok(self.base.join(key).exists())
    }
}

// ── S3 / R2 (same API) ──
#[cfg(feature = "s3")]
pub struct S3Store {
    bucket: String,
    client: aws_sdk_s3::Client,
}

#[cfg(feature = "s3")]
impl S3Store {
    pub async fn new(bucket: String) -> Result<Self> {
        let config = aws_config::load_from_env().await;
        let client = aws_sdk_s3::Client::new(&config);
        Ok(Self { bucket, client })
    }
}

#[cfg(feature = "s3")]
#[async_trait]
impl ObjectStore for S3Store {
    async fn put(&self, key: &str, data: Vec<u8>, content_type: &str) -> Result<String> {
        self.client.put_object()
            .bucket(&self.bucket).key(key)
            .body(data.into()).content_type(content_type).send().await?;
        Ok(key.to_string())
    }
    async fn get(&self, key: &str) -> Result<Vec<u8>> {
        let out = self.client.get_object().bucket(&self.bucket).key(key).send().await?;
        let bytes = out.body.collect().await?.into_bytes().to_vec();
        Ok(bytes)
    }
    async fn delete(&self, key: &str) -> Result<()> {
        self.client.delete_object().bucket(&self.bucket).key(key).send().await?; Ok(())
    }
    async fn exists(&self, key: &str) -> Result<bool> {
        Ok(self.client.head_object().bucket(&self.bucket).key(key).send().await.is_ok())
    }
}

#[cfg(not(feature = "s3"))]
pub struct S3Store;
#[cfg(not(feature = "s3"))]
#[async_trait]
impl ObjectStore for S3Store {
    async fn put(&self, _k: &str, _d: Vec<u8>, _ct: &str) -> Result<String> { anyhow::bail!("s3 feature not enabled") }
    async fn get(&self, _k: &str) -> Result<Vec<u8>> { anyhow::bail!("s3 feature not enabled") }
    async fn delete(&self, _k: &str) -> Result<()> { anyhow::bail!("s3 feature not enabled") }
    async fn exists(&self, _k: &str) -> Result<bool> { anyhow::bail!("s3 feature not enabled") }
}
