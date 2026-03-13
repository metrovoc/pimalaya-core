use async_trait::async_trait;
use tracing::info;

use super::{Flags, RemoveFlags};
use crate::{envelope::Id, gmail::GmailContext, AnyResult};

#[derive(Clone, Debug)]
pub struct RemoveGmailFlags {
    ctx: GmailContext,
}

impl RemoveGmailFlags {
    pub fn new(ctx: &GmailContext) -> Self {
        Self { ctx: ctx.clone() }
    }

    pub fn new_boxed(ctx: &GmailContext) -> Box<dyn RemoveFlags> {
        Box::new(Self::new(ctx))
    }

    pub fn some_new_boxed(ctx: &GmailContext) -> Option<Box<dyn RemoveFlags>> {
        Some(Self::new_boxed(ctx))
    }
}

#[async_trait]
impl RemoveFlags for RemoveGmailFlags {
    async fn remove_flags(&self, folder: &str, id: &Id, flags: &Flags) -> AnyResult<()> {
        info!("removing gmail flags {flags} from message(s) {id} from folder {folder}");

        let (add_label_ids, remove_label_ids) = self.ctx.remove_flag_label_updates(flags).await?;

        for id in id.iter() {
            self.ctx
                .modify_message_labels(id, &add_label_ids, &remove_label_ids)
                .await?;
        }

        Ok(())
    }
}
