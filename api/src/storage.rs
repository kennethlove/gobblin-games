use crate::AppError;
use object_store::ObjectStoreExt; // put/get/head/delete convenience methods
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use tokio::fs;
use tokio::io::AsyncWriteExt;

/// Cloudflare image-transformation prefix, e.g.
/// `/cdn-cgi/image/width=256,format=auto`. Empty (dev/local): public URLs
/// stay plain `/uploads/...` paths.
static IMAGE_CDN_PREFIX: LazyLock<String> = LazyLock::new(|| {
    std::env::var("IMAGE_CDN_PREFIX")
        .unwrap_or_default()
        .trim_end_matches('/')
        .to_string()
});

/// File validation constraints
pub struct UploadConstraints {
    pub max_size_bytes: usize,
    pub allowed_extensions: Vec<String>,
    pub max_width: Option<u32>,
    pub max_height: Option<u32>,
}

impl Default for UploadConstraints {
    fn default() -> Self {
        Self {
            max_size_bytes: 5 * 1024 * 1024, // 5MB
            allowed_extensions: vec![
                "jpg".to_string(),
                "jpeg".to_string(),
                "png".to_string(),
                "webp".to_string(),
            ],
            max_width: Some(1024),
            max_height: Some(1024),
        }
    }
}

/// Storage backend trait - allows swapping between local filesystem and S3/MinIO
// async_trait's expansion puts a message-less #[must_use] on boxed-future
// returns, which clippy 1.99 flags as double_must_use. Not fixable from here.
#[allow(clippy::double_must_use)]
#[async_trait::async_trait]
pub trait StorageBackend: Send + Sync {
    /// Save a file and return its public URL path
    async fn save(&self, path: &str, data: &[u8]) -> Result<String, AppError>;

    /// Delete a file
    async fn delete(&self, path: &str) -> Result<(), AppError>;

    /// Check if a file exists
    async fn exists(&self, path: &str) -> Result<bool, AppError>;

    /// Read a file's bytes (remote mode: proxy reads keep the bucket private)
    async fn read(&self, path: &str) -> Result<Vec<u8>, AppError>;

    /// Get the public URL for accessing the file
    fn public_url(&self, path: &str) -> String;
}

/// Local filesystem storage implementation
pub struct LocalStorage {
    base_path: PathBuf,
    public_prefix: String,
}

impl LocalStorage {
    pub fn new(base_path: impl AsRef<Path>, public_prefix: impl Into<String>) -> Self {
        Self {
            base_path: base_path.as_ref().to_path_buf(),
            public_prefix: public_prefix.into(),
        }
    }

    /// Ensure the storage directory exists
    pub async fn init(&self) -> Result<(), AppError> {
        fs::create_dir_all(&self.base_path).await.map_err(|e| {
            AppError::InternalServerError(format!("Failed to create storage directory: {}", e))
        })
    }
}

#[async_trait::async_trait]
impl StorageBackend for LocalStorage {
    async fn save(&self, path: &str, data: &[u8]) -> Result<String, AppError> {
        let file_path = self.base_path.join(path);

        // Ensure parent directory exists
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).await.map_err(|e| {
                AppError::InternalServerError(format!("Failed to create directory: {}", e))
            })?;
        }

        // Write file
        let mut file = fs::File::create(&file_path)
            .await
            .map_err(|e| AppError::InternalServerError(format!("Failed to create file: {}", e)))?;

        file.write_all(data)
            .await
            .map_err(|e| AppError::InternalServerError(format!("Failed to write file: {}", e)))?;

        Ok(path.to_string())
    }

    async fn delete(&self, path: &str) -> Result<(), AppError> {
        let file_path = self.base_path.join(path);

        if file_path.exists() {
            fs::remove_file(&file_path).await.map_err(|e| {
                AppError::InternalServerError(format!("Failed to delete file: {}", e))
            })?;
        }

        Ok(())
    }

    async fn exists(&self, path: &str) -> Result<bool, AppError> {
        let file_path = self.base_path.join(path);
        Ok(file_path.exists())
    }

    async fn read(&self, path: &str) -> Result<Vec<u8>, AppError> {
        let file_path = self.base_path.join(path);
        match fs::read(&file_path).await {
            Ok(data) => Ok(data),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(AppError::NotFound(format!("Upload not found: {path}")))
            }
            Err(e) => Err(AppError::InternalServerError(format!(
                "Failed to read {}: {e}",
                file_path.display()
            ))),
        }
    }

    fn public_url(&self, path: &str) -> String {
        if IMAGE_CDN_PREFIX.is_empty() {
            format!("{}/{}", self.public_prefix, path)
        } else {
            format!(
                "{}{}/{}",
                IMAGE_CDN_PREFIX.as_str(),
                self.public_prefix,
                path
            )
        }
    }
}

/// Object storage for any S3-compatible API — Cloudflare R2 (the cxi
/// target), MinIO, or AWS S3. Selected with `STORAGE_BACKEND=s3`.
/// `public_url` keeps the same `/uploads/...` shape as `LocalStorage`, so
/// stored avatar rows stay valid and reads proxy through the api with the
/// bucket kept private.
pub struct S3Storage {
    store: object_store::aws::AmazonS3,
    public_prefix: String,
}

impl S3Storage {
    /// Required env: `S3_ENDPOINT`, `S3_BUCKET`, `S3_ACCESS_KEY_ID`,
    /// `S3_SECRET_ACCESS_KEY`. Optional: `S3_REGION` (default `auto` — the
    /// value Cloudflare R2 requires).
    pub fn from_env() -> Result<Self, AppError> {
        let need = |key: &str| -> Result<String, AppError> {
            std::env::var(key).map_err(|_| {
                AppError::InternalServerError(format!("{key} must be set when STORAGE_BACKEND=s3"))
            })
        };
        let endpoint = need("S3_ENDPOINT")?;
        let store = object_store::aws::AmazonS3Builder::new()
            .with_bucket_name(need("S3_BUCKET")?)
            .with_endpoint(&endpoint)
            .with_region(std::env::var("S3_REGION").unwrap_or_else(|_| "auto".to_string()))
            .with_access_key_id(need("S3_ACCESS_KEY_ID")?)
            .with_secret_access_key(need("S3_SECRET_ACCESS_KEY")?)
            .with_virtual_hosted_style_request(false)
            .with_allow_http(endpoint.starts_with("http://"))
            .build()
            .map_err(|e| AppError::InternalServerError(format!("S3 client config: {e}")))?;
        Ok(Self {
            store,
            public_prefix: "/uploads".to_string(),
        })
    }
}

#[async_trait::async_trait]
impl StorageBackend for S3Storage {
    async fn save(&self, path: &str, data: &[u8]) -> Result<String, AppError> {
        self.store
            .put(&object_store::path::Path::from(path), data.to_vec().into())
            .await
            .map_err(|e| AppError::InternalServerError(format!("S3 put {path}: {e}")))?;
        Ok(path.to_string())
    }

    async fn delete(&self, path: &str) -> Result<(), AppError> {
        match self
            .store
            .delete(&object_store::path::Path::from(path))
            .await
        {
            Ok(()) => Ok(()),
            Err(object_store::Error::NotFound { .. }) => Ok(()),
            Err(e) => Err(AppError::InternalServerError(format!(
                "S3 delete {path}: {e}"
            ))),
        }
    }

    async fn exists(&self, path: &str) -> Result<bool, AppError> {
        match self.store.head(&object_store::path::Path::from(path)).await {
            Ok(_) => Ok(true),
            Err(object_store::Error::NotFound { .. }) => Ok(false),
            Err(e) => Err(AppError::InternalServerError(format!(
                "S3 head {path}: {e}"
            ))),
        }
    }

    async fn read(&self, path: &str) -> Result<Vec<u8>, AppError> {
        let result = self
            .store
            .get(&object_store::path::Path::from(path))
            .await
            .map_err(|e| match e {
                object_store::Error::NotFound { .. } => {
                    AppError::NotFound(format!("Upload not found: {path}"))
                }
                other => AppError::InternalServerError(format!("S3 get {path}: {other}")),
            })?;
        result
            .bytes()
            .await
            .map_err(|e| AppError::InternalServerError(format!("S3 read {path}: {e}")))
            .map(|bytes| bytes.to_vec())
    }

    fn public_url(&self, path: &str) -> String {
        if IMAGE_CDN_PREFIX.is_empty() {
            format!("{}/{}", self.public_prefix, path)
        } else {
            format!(
                "{}{}/{}",
                IMAGE_CDN_PREFIX.as_str(),
                self.public_prefix,
                path
            )
        }
    }
}

/// Validate uploaded file data
pub fn validate_upload(
    data: &[u8],
    filename: &str,
    constraints: &UploadConstraints,
) -> Result<(), AppError> {
    // Check size
    if data.len() > constraints.max_size_bytes {
        return Err(AppError::ValidationError(format!(
            "File size {} bytes exceeds maximum {} bytes",
            data.len(),
            constraints.max_size_bytes
        )));
    }

    // Check extension
    let ext = Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .ok_or_else(|| AppError::ValidationError("File has no extension".to_string()))?;

    if !constraints.allowed_extensions.contains(&ext) {
        return Err(AppError::ValidationError(format!(
            "File extension '{}' not allowed. Allowed: {}",
            ext,
            constraints.allowed_extensions.join(", ")
        )));
    }

    // Basic image format validation (magic bytes)
    validate_image_format(data, &ext)?;

    Ok(())
}

/// Validate image file format by checking magic bytes
fn validate_image_format(data: &[u8], expected_ext: &str) -> Result<(), AppError> {
    if data.len() < 12 {
        return Err(AppError::ValidationError(
            "File too small to be valid image".to_string(),
        ));
    }

    let is_valid = match expected_ext {
        "jpg" | "jpeg" => data.starts_with(&[0xFF, 0xD8, 0xFF]),
        "png" => data.starts_with(&[0x89, 0x50, 0x4E, 0x47]),
        "webp" => data.starts_with(b"RIFF") && data[8..12] == *b"WEBP",
        _ => {
            return Err(AppError::ValidationError(format!(
                "Unknown image format: {}",
                expected_ext
            )));
        }
    };

    if !is_valid {
        return Err(AppError::ValidationError(format!(
            "File content does not match {} format",
            expected_ext
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_jpeg_magic_bytes() {
        let jpeg_data = vec![
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01,
        ];
        assert!(validate_image_format(&jpeg_data, "jpg").is_ok());
    }

    #[test]
    fn test_validate_png_magic_bytes() {
        let png_data = vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D,
        ];
        assert!(validate_image_format(&png_data, "png").is_ok());
    }

    #[test]
    fn test_reject_invalid_magic_bytes() {
        let invalid_data = vec![
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        assert!(validate_image_format(&invalid_data, "jpg").is_err());
    }

    #[test]
    fn test_validate_upload_size() {
        let constraints = UploadConstraints {
            max_size_bytes: 100,
            ..Default::default()
        };
        let large_data = vec![0xFF; 200];
        assert!(validate_upload(&large_data, "test.jpg", &constraints).is_err());
    }

    #[test]
    fn test_validate_upload_extension() {
        let constraints = UploadConstraints::default();
        let data = vec![0xFF; 50];
        assert!(validate_upload(&data, "test.exe", &constraints).is_err());
    }
}
