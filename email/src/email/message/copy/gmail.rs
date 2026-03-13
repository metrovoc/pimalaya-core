use async_trait::async_trait;
use tracing::info;

use super::CopyMessages;
use crate::{envelope::Id, gmail::GmailContext, AnyResult};

#[derive(Clone, Debug)]
pub struct CopyGmailMessages {
    ctx: GmailContext,
}

impl CopyGmailMessages {
    pub fn new(ctx: &GmailContext) -> Self {
        Self { ctx: ctx.clone() }
    }

    pub fn new_boxed(ctx: &GmailContext) -> Box<dyn CopyMessages> {
        Box::new(Self::new(ctx))
    }

    pub fn some_new_boxed(ctx: &GmailContext) -> Option<Box<dyn CopyMessages>> {
        Some(Self::new_boxed(ctx))
    }
}

#[async_trait]
impl CopyMessages for CopyGmailMessages {
    async fn copy_messages(&self, from_folder: &str, to_folder: &str, id: &Id) -> AnyResult<()> {
        info!("copying gmail messages {id} from folder {from_folder} to folder {to_folder}");

        let to_label_id = self.ctx.resolve_label_id(to_folder).await?;

        for id in id.iter() {
            self.ctx
                .modify_message_labels(id, &[to_label_id.clone()], &[])
                .await?;
        }

        Ok(())
    }
}
