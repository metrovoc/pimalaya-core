use async_trait::async_trait;
use tracing::info;

use super::{Folders, ListFolders};
use crate::{gmail::GmailContext, AnyResult};

#[derive(Clone, Debug)]
pub struct ListGmailFolders {
    ctx: GmailContext,
}

impl ListGmailFolders {
    pub fn new(ctx: &GmailContext) -> Self {
        Self { ctx: ctx.clone() }
    }

    pub fn new_boxed(ctx: &GmailContext) -> Box<dyn ListFolders> {
        Box::new(Self::new(ctx))
    }

    pub fn some_new_boxed(ctx: &GmailContext) -> Option<Box<dyn ListFolders>> {
        Some(Self::new_boxed(ctx))
    }
}

#[async_trait]
impl ListFolders for ListGmailFolders {
    async fn list_folders(&self) -> AnyResult<Folders> {
        info!("listing gmail folders");

        let labels = self.ctx.list_labels().await?;
        let folders = labels
            .iter()
            .filter_map(|label| self.ctx.folder_from_label(label))
            .collect();

        Ok(folders)
    }
}
