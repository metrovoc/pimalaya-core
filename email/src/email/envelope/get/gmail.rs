use async_trait::async_trait;
use tracing::info;

use super::{Envelope, GetEnvelope};
use crate::{envelope::SingleId, gmail::GmailContext, AnyResult};

#[derive(Clone, Debug)]
pub struct GetGmailEnvelope {
    ctx: GmailContext,
}

impl GetGmailEnvelope {
    pub fn new(ctx: &GmailContext) -> Self {
        Self { ctx: ctx.clone() }
    }

    pub fn new_boxed(ctx: &GmailContext) -> Box<dyn GetEnvelope> {
        Box::new(Self::new(ctx))
    }

    pub fn some_new_boxed(ctx: &GmailContext) -> Option<Box<dyn GetEnvelope>> {
        Some(Self::new_boxed(ctx))
    }
}

#[async_trait]
impl GetEnvelope for GetGmailEnvelope {
    async fn get_envelope(&self, folder: &str, id: &SingleId) -> AnyResult<Envelope> {
        info!("getting gmail envelope {id:?} from folder {folder}");

        let current_folder_label_id = self.ctx.resolve_label_id(folder).await?;
        let labels_by_id = self.ctx.labels_by_id().await?;
        let message = self.ctx.get_message_metadata(id).await?;
        let envelope =
            self.ctx
                .build_envelope(message, Some(&current_folder_label_id), &labels_by_id);

        Ok(envelope)
    }
}
