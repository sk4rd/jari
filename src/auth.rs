use openidconnect::reqwest::async_http_client;
use openidconnect::RedirectUrl;
use openidconnect::{
    core::{CoreClient, CoreProviderMetadata},
    ClientId, ClientSecret, IssuerUrl,
};

use dotenvy::dotenv;
use std::env;

#[derive(Debug)]
pub struct OidcClient {
    pub client: CoreClient,
}

impl OidcClient {
    pub async fn new() -> Self {
        // Load env vars from .env
        dotenv().ok();
        
        let issuer_url =
            IssuerUrl::new("https://accounts.google.com".to_string()).expect("Invalid issuer URL");

        // Fetch OpenID Connect discovery document
        let provider_metadata = CoreProviderMetadata::discover_async(issuer_url, async_http_client)
            .await
            .expect("Failed to discover OpenId provider");

        // Get client id and secret from env vars
        let client_id =
            ClientId::new(env::var("OIDC_CLIENT_ID").expect("Missing OIDC_CLIENT_ID"));

        let client_secret = ClientSecret::new(
            env::var("OIDC_CLIENT_SECRET").expect("Missing OIDC_CLIENT_SECRET"),
        );

        let redirect_url = RedirectUrl::new("http://localhost:8080/auth/callback".to_string())
            .expect("Invalid redirect URL");

        let client =
            CoreClient::from_provider_metadata(provider_metadata, client_id, Some(client_secret))
                .set_redirect_uri(redirect_url);

        OidcClient { client }
    }
}
