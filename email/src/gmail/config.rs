use std::io;

use secret::Secret;
use tracing::debug;

pub use super::{Error, Result};
use crate::account::config::oauth2::{OAuth2Config, OAuth2Method, OAuth2Scopes};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[cfg_attr(
    feature = "derive",
    derive(serde::Serialize, serde::Deserialize),
    serde(
        rename_all = "kebab-case",
        from = "GmailConfigDerive",
        into = "GmailConfigDerive"
    )
)]
pub struct GmailConfig {
    pub auth: GmailAuthConfig,
}

impl GmailConfig {
    pub const GOOGLE_AUTH_URL: &'static str = "https://accounts.google.com/o/oauth2/v2/auth";
    pub const GOOGLE_TOKEN_URL: &'static str = "https://www.googleapis.com/oauth2/v3/token";
    pub const GOOGLE_MAIL_SCOPE: &'static str = "https://mail.google.com/";

    pub fn default_oauth2_config() -> OAuth2Config {
        OAuth2Config {
            method: OAuth2Method::XOAuth2,
            client_id: String::new(),
            client_secret: None,
            auth_url: Self::GOOGLE_AUTH_URL.to_owned(),
            token_url: Self::GOOGLE_TOKEN_URL.to_owned(),
            access_token: Secret::default(),
            refresh_token: Secret::default(),
            pkce: true,
            redirect_scheme: None,
            redirect_host: None,
            redirect_port: None,
            scopes: OAuth2Scopes::Scope(Self::GOOGLE_MAIL_SCOPE.to_owned()),
        }
    }

    pub fn oauth2_config(&self) -> &OAuth2Config {
        self.auth.oauth2_config()
    }

    pub fn oauth2_config_mut(&mut self) -> &mut OAuth2Config {
        self.auth.oauth2_config_mut()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GmailAuthConfig {
    OAuth2(OAuth2Config),
}

impl Default for GmailAuthConfig {
    fn default() -> Self {
        Self::OAuth2(GmailConfig::default_oauth2_config())
    }
}

impl GmailAuthConfig {
    pub fn oauth2_config(&self) -> &OAuth2Config {
        match self {
            Self::OAuth2(config) => config,
        }
    }

    pub fn oauth2_config_mut(&mut self) -> &mut OAuth2Config {
        match self {
            Self::OAuth2(config) => config,
        }
    }

    pub async fn reset(&self) -> Result<()> {
        debug!("resetting gmail backend configuration");

        match self {
            Self::OAuth2(config) => config
                .reset()
                .await
                .map_err(Error::ResetOAuthSecretsError)?,
        }

        Ok(())
    }

    pub async fn configure(
        &self,
        get_client_secret: impl Fn() -> io::Result<String>,
    ) -> Result<()> {
        debug!("configuring gmail backend");

        match self {
            Self::OAuth2(config) => config
                .configure(get_client_secret)
                .await
                .map_err(Error::ConfiguringOAuthError)?,
        }

        Ok(())
    }

    #[cfg(feature = "keyring")]
    pub fn replace_empty_secrets(&mut self, name: impl AsRef<str>) -> Result<()> {
        let name = name.as_ref();

        match self {
            Self::OAuth2(config) => {
                if let Some(secret) = config.client_secret.as_mut() {
                    secret
                        .replace_with_keyring_if_empty(format!("{name}-gmail-oauth2-client-secret"))
                        .map_err(Error::ReplacingKeyringFailed)?;
                }

                config
                    .access_token
                    .replace_with_keyring_if_empty(format!("{name}-gmail-oauth2-access-token"))
                    .map_err(Error::ReplacingKeyringFailed)?;
                config
                    .refresh_token
                    .replace_with_keyring_if_empty(format!("{name}-gmail-oauth2-refresh-token"))
                    .map_err(Error::ReplacingKeyringFailed)?;
            }
        }

        Ok(())
    }
}

#[cfg(feature = "derive")]
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct GmailConfigDerive {
    #[serde(default = "default_gmail_oauth2_method")]
    method: OAuth2Method,
    #[serde(default)]
    client_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    client_secret: Option<Secret>,
    #[serde(default, skip_serializing_if = "Secret::is_empty")]
    access_token: Secret,
    #[serde(default, skip_serializing_if = "Secret::is_empty")]
    refresh_token: Secret,
    #[serde(default = "default_gmail_auth_url")]
    auth_url: String,
    #[serde(default = "default_gmail_token_url")]
    token_url: String,
    #[serde(default = "default_gmail_pkce")]
    pkce: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    redirect_scheme: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    redirect_host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    redirect_port: Option<u16>,
    #[serde(flatten, default = "default_gmail_scopes")]
    scopes: OAuth2Scopes,
}

#[cfg(feature = "derive")]
impl From<GmailConfigDerive> for GmailConfig {
    fn from(config: GmailConfigDerive) -> Self {
        Self {
            auth: GmailAuthConfig::OAuth2(OAuth2Config {
                method: config.method,
                client_id: config.client_id,
                client_secret: config.client_secret,
                auth_url: config.auth_url,
                token_url: config.token_url,
                access_token: config.access_token,
                refresh_token: config.refresh_token,
                pkce: config.pkce,
                redirect_scheme: config.redirect_scheme,
                redirect_host: config.redirect_host,
                redirect_port: config.redirect_port,
                scopes: config.scopes,
            }),
        }
    }
}

#[cfg(feature = "derive")]
impl From<GmailConfig> for GmailConfigDerive {
    fn from(config: GmailConfig) -> Self {
        let GmailAuthConfig::OAuth2(config) = config.auth;

        Self {
            method: config.method,
            client_id: config.client_id,
            client_secret: config.client_secret,
            access_token: config.access_token,
            refresh_token: config.refresh_token,
            auth_url: config.auth_url,
            token_url: config.token_url,
            pkce: config.pkce,
            redirect_scheme: config.redirect_scheme,
            redirect_host: config.redirect_host,
            redirect_port: config.redirect_port,
            scopes: config.scopes,
        }
    }
}

#[cfg(feature = "derive")]
fn default_gmail_oauth2_method() -> OAuth2Method {
    OAuth2Method::XOAuth2
}

#[cfg(feature = "derive")]
fn default_gmail_auth_url() -> String {
    GmailConfig::GOOGLE_AUTH_URL.to_owned()
}

#[cfg(feature = "derive")]
fn default_gmail_token_url() -> String {
    GmailConfig::GOOGLE_TOKEN_URL.to_owned()
}

#[cfg(feature = "derive")]
fn default_gmail_pkce() -> bool {
    true
}

#[cfg(feature = "derive")]
#[allow(dead_code)]
fn default_gmail_scopes() -> OAuth2Scopes {
    OAuth2Scopes::Scope(GmailConfig::GOOGLE_MAIL_SCOPE.to_owned())
}
