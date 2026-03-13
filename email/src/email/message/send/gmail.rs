use async_trait::async_trait;
use serde_json::json;
use tracing::info;

use super::SendMessage;
use crate::{gmail::GmailContext, AnyResult};

#[derive(Clone, Debug)]
pub struct SendGmailMessage {
    ctx: GmailContext,
}

impl SendGmailMessage {
    pub fn new(ctx: &GmailContext) -> Self {
        Self { ctx: ctx.clone() }
    }

    pub fn new_boxed(ctx: &GmailContext) -> Box<dyn SendMessage> {
        Box::new(Self::new(ctx))
    }

    pub fn some_new_boxed(ctx: &GmailContext) -> Option<Box<dyn SendMessage>> {
        Some(Self::new_boxed(ctx))
    }
}

#[async_trait]
impl SendMessage for SendGmailMessage {
    async fn send_message(&self, msg: &[u8]) -> AnyResult<()> {
        info!("sending gmail message");

        let msg = self.ctx.preprocess_outgoing_message(msg).await;
        let body = json!({
            "raw": self.ctx.encode_raw_message(&msg),
        });

        let _: crate::gmail::GmailMessageStoredResponse = self
            .ctx
            .api_post_json("users/me/messages/send", &body)
            .await?;

        Ok(())
    }
}
