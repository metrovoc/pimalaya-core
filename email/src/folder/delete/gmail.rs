use async_trait::async_trait;
use tracing::info;

use super::DeleteFolder;
use crate::{gmail::GmailContext, AnyResult};

#[derive(Clone, Debug)]
pub struct DeleteGmailFolder {
    ctx: GmailContext,
}

impl DeleteGmailFolder {
    pub fn new(ctx: &GmailContext) -> Self {
        Self { ctx: ctx.clone() }
    }

    pub fn new_boxed(ctx: &GmailContext) -> Box<dyn DeleteFolder> {
        Box::new(Self::new(ctx))
    }

    pub fn some_new_boxed(ctx: &GmailContext) -> Option<Box<dyn DeleteFolder>> {
        Some(Self::new_boxed(ctx))
    }
}

#[async_trait]
impl DeleteFolder for DeleteGmailFolder {
    async fn delete_folder(&self, folder: &str) -> AnyResult<()> {
        info!("deleting gmail label {folder}");

        let label_id = self.ctx.resolve_label_id(folder).await?;
        let path = format!("users/me/labels/{label_id}");
        self.ctx.api_delete(&path).await?;

        Ok(())
    }
}
