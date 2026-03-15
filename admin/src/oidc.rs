use anyhow::Result;
use openidconnect::{
    core::{CoreClient, CoreProviderMetadata},
    reqwest::async_http_client,
    ClientId, ClientSecret, IssuerUrl, RedirectUrl,
};

use crate::config::Config;

pub async fn create_client(config: &Config) -> Result<CoreClient> {
    // Auth0 の issuer URI はトレイリングスラッシュ付き（例: https://xxx.auth0.com/）
    let issuer_url = IssuerUrl::new(format!("https://{}/", config.auth0.domain))
        .map_err(|e| anyhow::anyhow!("IssuerURL の作成に失敗: {}", e))?;

    let provider_metadata = CoreProviderMetadata::discover_async(issuer_url, async_http_client)
        .await
        .map_err(|e| anyhow::anyhow!("OIDC プロバイダーの検索に失敗: {}", e))?;

    let redirect_url = RedirectUrl::new(format!("{}/auth/callback", config.server.base_url))
        .map_err(|e| anyhow::anyhow!("リダイレクト URL の作成に失敗: {}", e))?;

    let client = CoreClient::from_provider_metadata(
        provider_metadata,
        ClientId::new(config.auth0.client_id.clone()),
        Some(ClientSecret::new(config.auth0.client_secret.clone())),
    )
    .set_redirect_uri(redirect_url);

    Ok(client)
}
