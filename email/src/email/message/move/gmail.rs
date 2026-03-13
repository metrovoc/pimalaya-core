use async_trait::async_trait;
use tracing::info;

use super::MoveMessages;
use crate::{envelope::Id, gmail::GmailContext, AnyResult};

#[derive(Clone, Debug)]
pub struct MoveGmailMessages {
    ctx: GmailContext,
}

impl MoveGmailMessages {
    pub fn new(ctx: &GmailContext) -> Self {
        Self { ctx: ctx.clone() }
    }

    pub fn new_boxed(ctx: &GmailContext) -> Box<dyn MoveMessages> {
        Box::new(Self::new(ctx))
    }

    pub fn some_new_boxed(ctx: &GmailContext) -> Option<Box<dyn MoveMessages>> {
        Some(Self::new_boxed(ctx))
    }
}

#[async_trait]
impl MoveMessages for MoveGmailMessages {
    async fn move_messages(&self, from_folder: &str, to_folder: &str, id: &Id) -> AnyResult<()> {
        info!("moving gmail messages {id} from folder {from_folder} to folder {to_folder}");

        let from_label_id = self.ctx.resolve_label_id(from_folder).await?;
        let to_label_id = self.ctx.resolve_label_id(to_folder).await?;

        if from_label_id == to_label_id {
            return Ok(());
        }

        for id in id.iter() {
            self.ctx
                .modify_message_labels(id, &[to_label_id.clone()], &[from_label_id.clone()])
                .await?;
        }

        Ok(())
    }
}
