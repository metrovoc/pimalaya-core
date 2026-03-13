use async_trait::async_trait;

use super::{DefaultGetMessages, GetMessages, Messages};
use crate::{
    envelope::Id,
    flag::{add::AddFlags, Flags},
    gmail::GmailContext,
    message::peek::{gmail::PeekGmailMessages, PeekMessages},
    AnyResult,
};

#[derive(Clone)]
pub struct GetGmailMessages {
    peek_messages: PeekGmailMessages,
    add_flags: crate::flag::add::gmail::AddGmailFlags,
}

impl GetGmailMessages {
    pub fn new(ctx: &GmailContext) -> Self {
        Self {
            peek_messages: PeekGmailMessages::new(ctx),
            add_flags: crate::flag::add::gmail::AddGmailFlags::new(ctx),
        }
    }

    pub fn new_boxed(ctx: &GmailContext) -> Box<dyn GetMessages> {
        Box::new(Self::new(ctx))
    }

    pub fn some_new_boxed(ctx: &GmailContext) -> Option<Box<dyn GetMessages>> {
        Some(Self::new_boxed(ctx))
    }
}

#[async_trait]
impl PeekMessages for GetGmailMessages {
    async fn peek_messages(&self, folder: &str, id: &Id) -> AnyResult<Messages> {
        self.peek_messages.peek_messages(folder, id).await
    }
}

#[async_trait]
impl AddFlags for GetGmailMessages {
    async fn add_flags(&self, folder: &str, id: &Id, flags: &Flags) -> AnyResult<()> {
        self.add_flags.add_flags(folder, id, flags).await
    }
}

#[async_trait]
impl DefaultGetMessages for GetGmailMessages {}
