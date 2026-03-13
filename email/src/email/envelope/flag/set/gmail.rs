use async_trait::async_trait;
use tracing::info;

use super::{Flags, SetFlags};
use crate::{envelope::Id, gmail::GmailContext, AnyResult};

#[derive(Clone, Debug)]
pub struct SetGmailFlags {
    ctx: GmailContext,
}

impl SetGmailFlags {
    pub fn new(ctx: &GmailContext) -> Self {
        Self { ctx: ctx.clone() }
    }

    pub fn new_boxed(ctx: &GmailContext) -> Box<dyn SetFlags> {
        Box::new(Self::new(ctx))
    }

    pub fn some_new_boxed(ctx: &GmailContext) -> Option<Box<dyn SetFlags>> {
        Some(Self::new_boxed(ctx))
    }
}

#[async_trait]
impl SetFlags for SetGmailFlags {
    async fn set_flags(&self, folder: &str, id: &Id, flags: &Flags) -> AnyResult<()> {
        info!("setting gmail flags {flags} to message(s) {id} from folder {folder}");

        let current_folder_label_id = self.ctx.resolve_label_id(folder).await?;
        let labels_by_id = self.ctx.labels_by_id().await?;

        for id in id.iter() {
            let message = self.ctx.get_message_metadata(id).await?;
            let (add_label_ids, remove_label_ids) = self
                .ctx
                .set_flag_label_updates(
                    &message.label_ids,
                    Some(&current_folder_label_id),
                    flags,
                    &labels_by_id,
                )
                .await?;

            self.ctx
                .modify_message_labels(id, &add_label_ids, &remove_label_ids)
                .await?;
        }

        Ok(())
    }
}
