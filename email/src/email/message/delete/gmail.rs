use async_trait::async_trait;
use tracing::info;

use super::DeleteMessages;
use crate::{envelope::Id, gmail::GmailContext, AnyResult};

#[derive(Clone, Debug)]
pub struct DeleteGmailMessages {
    ctx: GmailContext,
}

impl DeleteGmailMessages {
    pub fn new(ctx: &GmailContext) -> Self {
        Self { ctx: ctx.clone() }
    }

    pub fn new_boxed(ctx: &GmailContext) -> Box<dyn DeleteMessages> {
        Box::new(Self::new(ctx))
    }

    pub fn some_new_boxed(ctx: &GmailContext) -> Option<Box<dyn DeleteMessages>> {
        Some(Self::new_boxed(ctx))
    }
}

#[async_trait]
impl DeleteMessages for DeleteGmailMessages {
    async fn delete_messages(&self, folder: &str, id: &Id) -> AnyResult<()> {
        info!("trashing gmail messages {id} from folder {folder}");

        for id in id.iter() {
            self.ctx.trash_message(id).await?;
        }

        Ok(())
    }
}
