use async_trait::async_trait;
use tracing::info;

use super::{Messages, PeekMessages};
use crate::{envelope::Id, gmail::GmailContext, AnyResult};

#[derive(Clone, Debug)]
pub struct PeekGmailMessages {
    ctx: GmailContext,
}

impl PeekGmailMessages {
    pub fn new(ctx: &GmailContext) -> Self {
        Self { ctx: ctx.clone() }
    }

    pub fn new_boxed(ctx: &GmailContext) -> Box<dyn PeekMessages> {
        Box::new(Self::new(ctx))
    }

    pub fn some_new_boxed(ctx: &GmailContext) -> Option<Box<dyn PeekMessages>> {
        Some(Self::new_boxed(ctx))
    }
}

#[async_trait]
impl PeekMessages for PeekGmailMessages {
    async fn peek_messages(&self, folder: &str, id: &Id) -> AnyResult<Messages> {
        info!("peeking gmail messages {id} from folder {folder}");

        let mut raw_messages = Vec::new();

        for id in id.iter() {
            raw_messages.push(self.ctx.get_message_raw(id).await?);
        }

        Ok(Messages::from(raw_messages))
    }
}
