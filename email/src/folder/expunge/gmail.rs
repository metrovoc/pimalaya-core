use async_trait::async_trait;
use tracing::info;

use super::ExpungeFolder;
use crate::{gmail::GmailContext, AnyResult};

#[derive(Clone, Debug)]
pub struct ExpungeGmailFolder {
    _ctx: GmailContext,
}

impl ExpungeGmailFolder {
    pub fn new(ctx: &GmailContext) -> Self {
        Self { _ctx: ctx.clone() }
    }

    pub fn new_boxed(ctx: &GmailContext) -> Box<dyn ExpungeFolder> {
        Box::new(Self::new(ctx))
    }

    pub fn some_new_boxed(ctx: &GmailContext) -> Option<Box<dyn ExpungeFolder>> {
        Some(Self::new_boxed(ctx))
    }
}

#[async_trait]
impl ExpungeFolder for ExpungeGmailFolder {
    async fn expunge_folder(&self, folder: &str) -> AnyResult<()> {
        info!("gmail expunge is a no-op for folder {folder}");
        Ok(())
    }
}
