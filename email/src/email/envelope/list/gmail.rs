use async_trait::async_trait;
use chrono::TimeDelta;
use tracing::{debug, info};

use super::{Envelopes, ListEnvelopes, ListEnvelopesOptions};
use crate::{
    flag::Flag,
    gmail::GmailContext,
    search_query::{filter::SearchEmailsFilterQuery, SearchEmailsQuery},
    AnyResult,
};

#[derive(Clone, Debug)]
pub struct ListGmailEnvelopes {
    ctx: GmailContext,
}

impl ListGmailEnvelopes {
    pub fn new(ctx: &GmailContext) -> Self {
        Self { ctx: ctx.clone() }
    }

    pub fn new_boxed(ctx: &GmailContext) -> Box<dyn ListEnvelopes> {
        Box::new(Self::new(ctx))
    }

    pub fn some_new_boxed(ctx: &GmailContext) -> Option<Box<dyn ListEnvelopes>> {
        Some(Self::new_boxed(ctx))
    }
}

#[async_trait]
impl ListEnvelopes for ListGmailEnvelopes {
    async fn list_envelopes(
        &self,
        folder: &str,
        opts: ListEnvelopesOptions,
    ) -> AnyResult<Envelopes> {
        info!("listing gmail envelopes from folder {folder}");

        let current_folder_label_id = self.ctx.resolve_label_id(folder).await?;
        let gmail_query = opts
            .query
            .as_ref()
            .and_then(SearchEmailsQuery::to_gmail_query);

        let ids = if opts
            .query
            .as_ref()
            .and_then(|query| query.sort.as_ref())
            .is_some()
        {
            self.ctx
                .list_all_message_ids(&current_folder_label_id, gmail_query.as_deref())
                .await?
        } else {
            self.ctx
                .list_page_message_ids(
                    &current_folder_label_id,
                    gmail_query.as_deref(),
                    opts.page,
                    opts.page_size,
                )
                .await?
        };

        if ids.is_empty() {
            return Ok(Envelopes::default());
        }

        let labels_by_id = self.ctx.labels_by_id().await?;
        let messages = self.ctx.get_messages_metadata(&ids).await?;
        let mut envelopes: Envelopes = messages
            .into_iter()
            .map(|message| {
                self.ctx
                    .build_envelope(message, Some(&current_folder_label_id), &labels_by_id)
            })
            .collect();

        if opts
            .query
            .as_ref()
            .and_then(|query| query.sort.as_ref())
            .is_some()
        {
            opts.sort_envelopes(&mut envelopes);
            apply_pagination(&mut envelopes, opts.page, opts.page_size)?;
        }

        debug!(count = envelopes.len(), "found gmail envelopes");

        Ok(envelopes)
    }
}

impl SearchEmailsQuery {
    pub fn to_gmail_query(&self) -> Option<String> {
        self.filter
            .as_ref()
            .map(SearchEmailsFilterQuery::to_gmail_query)
            .filter(|query| !query.trim().is_empty())
    }
}

impl SearchEmailsFilterQuery {
    pub fn to_gmail_query(&self) -> String {
        match self {
            Self::And(left, right) => {
                format!("({} {})", left.to_gmail_query(), right.to_gmail_query())
            }
            Self::Or(left, right) => {
                format!("({} OR {})", left.to_gmail_query(), right.to_gmail_query())
            }
            Self::Not(query) => format!("-({})", query.to_gmail_query()),
            Self::Date(date) => {
                let next_day = *date + TimeDelta::try_days(1).unwrap();
                format!(
                    "after:{} before:{}",
                    date.format("%Y/%m/%d"),
                    next_day.format("%Y/%m/%d")
                )
            }
            Self::BeforeDate(date) => format!("before:{}", date.format("%Y/%m/%d")),
            Self::AfterDate(date) => format!("after:{}", date.format("%Y/%m/%d")),
            Self::From(pattern) => format!("from:{}", quote_gmail_term(pattern)),
            Self::To(pattern) => format!("to:{}", quote_gmail_term(pattern)),
            Self::Subject(pattern) => format!("subject:{}", quote_gmail_term(pattern)),
            Self::Body(pattern) => quote_gmail_term(pattern),
            Self::Flag(flag) => match flag {
                Flag::Seen => String::from("-is:unread"),
                Flag::Flagged => String::from("is:starred"),
                Flag::Draft => String::from("label:DRAFT"),
                Flag::Deleted => String::from("label:TRASH"),
                Flag::Answered => String::from("is:answered"),
                Flag::Custom(label) => format!("label:{}", quote_gmail_term(label)),
            },
        }
    }
}

fn quote_gmail_term(value: &str) -> String {
    if value
        .chars()
        .any(|char| char.is_whitespace() || matches!(char, '"' | '(' | ')' | ':'))
    {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        value.to_owned()
    }
}

fn apply_pagination(envelopes: &mut Envelopes, page: usize, page_size: usize) -> AnyResult<()> {
    if page_size == 0 {
        return Ok(());
    }

    let total = envelopes.len();
    let start = page.saturating_mul(page_size);

    if start >= total {
        return Err(crate::gmail::Error::PageOutOfBoundsError(page + 1).into());
    }

    let end = (start + page_size).min(total);
    *envelopes = envelopes[start..end].iter().cloned().collect();

    Ok(())
}
