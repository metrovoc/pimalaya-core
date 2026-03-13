use async_trait::async_trait;
use serde_json::json;
use tracing::info;

use super::AddFolder;
use crate::{gmail::GmailContext, AnyResult};

#[derive(Clone, Debug)]
pub struct AddGmailFolder {
    ctx: GmailContext,
}

impl AddGmailFolder {
    pub fn new(ctx: &GmailContext) -> Self {
        Self { ctx: ctx.clone() }
    }

    pub fn new_boxed(ctx: &GmailContext) -> Box<dyn AddFolder> {
        Box::new(Self::new(ctx))
    }

    pub fn some_new_boxed(ctx: &GmailContext) -> Option<Box<dyn AddFolder>> {
        Some(Self::new_boxed(ctx))
    }
}

#[async_trait]
impl AddFolder for AddGmailFolder {
    async fn add_folder(&self, folder: &str) -> AnyResult<()> {
        info!("creating gmail label {folder}");

        let name = self.ctx.account_config.get_folder_alias(folder);
        let body = json!({
            "name": name,
            "labelListVisibility": "labelShow",
            "messageListVisibility": "show",
        });

        let _: serde_json::Value = self.ctx.api_post_json("users/me/labels", &body).await?;
        Ok(())
    }
}
