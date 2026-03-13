#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![doc = include_str!("../README.md")]

mod error;

pub use ureq;
use ureq::{
    config::Config,
    http::Response,
    tls::{RootCerts, TlsConfig, TlsProvider},
    Agent, Body, Proxy, ProxyProtocol,
};

#[doc(inline)]
pub use crate::error::{Error, Result};

#[cfg(any(
    all(feature = "tokio", feature = "async-std"),
    not(any(feature = "tokio", feature = "async-std"))
))]
compile_error!("Either feature `tokio` or `async-std` must be enabled for this crate.");

#[cfg(any(
    all(feature = "rustls", feature = "native-tls"),
    not(any(feature = "rustls", feature = "native-tls"))
))]
compile_error!("Either feature `rustls` or `native-tls` must be enabled for this crate.");

/// The HTTP client structure.
///
/// This structure wraps a HTTP agent, which is used by the
/// [`Client::send`] function.
#[derive(Clone, Debug)]
pub struct Client {
    /// The HTTP agent used to perform calls.
    agent: Agent,
}

impl Client {
    /// Creates a new HTTP client with sane defaults.
    pub fn new() -> Self {
        let tls = TlsConfig::builder()
            .root_certs(RootCerts::PlatformVerifier)
            .provider(
                #[cfg(feature = "native-tls")]
                TlsProvider::NativeTls,
                #[cfg(feature = "rustls")]
                TlsProvider::Rustls,
            );

        let proxy = Self::detect_proxy();

        let config = Config::builder()
            .tls_config(tls.build())
            .proxy(proxy)
            .build();
        let agent = config.new_agent();

        Self { agent }
    }

    /// Detect proxy from environment, filtering out SOCKS proxies
    /// when the `socks-proxy` feature is not enabled in ureq.
    fn detect_proxy() -> Option<Proxy> {
        let proxy = Proxy::try_from_env()?;

        #[cfg(feature = "socks-proxy")]
        {
            return Some(proxy);
        }

        #[cfg(not(feature = "socks-proxy"))]
        match proxy.protocol() {
            ProxyProtocol::Socks4
            | ProxyProtocol::Socks4A
            | ProxyProtocol::Socks5
            | ProxyProtocol::Socks5h => {
                tracing::warn!(
                    "detected SOCKS proxy from environment but socks-proxy \
                     feature is not enabled, falling back to HTTP proxy"
                );
                Self::try_http_proxy_from_env()
            }
            ProxyProtocol::Http | ProxyProtocol::Https => Some(proxy),
            _ => Some(proxy),
        }
    }

    /// Try HTTPS_PROXY / HTTP_PROXY env vars only (skip ALL_PROXY).
    #[cfg(not(feature = "socks-proxy"))]
    fn try_http_proxy_from_env() -> Option<Proxy> {
        const HTTP_PROXY_VARS: &[&str] = &[
            "HTTPS_PROXY",
            "https_proxy",
            "HTTP_PROXY",
            "http_proxy",
        ];

        for var in HTTP_PROXY_VARS {
            if let Ok(val) = std::env::var(var) {
                if let Ok(proxy) = Proxy::new(&val) {
                    if !matches!(
                        proxy.protocol(),
                        ProxyProtocol::Socks4
                            | ProxyProtocol::Socks4A
                            | ProxyProtocol::Socks5
                            | ProxyProtocol::Socks5h
                    ) {
                        return Some(proxy);
                    }
                }
            }
        }

        None
    }

    /// Sends a request.
    ///
    /// This function takes a callback that tells how the request
    /// looks like. It takes a reference to the inner HTTP agent as
    /// parameter.
    pub async fn send(
        &self,
        f: impl FnOnce(&Agent) -> std::result::Result<Response<Body>, ureq::Error> + Send + 'static,
    ) -> Result<Response<Body>> {
        let agent = self.agent.clone();

        spawn_blocking(move || f(&agent))
            .await?
            .map_err(Error::SendRequestError)
    }
}

/// Spawns a blocking task using [`async_std`].
#[cfg(feature = "async-std")]
async fn spawn_blocking<F, T>(f: F) -> Result<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    Ok(async_std::task::spawn_blocking(f).await)
}

/// Spawns a blocking task using [`tokio`].
#[cfg(feature = "tokio")]
async fn spawn_blocking<F, T>(f: F) -> Result<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    Ok(tokio::task::spawn_blocking(f).await?)
}
