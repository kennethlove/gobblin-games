//! The [`Commentator`] trait — the seam between the broadcast package and
//! whatever produces narration for it.

use async_trait::async_trait;
use futures::stream::Stream;
use std::pin::Pin;

use crate::types::{BroadcastPackage, CommentaryError, CommentaryLine, CommentarySegment};

/// A commentator turns a [`BroadcastPackage`] into spoken commentary lines.
///
/// Implementations must be `Send + Sync` so they can be shared across
/// async API handlers via `Arc<dyn Commentator>`. The default
/// implementation is [`crate::Chronicler`]: deterministic template
/// narration with no network or model dependencies.
// async_trait's expansion puts a message-less #[must_use] on the boxed-future
// return, which clippy 1.99 flags as double_must_use. Not fixable from here.
#[allow(clippy::double_must_use)]
#[async_trait]
pub trait Commentator: Send + Sync {
    /// Generate a commentary segment for one phase.
    async fn generate(
        &self,
        package: &BroadcastPackage,
    ) -> Result<CommentarySegment, CommentaryError>;

    /// Stream commentary lines progressively.
    fn generate_stream(
        &self,
        package: &BroadcastPackage,
    ) -> Pin<Box<dyn Stream<Item = Result<CommentaryLine, CommentaryError>> + Send>>;
}
