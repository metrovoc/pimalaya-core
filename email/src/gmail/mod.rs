pub mod config;
mod error;

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, FixedOffset, TimeZone, Utc};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};
use tracing::{debug, info, warn};
use uuid::Uuid;

#[doc(inline)]
pub use self::{
    config::{GmailAuthConfig, GmailConfig},
    error::{Error, Result},
};
use crate::{
    account::config::{oauth2::OAuth2Config, AccountConfig},
    backend::{
        context::{BackendContext, BackendContextBuilder},
        feature::{BackendFeature, CheckUp},
    },
    envelope::{
        get::{gmail::GetGmailEnvelope, GetEnvelope},
        list::{gmail::ListGmailEnvelopes, ListEnvelopes},
        Envelope, Flags,
    },
    flag::{
        add::{gmail::AddGmailFlags, AddFlags},
        remove::{gmail::RemoveGmailFlags, RemoveFlags},
        set::{gmail::SetGmailFlags, SetFlags},
        Flag,
    },
    folder::{
        add::{gmail::AddGmailFolder, AddFolder},
        delete::{gmail::DeleteGmailFolder, DeleteFolder},
        expunge::{gmail::ExpungeGmailFolder, ExpungeFolder},
        list::{gmail::ListGmailFolders, ListFolders},
        purge::{gmail::PurgeGmailFolder, PurgeFolder},
        Folder, FolderKind,
    },
    message::{
        add::{gmail::AddGmailMessage, AddMessage},
        copy::{gmail::CopyGmailMessages, CopyMessages},
        delete::{gmail::DeleteGmailMessages, DeleteMessages},
        get::{gmail::GetGmailMessages, GetMessages},
        peek::{gmail::PeekGmailMessages, PeekMessages},
        r#move::{gmail::MoveGmailMessages, MoveMessages},
        remove::{gmail::RemoveGmailMessages, RemoveMessages},
        send::{gmail::SendGmailMessage, SendMessage},
        Message,
    },
    AnyResult,
};

const GMAIL_API_BASE_URL: &str = "https://gmail.googleapis.com/gmail/v1/";
const GMAIL_BATCH_URL: &str = "https://www.googleapis.com/batch/gmail/v1";
const GMAIL_PAGE_SIZE_LIMIT: usize = 500;
const GMAIL_BATCH_METADATA_SIZE: usize = 100;
const GMAIL_MAX_REQUEST_ATTEMPTS: u8 = 5;
const GMAIL_INITIAL_BACKOFF_MS: u64 = 250;

#[derive(Clone, Copy, Debug)]
pub(crate) enum GmailApiMethod {
    Get,
    Post,
    Delete,
}

#[derive(Clone, Debug)]
pub(crate) struct GmailHttpResponse {
    pub headers: HashMap<String, String>,
    pub body: String,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct GmailListLabelsResponse {
    #[serde(default)]
    pub labels: Vec<GmailLabel>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct GmailLabel {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub label_type: Option<String>,
}

impl GmailLabel {
    pub fn is_system(&self) -> bool {
        matches!(self.label_type.as_deref(), Some("system"))
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct GmailListMessagesResponse {
    #[serde(default)]
    pub messages: Vec<GmailMessageId>,
    #[serde(rename = "nextPageToken")]
    pub next_page_token: Option<String>,
    #[serde(rename = "resultSizeEstimate")]
    pub result_size_estimate: Option<usize>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct GmailMessageId {
    pub id: String,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct GmailMessageResponse {
    pub id: String,
    #[serde(rename = "labelIds", default)]
    pub label_ids: Vec<String>,
    #[serde(rename = "internalDate")]
    pub internal_date: Option<String>,
    pub payload: Option<GmailMessagePayload>,
    pub raw: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct GmailMessagePayload {
    #[serde(rename = "mimeType")]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub filename: String,
    #[serde(default)]
    pub headers: Vec<GmailMessageHeader>,
    #[serde(default)]
    pub parts: Vec<GmailMessagePayload>,
}

impl GmailMessagePayload {
    pub fn find_header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|header| header.name.eq_ignore_ascii_case(name))
            .map(|header| header.value.as_str())
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct GmailMessageHeader {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct GmailMessageStoredResponse {
    pub id: String,
}

#[derive(Clone, Debug, Deserialize)]
struct GmailProfileResponse {
    #[serde(rename = "emailAddress")]
    _email_address: String,
}

#[derive(Clone, Debug)]
pub struct GmailContext {
    pub account_config: Arc<AccountConfig>,
    pub gmail_config: Arc<GmailConfig>,
    http_client: http::Client,
    oauth2_config: OAuth2Config,
}

impl GmailContext {
    pub fn new(account_config: Arc<AccountConfig>, gmail_config: Arc<GmailConfig>) -> Self {
        Self {
            account_config,
            oauth2_config: gmail_config.oauth2_config().clone(),
            gmail_config,
            http_client: http::Client::new(),
        }
    }

    pub(crate) async fn get_access_token(&self) -> Result<String> {
        match self.oauth2_config.access_token().await {
            Ok(token) => Ok(token),
            Err(_) => self
                .oauth2_config
                .refresh_access_token()
                .await
                .map_err(Error::RefreshAccessTokenError),
        }
    }

    pub(crate) async fn list_labels(&self) -> Result<Vec<GmailLabel>> {
        let response: GmailListLabelsResponse = self.api_get_json("users/me/labels").await?;
        Ok(response.labels)
    }

    pub(crate) async fn labels_by_id(&self) -> Result<HashMap<String, GmailLabel>> {
        Ok(self
            .list_labels()
            .await?
            .into_iter()
            .map(|label| (label.id.clone(), label))
            .collect())
    }

    pub(crate) fn folder_from_label(&self, label: &GmailLabel) -> Option<Folder> {
        if should_skip_folder_label(label) {
            return None;
        }

        let (kind, name) = match label.id.as_str() {
            "INBOX" => (Some(FolderKind::Inbox), String::from("INBOX")),
            "SENT" => (Some(FolderKind::Sent), String::from("Sent")),
            "DRAFT" => (Some(FolderKind::Drafts), String::from("Drafts")),
            "TRASH" => (Some(FolderKind::Trash), String::from("Trash")),
            "SPAM" => (None, String::from("Junk")),
            _ => {
                let name = label.name.clone();
                let kind = self
                    .account_config
                    .find_folder_kind_from_alias(&name)
                    .or_else(|| name.parse().ok());
                (kind, name)
            }
        };

        Some(Folder {
            kind,
            name,
            desc: label.id.clone(),
        })
    }

    pub(crate) async fn resolve_label_id(&self, folder: &str) -> Result<String> {
        let alias = self.account_config.get_folder_alias(folder);

        if let Some(label_id) =
            find_system_label_id(folder).or_else(|| find_system_label_id(&alias))
        {
            return Ok(label_id.to_owned());
        }

        self.list_labels()
            .await?
            .into_iter()
            .find(|label| {
                label.id.eq_ignore_ascii_case(&alias) || label.name.eq_ignore_ascii_case(&alias)
            })
            .map(|label| label.id)
            .ok_or_else(|| Error::ResolveLabelError(alias))
    }

    pub(crate) async fn get_profile(&self) -> Result<()> {
        let _: GmailProfileResponse = self.api_get_json("users/me/profile").await?;
        Ok(())
    }

    pub(crate) async fn get_message_metadata(&self, id: &str) -> Result<GmailMessageResponse> {
        let path = format!("users/me/messages/{id}");
        let query = vec![("format", String::from("metadata"))];
        self.api_get_json_with_query(&path, &query).await
    }

    pub(crate) async fn get_messages_metadata(
        &self,
        ids: &[String],
    ) -> Result<Vec<GmailMessageResponse>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut messages = Vec::with_capacity(ids.len());

        for chunk in ids.chunks(GMAIL_BATCH_METADATA_SIZE) {
            let boundary = format!("gmail-batch-{}", Uuid::new_v4().simple());
            let mut body = String::new();

            for id in chunk {
                body.push_str(&format!(
                    "--{boundary}\r\nContent-Type: application/http\r\nContent-ID: <{id}>\r\n\r\nGET /gmail/v1/users/me/messages/{id}?format=metadata HTTP/1.1\r\n\r\n"
                ));
            }

            body.push_str(&format!("--{boundary}--\r\n"));

            let response = self
                .send_authorized_request_raw(
                    GmailApiMethod::Post,
                    GMAIL_BATCH_URL.to_owned(),
                    Some(format!("multipart/mixed; boundary={boundary}")),
                    Some(body.into_bytes()),
                )
                .await?;

            let content_type = response
                .headers
                .get("content-type")
                .cloned()
                .ok_or_else(|| {
                    Error::ParseBatchResponseError(String::from("missing content-type header"))
                })?;

            messages.extend(parse_batch_response(&content_type, &response.body)?);
        }

        let mut messages_by_id: HashMap<_, _> = messages
            .into_iter()
            .map(|message| (message.id.clone(), message))
            .collect();

        ids.iter()
            .map(|id| {
                messages_by_id
                    .remove(id)
                    .ok_or_else(|| Error::MessageNotFoundError(id.clone()))
            })
            .collect()
    }

    pub(crate) async fn get_message_raw(&self, id: &str) -> Result<Vec<u8>> {
        let path = format!("users/me/messages/{id}");
        let query = vec![("format", String::from("raw"))];
        let message: GmailMessageResponse = self.api_get_json_with_query(&path, &query).await?;
        let raw = message
            .raw
            .ok_or_else(|| Error::MessageNotFoundError(id.to_owned()))?;

        URL_SAFE_NO_PAD
            .decode(raw)
            .map_err(Error::Base64DecodeError)
    }

    pub(crate) async fn list_all_message_ids(
        &self,
        label_id: &str,
        gmail_query: Option<&str>,
    ) -> Result<Vec<String>> {
        let mut ids = Vec::new();
        let mut page_token = None;

        loop {
            let response = self
                .list_messages_page(
                    label_id,
                    gmail_query,
                    page_token.clone(),
                    GMAIL_PAGE_SIZE_LIMIT,
                )
                .await?;

            ids.extend(response.messages.into_iter().map(|message| message.id));

            match response.next_page_token {
                Some(token) => page_token = Some(token),
                None => break,
            }
        }

        Ok(ids)
    }

    pub(crate) async fn list_page_message_ids(
        &self,
        label_id: &str,
        gmail_query: Option<&str>,
        page: usize,
        page_size: usize,
    ) -> Result<Vec<String>> {
        if page_size == 0 {
            return self.list_all_message_ids(label_id, gmail_query).await;
        }

        let mut ids = Vec::with_capacity(page_size);
        let mut skipped = 0usize;
        let mut page_token = None;
        let target_start = page.saturating_mul(page_size);
        let mut first_page = true;

        loop {
            let response = self
                .list_messages_page(
                    label_id,
                    gmail_query,
                    page_token.clone(),
                    GMAIL_PAGE_SIZE_LIMIT,
                )
                .await?;

            if first_page {
                if let Some(total) = response.result_size_estimate {
                    if target_start >= total && total > 0 {
                        return Err(Error::PageOutOfBoundsError(page + 1));
                    }
                }
                first_page = false;
            }

            if response.messages.is_empty() {
                break;
            }

            for message in response.messages {
                if skipped < target_start {
                    skipped += 1;
                    continue;
                }

                ids.push(message.id);

                if ids.len() == page_size {
                    return Ok(ids);
                }
            }

            match response.next_page_token {
                Some(token) => page_token = Some(token),
                None => break,
            }
        }

        if ids.is_empty() && target_start > 0 {
            return Err(Error::PageOutOfBoundsError(page + 1));
        }

        Ok(ids)
    }

    pub(crate) fn build_envelope(
        &self,
        message: GmailMessageResponse,
        current_folder_label_id: Option<&str>,
        labels_by_id: &HashMap<String, GmailLabel>,
    ) -> Envelope {
        let GmailMessageResponse {
            id,
            label_ids,
            internal_date,
            payload,
            ..
        } = message;

        let flags = self.flags_from_label_ids(&label_ids, current_folder_label_id, labels_by_id);
        let mut raw_headers = String::new();

        if let Some(payload) = payload.as_ref() {
            for header_name in ["Message-ID", "In-Reply-To", "Date", "From", "To", "Subject"] {
                if let Some(header_value) = payload.find_header(header_name) {
                    raw_headers.push_str(header_name);
                    raw_headers.push_str(": ");
                    raw_headers.push_str(header_value);
                    raw_headers.push_str("\r\n");
                }
            }
        }

        raw_headers.push_str("\r\n");

        let mut envelope = Envelope::from_msg(id, flags, Message::from(raw_headers.as_bytes()));

        if envelope.date == DateTime::<FixedOffset>::default() {
            if let Some(date) = internal_date.as_deref().and_then(parse_internal_date) {
                envelope.date = date;
            }
        }

        envelope.has_attachment = payload
            .as_ref()
            .map(payload_has_attachment)
            .unwrap_or_default();

        envelope
    }

    pub(crate) fn flags_from_label_ids(
        &self,
        label_ids: &[String],
        current_folder_label_id: Option<&str>,
        labels_by_id: &HashMap<String, GmailLabel>,
    ) -> Flags {
        let mut flags = Flags::default();
        let label_ids_set: HashSet<_> = label_ids.iter().map(String::as_str).collect();

        if !label_ids_set.contains("UNREAD") {
            flags.insert(Flag::Seen);
        }

        if label_ids_set.contains("STARRED") {
            flags.insert(Flag::Flagged);
        }

        if label_ids_set.contains("DRAFT") {
            flags.insert(Flag::Draft);
        }

        if label_ids_set.contains("TRASH") {
            flags.insert(Flag::Deleted);
        }

        for label_id in label_ids {
            if should_skip_current_folder_label(current_folder_label_id, label_id) {
                continue;
            }

            if is_reserved_gmail_label(label_id) {
                continue;
            }

            if let Some(label) = labels_by_id.get(label_id) {
                if !label.is_system() {
                    flags.insert(Flag::Custom(label.name.clone()));
                }
            }
        }

        flags
    }

    pub(crate) async fn build_import_label_ids(
        &self,
        folder_label_id: &str,
        flags: &Flags,
    ) -> Result<Vec<String>> {
        let mut label_ids = self.desired_flag_label_ids(flags).await?;
        label_ids.insert(folder_label_id.to_owned());
        Ok(label_ids.into_iter().collect())
    }

    pub(crate) async fn add_flag_label_updates(
        &self,
        flags: &Flags,
    ) -> Result<(Vec<String>, Vec<String>)> {
        let mut add = HashSet::new();
        let mut remove = HashSet::new();

        for flag in flags.iter() {
            match flag {
                Flag::Seen => {
                    remove.insert(String::from("UNREAD"));
                }
                Flag::Flagged => {
                    add.insert(String::from("STARRED"));
                }
                Flag::Draft => {
                    add.insert(String::from("DRAFT"));
                }
                Flag::Deleted => {
                    add.insert(String::from("TRASH"));
                }
                Flag::Answered => {}
                Flag::Custom(label) => {
                    add.insert(self.resolve_label_id(label).await?);
                }
            }
        }

        Ok((add.into_iter().collect(), remove.into_iter().collect()))
    }

    pub(crate) async fn remove_flag_label_updates(
        &self,
        flags: &Flags,
    ) -> Result<(Vec<String>, Vec<String>)> {
        let mut add = HashSet::new();
        let mut remove = HashSet::new();

        for flag in flags.iter() {
            match flag {
                Flag::Seen => {
                    add.insert(String::from("UNREAD"));
                }
                Flag::Flagged => {
                    remove.insert(String::from("STARRED"));
                }
                Flag::Draft => {
                    remove.insert(String::from("DRAFT"));
                }
                Flag::Deleted => {
                    remove.insert(String::from("TRASH"));
                }
                Flag::Answered => {}
                Flag::Custom(label) => {
                    remove.insert(self.resolve_label_id(label).await?);
                }
            }
        }

        Ok((add.into_iter().collect(), remove.into_iter().collect()))
    }

    pub(crate) async fn set_flag_label_updates(
        &self,
        current_label_ids: &[String],
        current_folder_label_id: Option<&str>,
        flags: &Flags,
        labels_by_id: &HashMap<String, GmailLabel>,
    ) -> Result<(Vec<String>, Vec<String>)> {
        let current_flag_label_ids =
            current_flag_label_ids(current_label_ids, current_folder_label_id, labels_by_id);
        let desired_flag_label_ids = self.desired_flag_label_ids(flags).await?;

        let add = desired_flag_label_ids
            .difference(&current_flag_label_ids)
            .cloned()
            .collect();
        let remove = current_flag_label_ids
            .difference(&desired_flag_label_ids)
            .cloned()
            .collect();

        Ok((add, remove))
    }

    pub(crate) async fn desired_flag_label_ids(&self, flags: &Flags) -> Result<HashSet<String>> {
        let mut label_ids = HashSet::new();

        if !flags.contains(&Flag::Seen) {
            label_ids.insert(String::from("UNREAD"));
        }

        if flags.contains(&Flag::Flagged) {
            label_ids.insert(String::from("STARRED"));
        }

        if flags.contains(&Flag::Draft) {
            label_ids.insert(String::from("DRAFT"));
        }

        if flags.contains(&Flag::Deleted) {
            label_ids.insert(String::from("TRASH"));
        }

        for flag in flags.iter() {
            if let Flag::Custom(label) = flag {
                label_ids.insert(self.resolve_label_id(label).await?);
            }
        }

        Ok(label_ids)
    }

    pub(crate) async fn modify_message_labels(
        &self,
        id: &str,
        add_label_ids: &[String],
        remove_label_ids: &[String],
    ) -> Result<()> {
        if add_label_ids.is_empty() && remove_label_ids.is_empty() {
            return Ok(());
        }

        let path = format!("users/me/messages/{id}/modify");
        let body = json!({
            "addLabelIds": add_label_ids,
            "removeLabelIds": remove_label_ids,
        });

        let _: GmailMessageResponse = self.api_post_json(&path, &body).await?;
        Ok(())
    }

    pub(crate) async fn trash_message(&self, id: &str) -> Result<()> {
        let path = format!("users/me/messages/{id}/trash");
        self.api_post_empty(&path).await
    }

    pub(crate) async fn delete_message_permanently(&self, id: &str) -> Result<()> {
        let path = format!("users/me/messages/{id}");
        self.api_delete(&path).await
    }

    pub(crate) async fn preprocess_outgoing_message(&self, msg: &[u8]) -> Vec<u8> {
        let mut msg = msg.to_vec();

        if let Some(cmd) = self.account_config.find_message_pre_send_hook() {
            match cmd.run_with(&msg).await {
                Ok(res) => msg = res.into(),
                Err(err) => {
                    debug!(?err, "cannot execute Gmail pre-send hook");
                }
            }
        }

        msg
    }

    pub(crate) fn encode_raw_message(&self, msg: &[u8]) -> String {
        URL_SAFE_NO_PAD.encode(msg)
    }

    pub(crate) async fn api_get_json<T>(&self, path: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        self.api_get_json_with_query(path, &[]).await
    }

    pub(crate) async fn api_get_json_with_query<T>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T>
    where
        T: DeserializeOwned,
    {
        self.api_request_json(GmailApiMethod::Get, self.api_url(path, query), None)
            .await
    }

    pub(crate) async fn api_post_json<T>(&self, path: &str, body: &Value) -> Result<T>
    where
        T: DeserializeOwned,
    {
        self.api_request_json(
            GmailApiMethod::Post,
            self.api_url(path, &[]),
            Some(body.to_string().into_bytes()),
        )
        .await
    }

    pub(crate) async fn api_post_empty(&self, path: &str) -> Result<()> {
        self.send_authorized_request_raw(
            GmailApiMethod::Post,
            self.api_url(path, &[]),
            Some(String::from("application/json")),
            Some(Vec::from("{}")),
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn api_delete(&self, path: &str) -> Result<()> {
        self.send_authorized_request_raw(
            GmailApiMethod::Delete,
            self.api_url(path, &[]),
            None,
            None,
        )
        .await?;
        Ok(())
    }

    fn api_url(&self, path: &str, query: &[(&str, String)]) -> String {
        let path = path.trim_start_matches('/');
        let mut url = format!("{GMAIL_API_BASE_URL}{path}");

        if !query.is_empty() {
            url.push('?');
            url.push_str(
                &query
                    .iter()
                    .map(|(key, value)| format!("{key}={}", urlencoding::encode(value)))
                    .collect::<Vec<_>>()
                    .join("&"),
            );
        }

        url
    }

    async fn list_messages_page(
        &self,
        label_id: &str,
        gmail_query: Option<&str>,
        page_token: Option<String>,
        page_size: usize,
    ) -> Result<GmailListMessagesResponse> {
        let mut query = vec![
            ("labelIds", label_id.to_owned()),
            (
                "maxResults",
                page_size.min(GMAIL_PAGE_SIZE_LIMIT).to_string(),
            ),
        ];

        if let Some(gmail_query) = gmail_query.filter(|query| !query.trim().is_empty()) {
            query.push(("q", gmail_query.to_owned()));
        }

        if let Some(page_token) = page_token {
            query.push(("pageToken", page_token));
        }

        self.api_get_json_with_query("users/me/messages", &query)
            .await
    }

    async fn api_request_json<T>(
        &self,
        method: GmailApiMethod,
        url: String,
        body: Option<Vec<u8>>,
    ) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let response = self
            .send_authorized_request_raw(method, url, Some(String::from("application/json")), body)
            .await?;

        serde_json::from_str(&response.body).map_err(Error::ParseResponseError)
    }

    async fn send_authorized_request_raw(
        &self,
        method: GmailApiMethod,
        url: String,
        content_type: Option<String>,
        body: Option<Vec<u8>>,
    ) -> Result<GmailHttpResponse> {
        let mut refreshed_access_token = false;
        let mut backoff = Duration::from_millis(GMAIL_INITIAL_BACKOFF_MS);

        for attempt in 0..GMAIL_MAX_REQUEST_ATTEMPTS {
            let access_token = self.get_access_token().await?;
            let authorization = format!("Bearer {access_token}");
            let url_clone = url.clone();
            let content_type_clone = content_type.clone();
            let body_clone = body.clone().unwrap_or_default();

            let response = self
                .http_client
                .send(move |agent| match method {
                    GmailApiMethod::Get => {
                        let mut request = agent
                            .get(&url_clone)
                            .header("Authorization", &authorization)
                            .header("Accept", "application/json");

                        if let Some(content_type) = content_type_clone.as_deref() {
                            request = request.header("Content-Type", content_type);
                        }

                        Ok(request
                            .config()
                            .http_status_as_error(false)
                            .build()
                            .call()?)
                    }
                    GmailApiMethod::Post => {
                        let mut request = agent
                            .post(&url_clone)
                            .header("Authorization", &authorization)
                            .header("Accept", "application/json");

                        if let Some(content_type) = content_type_clone.as_deref() {
                            request = request.header("Content-Type", content_type);
                        }

                        Ok(request
                            .config()
                            .http_status_as_error(false)
                            .build()
                            .send(body_clone)?)
                    }
                    GmailApiMethod::Delete => {
                        let mut request = agent
                            .delete(&url_clone)
                            .header("Authorization", &authorization)
                            .header("Accept", "application/json");

                        if let Some(content_type) = content_type_clone.as_deref() {
                            request = request.header("Content-Type", content_type);
                        }

                        Ok(request
                            .config()
                            .http_status_as_error(false)
                            .build()
                            .call()?)
                    }
                })
                .await
                .map_err(|err| Error::ApiRequestError(err, url.clone()))?;

            let status = response.status();
            let status_code = status.as_u16();

            let headers = response
                .headers()
                .iter()
                .filter_map(|(name, value)| {
                    let value = value.to_str().ok()?;
                    Some((name.as_str().to_ascii_lowercase(), value.to_owned()))
                })
                .collect::<HashMap<_, _>>();

            let body = response
                .into_body()
                .read_to_string()
                .map_err(http::Error::from)
                .map_err(Error::ReadResponseError)?;

            if status_code == 401 && !refreshed_access_token {
                warn!("Gmail request returned 401, refreshing access token and retrying");
                self.oauth2_config
                    .refresh_access_token()
                    .await
                    .map_err(Error::RefreshAccessTokenError)?;
                refreshed_access_token = true;
                continue;
            }

            if is_retryable_status(status_code) && attempt + 1 < GMAIL_MAX_REQUEST_ATTEMPTS {
                warn!(
                    status = status_code,
                    "retrying Gmail request after API backoff"
                );
                sleep_backoff(backoff).await;
                backoff = backoff.saturating_mul(2);
                continue;
            }

            if !status.is_success() {
                return Err(Error::ApiResponseError {
                    status: status_code,
                    message: extract_api_error_message(&body),
                });
            }

            return Ok(GmailHttpResponse { headers, body });
        }

        Err(Error::ApiResponseError {
            status: 429,
            message: String::from("request retried too many times"),
        })
    }
}

impl BackendContext for GmailContext {}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GmailContextBuilder {
    pub account_config: Arc<AccountConfig>,
    pub gmail_config: Arc<GmailConfig>,
}

impl GmailContextBuilder {
    pub fn new(account_config: Arc<AccountConfig>, gmail_config: Arc<GmailConfig>) -> Self {
        Self {
            account_config,
            gmail_config,
        }
    }
}

#[async_trait]
impl BackendContextBuilder for GmailContextBuilder {
    type Context = GmailContext;

    fn check_configuration(&self) -> AnyResult<()> {
        if self
            .gmail_config
            .oauth2_config()
            .client_id
            .trim()
            .is_empty()
        {
            return Err(Error::MissingClientIdError.into());
        }

        Ok(())
    }

    fn check_up(&self) -> Option<BackendFeature<Self::Context, dyn CheckUp>> {
        Some(Arc::new(CheckUpGmail::some_new_boxed))
    }

    fn add_folder(&self) -> Option<BackendFeature<Self::Context, dyn AddFolder>> {
        Some(Arc::new(AddGmailFolder::some_new_boxed))
    }

    fn list_folders(&self) -> Option<BackendFeature<Self::Context, dyn ListFolders>> {
        Some(Arc::new(ListGmailFolders::some_new_boxed))
    }

    fn expunge_folder(&self) -> Option<BackendFeature<Self::Context, dyn ExpungeFolder>> {
        Some(Arc::new(ExpungeGmailFolder::some_new_boxed))
    }

    fn purge_folder(&self) -> Option<BackendFeature<Self::Context, dyn PurgeFolder>> {
        Some(Arc::new(PurgeGmailFolder::some_new_boxed))
    }

    fn delete_folder(&self) -> Option<BackendFeature<Self::Context, dyn DeleteFolder>> {
        Some(Arc::new(DeleteGmailFolder::some_new_boxed))
    }

    fn get_envelope(&self) -> Option<BackendFeature<Self::Context, dyn GetEnvelope>> {
        Some(Arc::new(GetGmailEnvelope::some_new_boxed))
    }

    fn list_envelopes(&self) -> Option<BackendFeature<Self::Context, dyn ListEnvelopes>> {
        Some(Arc::new(ListGmailEnvelopes::some_new_boxed))
    }

    fn add_flags(&self) -> Option<BackendFeature<Self::Context, dyn AddFlags>> {
        Some(Arc::new(AddGmailFlags::some_new_boxed))
    }

    fn set_flags(&self) -> Option<BackendFeature<Self::Context, dyn SetFlags>> {
        Some(Arc::new(SetGmailFlags::some_new_boxed))
    }

    fn remove_flags(&self) -> Option<BackendFeature<Self::Context, dyn RemoveFlags>> {
        Some(Arc::new(RemoveGmailFlags::some_new_boxed))
    }

    fn add_message(&self) -> Option<BackendFeature<Self::Context, dyn AddMessage>> {
        Some(Arc::new(AddGmailMessage::some_new_boxed))
    }

    fn send_message(&self) -> Option<BackendFeature<Self::Context, dyn SendMessage>> {
        Some(Arc::new(SendGmailMessage::some_new_boxed))
    }

    fn peek_messages(&self) -> Option<BackendFeature<Self::Context, dyn PeekMessages>> {
        Some(Arc::new(PeekGmailMessages::some_new_boxed))
    }

    fn get_messages(&self) -> Option<BackendFeature<Self::Context, dyn GetMessages>> {
        Some(Arc::new(GetGmailMessages::some_new_boxed))
    }

    fn copy_messages(&self) -> Option<BackendFeature<Self::Context, dyn CopyMessages>> {
        Some(Arc::new(CopyGmailMessages::some_new_boxed))
    }

    fn move_messages(&self) -> Option<BackendFeature<Self::Context, dyn MoveMessages>> {
        Some(Arc::new(MoveGmailMessages::some_new_boxed))
    }

    fn delete_messages(&self) -> Option<BackendFeature<Self::Context, dyn DeleteMessages>> {
        Some(Arc::new(DeleteGmailMessages::some_new_boxed))
    }

    fn remove_messages(&self) -> Option<BackendFeature<Self::Context, dyn RemoveMessages>> {
        Some(Arc::new(RemoveGmailMessages::some_new_boxed))
    }

    async fn build(self) -> AnyResult<Self::Context> {
        info!("building new gmail context");

        Ok(GmailContext::new(self.account_config, self.gmail_config))
    }
}

#[derive(Clone)]
pub struct CheckUpGmail {
    ctx: GmailContext,
}

impl CheckUpGmail {
    pub fn new(ctx: &GmailContext) -> Self {
        Self { ctx: ctx.clone() }
    }

    pub fn new_boxed(ctx: &GmailContext) -> Box<dyn CheckUp> {
        Box::new(Self::new(ctx))
    }

    pub fn some_new_boxed(ctx: &GmailContext) -> Option<Box<dyn CheckUp>> {
        Some(Self::new_boxed(ctx))
    }
}

#[async_trait]
impl CheckUp for CheckUpGmail {
    async fn check_up(&self) -> AnyResult<()> {
        self.ctx.get_profile().await?;
        Ok(())
    }
}

fn should_skip_folder_label(label: &GmailLabel) -> bool {
    matches!(
        label.id.as_str(),
        "STARRED" | "UNREAD" | "IMPORTANT" | "CHAT"
    ) || label.id.starts_with("CATEGORY_")
}

fn find_system_label_id(folder: &str) -> Option<&'static str> {
    let normalized = folder.trim().to_ascii_lowercase();

    match normalized.as_str() {
        "inbox" => Some("INBOX"),
        "sent" | "sent mail" | "[gmail]/sent mail" | "[google mail]/sent mail" => Some("SENT"),
        "draft" | "drafts" | "[gmail]/drafts" | "[google mail]/drafts" => Some("DRAFT"),
        "trash" | "[gmail]/trash" | "[google mail]/trash" => Some("TRASH"),
        "junk" | "spam" | "[gmail]/spam" | "[google mail]/spam" => Some("SPAM"),
        _ => None,
    }
}

fn should_skip_current_folder_label(current_folder_label_id: Option<&str>, label_id: &str) -> bool {
    current_folder_label_id
        .filter(|current| *current == label_id)
        .filter(|current| !matches!(*current, "DRAFT" | "TRASH"))
        .is_some()
}

fn is_reserved_gmail_label(label_id: &str) -> bool {
    matches!(
        label_id,
        "INBOX" | "SENT" | "SPAM" | "UNREAD" | "STARRED" | "IMPORTANT" | "CHAT"
    ) || label_id.starts_with("CATEGORY_")
}

fn current_flag_label_ids(
    current_label_ids: &[String],
    current_folder_label_id: Option<&str>,
    labels_by_id: &HashMap<String, GmailLabel>,
) -> HashSet<String> {
    let mut label_ids = HashSet::new();

    for label_id in current_label_ids {
        if should_skip_current_folder_label(current_folder_label_id, label_id) {
            continue;
        }

        match label_id.as_str() {
            "UNREAD" | "STARRED" | "DRAFT" | "TRASH" => {
                label_ids.insert(label_id.clone());
            }
            _ => {
                if let Some(label) = labels_by_id.get(label_id) {
                    if !label.is_system() {
                        label_ids.insert(label_id.clone());
                    }
                }
            }
        }
    }

    label_ids
}

fn payload_has_attachment(payload: &GmailMessagePayload) -> bool {
    if !payload.filename.is_empty() {
        return true;
    }

    if payload.parts.iter().any(payload_has_attachment) {
        return true;
    }

    matches!(
        payload.mime_type.as_deref(),
        Some(mime_type) if !mime_type.starts_with("text/") && !mime_type.starts_with("multipart/")
    )
}

fn parse_internal_date(value: &str) -> Option<DateTime<FixedOffset>> {
    let timestamp = value.parse::<i64>().ok()?;
    let date = Utc.timestamp_millis_opt(timestamp).single()?;
    Some(date.with_timezone(&FixedOffset::east_opt(0)?))
}

fn extract_api_error_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value.get("error").and_then(|value| {
                value
                    .get("message")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                    .or_else(|| value.as_str().map(ToOwned::to_owned))
            })
        })
        .unwrap_or_else(|| {
            let message = body.trim();
            if message.is_empty() {
                String::from("unknown Gmail API error")
            } else {
                message.to_owned()
            }
        })
}

fn is_retryable_status(status: u16) -> bool {
    matches!(status, 429 | 500 | 502 | 503 | 504)
}

fn parse_batch_response(content_type: &str, body: &str) -> Result<Vec<GmailMessageResponse>> {
    let boundary = content_type
        .split(';')
        .find_map(|part| part.trim().strip_prefix("boundary="))
        .map(|boundary| boundary.trim_matches('"').to_owned())
        .ok_or_else(|| Error::ParseBatchResponseError(String::from("missing boundary")))?;

    let boundary = format!("--{boundary}");
    let mut messages = Vec::new();

    for part in body.split(&boundary) {
        let part = part.trim();
        if part.is_empty() || part == "--" {
            continue;
        }

        let (_, response) = split_once_empty_line(part)
            .ok_or_else(|| Error::ParseBatchResponseError(String::from("invalid MIME part")))?;

        let response = response.trim();
        let (status_line, response) = response.split_once('\n').ok_or_else(|| {
            Error::ParseBatchResponseError(String::from("missing HTTP status line"))
        })?;

        let status = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|status| status.parse::<u16>().ok())
            .ok_or_else(|| {
                Error::ParseBatchResponseError(String::from("invalid HTTP status line"))
            })?;

        let (_, response_body) = split_once_empty_line(response).ok_or_else(|| {
            Error::ParseBatchResponseError(String::from("missing HTTP response body"))
        })?;

        if !(200..300).contains(&status) {
            return Err(Error::ApiResponseError {
                status,
                message: extract_api_error_message(response_body.trim()),
            });
        }

        let message =
            serde_json::from_str(response_body.trim()).map_err(Error::ParseResponseError)?;
        messages.push(message);
    }

    Ok(messages)
}

fn split_once_empty_line(input: &str) -> Option<(&str, &str)> {
    input
        .split_once("\r\n\r\n")
        .or_else(|| input.split_once("\n\n"))
}

#[cfg(feature = "tokio")]
async fn sleep_backoff(duration: Duration) {
    tokio::time::sleep(duration).await;
}

#[cfg(feature = "async-std")]
async fn sleep_backoff(duration: Duration) {
    async_std::task::sleep(duration).await;
}
