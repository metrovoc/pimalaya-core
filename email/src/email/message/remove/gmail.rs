use async_trait::async_trait;
use tracing::info;

use super::RemoveMessages;
use crate::{envelope::Id, gmail::GmailContext, AnyResult};

#[derive(Clone, Debug)]
pub struct RemoveGmailMessages {
    ctx: GmailContext,
}

impl RemoveGmailMessages {
    pub fn new(ctx: &GmailContext) -> Self {
        Self { ctx: ctx.clone() }
    }

    pub fn new_boxed(ctx: &GmailContext) -> Box<dyn RemoveMessages> {
        Box::new(Self::new(ctx))
    }

    pub fn some_new_boxed(ctx: &GmailContext) -> Option<Box<dyn RemoveMessages>> {
        Some(Self::new_boxed(ctx))
    }
}

#[async_trait]
impl RemoveMessages for RemoveGmailMessages {
    async fn remove_messages(&self, folder: &str, id: &Id) -> AnyResult<()> {
        info!("removing gmail messages {id} from folder {folder}");

        for id in id.iter() {
            self.ctx.delete_message_permanently(id).await?;
        }

        Ok(())
    }
}
