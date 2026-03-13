use async_trait::async_trait;
use tracing::info;

use super::{AddFlags, Flags};
use crate::{envelope::Id, gmail::GmailContext, AnyResult};

#[derive(Clone, Debug)]
pub struct AddGmailFlags {
    ctx: GmailContext,
}

impl AddGmailFlags {
    pub fn new(ctx: &GmailContext) -> Self {
        Self { ctx: ctx.clone() }
    }

    pub fn new_boxed(ctx: &GmailContext) -> Box<dyn AddFlags> {
        Box::new(Self::new(ctx))
    }

    pub fn some_new_boxed(ctx: &GmailContext) -> Option<Box<dyn AddFlags>> {
        Some(Self::new_boxed(ctx))
    }
}

#[async_trait]
impl AddFlags for AddGmailFlags {
    async fn add_flags(&self, folder: &str, id: &Id, flags: &Flags) -> AnyResult<()> {
        info!("adding gmail flags {flags} to message(s) {id} from folder {folder}");

        let (add_label_ids, remove_label_ids) = self.ctx.add_flag_label_updates(flags).await?;

        for id in id.iter() {
            self.ctx
                .modify_message_labels(id, &add_label_ids, &remove_label_ids)
                .await?;
        }

        Ok(())
    }
}
