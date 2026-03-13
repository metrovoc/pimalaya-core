use async_trait::async_trait;
use tracing::info;

use super::PurgeFolder;
use crate::{gmail::GmailContext, AnyResult};

#[derive(Clone, Debug)]
pub struct PurgeGmailFolder {
    ctx: GmailContext,
}

impl PurgeGmailFolder {
    pub fn new(ctx: &GmailContext) -> Self {
        Self { ctx: ctx.clone() }
    }

    pub fn new_boxed(ctx: &GmailContext) -> Box<dyn PurgeFolder> {
        Box::new(Self::new(ctx))
    }

    pub fn some_new_boxed(ctx: &GmailContext) -> Option<Box<dyn PurgeFolder>> {
        Some(Self::new_boxed(ctx))
    }
}

#[async_trait]
impl PurgeFolder for PurgeGmailFolder {
    async fn purge_folder(&self, folder: &str) -> AnyResult<()> {
        info!("purging gmail folder {folder}");

        let label_id = self.ctx.resolve_label_id(folder).await?;
        let ids = self.ctx.list_all_message_ids(&label_id, None).await?;

        for id in ids {
            self.ctx.delete_message_permanently(&id).await?;
        }

        Ok(())
    }
}
