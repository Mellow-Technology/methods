//! Object storage for uploads: any S3-compatible service, or local disk.

use std::sync::Arc;

use bytes::Bytes;
use object_store::{ObjectStore, aws::AmazonS3Builder, local::LocalFileSystem, path::Path};

use crate::config::StorageConfig;

#[derive(Clone)]
pub struct Storage {
    store: Arc<dyn ObjectStore>,
    pub description: String,
}

impl Storage {
    pub fn from_config(config: &StorageConfig) -> anyhow::Result<Self> {
        match config {
            StorageConfig::Local { root } => {
                std::fs::create_dir_all(root)?;
                Ok(Storage {
                    store: Arc::new(LocalFileSystem::new_with_prefix(root)?),
                    description: format!("local disk at {root}"),
                })
            }
            StorageConfig::S3 {
                bucket,
                region,
                endpoint,
                access_key_id,
                secret_access_key,
                allow_http,
                virtual_hosted,
            } => {
                let mut builder = AmazonS3Builder::from_env()
                    .with_bucket_name(bucket)
                    .with_region(region)
                    .with_allow_http(*allow_http)
                    .with_virtual_hosted_style_request(*virtual_hosted);
                if let Some(endpoint) = endpoint {
                    builder = builder.with_endpoint(endpoint);
                }
                if let Some(key) = access_key_id {
                    builder = builder.with_access_key_id(key);
                }
                if let Some(secret) = secret_access_key {
                    builder = builder.with_secret_access_key(secret);
                }
                Ok(Storage {
                    store: Arc::new(builder.build()?),
                    description: format!(
                        "S3 bucket {bucket}{}",
                        endpoint.as_ref().map(|e| format!(" at {e}")).unwrap_or_default()
                    ),
                })
            }
        }
    }

    pub async fn put(&self, key: &str, data: Bytes) -> anyhow::Result<()> {
        self.store.put(&Path::from(key), data.into()).await?;
        Ok(())
    }

    pub async fn get(&self, key: &str) -> anyhow::Result<Bytes> {
        Ok(self.store.get(&Path::from(key)).await?.bytes().await?)
    }

    pub async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.store.delete(&Path::from(key)).await?;
        Ok(())
    }
}
