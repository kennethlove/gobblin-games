use crate::AppError;
use dicebear_core::{Avatar, Style};
use object_store::ObjectStoreExt; // put/get/head/delete convenience methods
use serde_json::json;
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

/// DiceBear renders in-process (user instruction: Rust library, not the
/// HTTP API) — no service to host, no outbound call per avatar. Style
/// definitions embed at compile time (one cargo feature per style);
/// parse once, render per call.
static PLAYER_STYLE: LazyLock<Style> = LazyLock::new(|| {
    Style::from_str(dicebear_styles::AVATAAARS_NEUTRAL)
        .expect("embedded avataaars-neutral style parses")
});
static GOBLIN_STYLE: LazyLock<Style> = LazyLock::new(|| {
    Style::from_str(dicebear_styles::CRITTERS).expect("embedded critters style parses")
});

// avataaars-neutral presets (verified against the hosted service earlier):
// "Electric" on light themes, "Cool" on dark. idRandomization keeps ids
// from repeating when seeds collide (DiceBear docs).
const AVATARAARS_LIGHT: &str = "ff2e88,00e5ff,7cff00,ffe600,ff6a00,b400ff";
const AVATARAARS_DARK: &str = "3b82f6,06b6d4,8b5cf6,38bdf8,6366f1,0ea5e9";

/// Player avatar: avataaars-neutral, Electric (light) / Cool (dark).
/// Seed = one-way hash of the email; raw addresses never reach the
/// markup. Returns a data URI (locally rendered, percent-encoded SVG).
pub fn player_avatar_url(email: &str, dark: bool) -> String {
    let seed = blake3::hash(email.as_bytes()).to_hex().to_string();
    let background: Vec<&str> = if dark {
        AVATARAARS_DARK
    } else {
        AVATARAARS_LIGHT
    }
    .split(',')
    .collect();
    let options = json!({
        "seed": seed,
        "idRandomization": true,
        "mouthVariant": [
            "concerned", "default", "disbelief", "grimace", "sad",
            "screamOpen", "serious", "smile", "twinkle",
        ],
        "backgroundColor": background,
    });
    Avatar::new(&PLAYER_STYLE, options)
        .expect("fixed avataaars options render")
        .to_data_uri()
}

/// Critters goblin avatar, stable per character (theme-neutral).
/// Animation on (seed picks one of the three speeds), bounded variant
/// sets, and body/accent/ink palettes — lighter blues, yellow->browns,
/// muted reds, several greens, with accent/ink = the same palette slightly
/// darker (user spec, 2026-11-06). The goblin's title rides on the HTML
/// `<img>` attributes.
pub fn goblin_avatar_url(identifier: &str) -> String {
    let options = json!({
        "seed": identifier,
        "idRandomization": true,
        "tags": ["animation"],
        "animationVariant": ["slow", "medium", "fast"],
        "topVariant": ["earsDroop", "earsPointy", "horns", "hornsIn", "hornsSmall"],
        "bodyVariant": [
            "bell", "blob", "dome", "lean", "peak", "round", "squat", "tilt", "tower",
        ],
        "eyesVariant": [
            "angry", "bigPupils", "close", "dots", "inward", "round",
            "sideeye", "sleepy", "uneven", "wide",
        ],
        "bodyColor": [
            "8ecdf0", "aeddf7", "c3e6fb", "ecc75e", "d4a03c", "ad7c33", "c98372",
            "b5705f", "a06868", "a8cf82", "8bbf68", "72ac5c", "5f9a6a",
        ],
        "accentColor": [
            "74a8c5", "8fb5cb", "a0bdce", "c2a34d", "ae8331", "8e662a",
            "a56b5d", "945c4e", "835555", "8aaa6b", "729d55", "5d8d4b", "4e7e57",
        ],
        "inkColor": [
            "74a8c5", "8fb5cb", "a0bdce", "c2a34d", "ae8331", "8e662a",
            "a56b5d", "945c4e", "835555", "8aaa6b", "729d55", "5d8d4b", "4e7e57",
        ],
    });
    Avatar::new(&GOBLIN_STYLE, options)
        .expect("fixed critters options render")
        .to_data_uri()
}

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

    /// Background fill of the avatar's canvas rect (percent-encoded SVG).
    fn bg(svg: &str) -> &str {
        const NEEDLE: &str = "width%3D%22112%22%20height%3D%22112%22%20fill%3D%22%23";
        let i = svg.find(NEEDLE).expect("canvas rect present") + NEEDLE.len();
        let rest = &svg[i..];
        &rest[..rest.find("%22").expect("color closes")]
    }

    /// Component variant chosen for this seed: the element id up to the
    /// per-render random suffix (e.g. `mouth-sad` — renders differ only
    /// in that suffix, so compare picks, not whole strings).
    fn pick<'a>(svg: &'a str, component: &'a str) -> &'a str {
        let needle = format!("{component}-");
        let i = svg.find(&needle).expect("component id present") + needle.len();
        let end = svg[i..].find('-').expect("variant suffix");
        &svg[i - component.len() - 1..i + end]
    }

    #[test]
    fn test_player_avatar_url_hides_email_and_varies_by_theme() {
        let light = player_avatar_url("player@example.com", false);
        let dark = player_avatar_url("player@example.com", true);
        assert!(light.starts_with("data:image/svg+xml;charset=utf-8,"));
        assert!(!light.contains("player@example.com"));
        // Same email, same seed: same feature picks; only the theme
        // preset (background) changes.
        assert_eq!(
            pick(&light, "mouth"),
            pick(&player_avatar_url("player@example.com", false), "mouth")
        );
        assert_ne!(bg(&light), bg(&dark));
    }

    #[test]
    fn test_goblin_avatar_url_seeded_by_identifier() {
        let avatar = goblin_avatar_url("gob-42");
        assert!(avatar.starts_with("data:image/svg+xml;charset=utf-8,"));
        // Animated critters render the reduced-motion hook.
        assert!(avatar.contains("prefers-reduced-motion"));
        // Same identifier, same goblin: variant picks are seed-driven.
        assert_eq!(
            pick(&avatar, "body"),
            pick(&goblin_avatar_url("gob-42"), "body")
        );
    }
}
