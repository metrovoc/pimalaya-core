use async_trait::async_trait;
use serde_json::json;
use tracing::info;

use super::{AddMessage, Flags};
use crate::{envelope::SingleId, gmail::GmailContext, AnyResult};

#[derive(Clone, Debug)]
pub struct AddGmailMessage {
    ctx: GmailContext,
}

impl AddGmailMessage {
    pub fn new(ctx: &GmailContext) -> Self {
        Self { ctx: ctx.clone() }
    }

    pub fn new_boxed(ctx: &GmailContext) -> Box<dyn AddMessage> {
        Box::new(Self::new(ctx))
    }

    pub fn some_new_boxed(ctx: &GmailContext) -> Option<Box<dyn AddMessage>> {
        Some(Self::new_boxed(ctx))
    }
}

#[async_trait]
impl AddMessage for AddGmailMessage {
    async fn add_message_with_flags(
        &self,
        folder: &str,
        msg: &[u8],
        flags: &Flags,
    ) -> AnyResult<SingleId> {
        info!("adding gmail message to folder {folder} with flags {flags}");

        let folder_label_id = self.ctx.resolve_label_id(folder).await?;
        let label_ids = self
            .ctx
            .build_import_label_ids(&folder_label_id, flags)
            .await?;
        let body = json!({
            "raw": self.ctx.encode_raw_message(msg),
            "labelIds": label_ids,
        });

        let response: crate::gmail::GmailMessageStoredResponse = self
            .ctx
            .api_post_json("users/me/messages/import", &body)
            .await?;

        Ok(SingleId::from(response.id))
    }
}
