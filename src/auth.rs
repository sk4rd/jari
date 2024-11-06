use actix::Response;
use actix_web::{FromRequest, HttpResponse};
use futures::io::Empty;
use openidconnect::reqwest::async_http_client;
use openidconnect::{
    core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata},
    ClientId, ClientSecret, IssuerUrl,
};
use openidconnect::{AuthPrompt, PkceCodeVerifier, RedirectUrl};

use actix_web::{
    routes,
    web::{self},
    Responder,
};
use dotenvy::dotenv;
use openidconnect::{
    AccessTokenHash, AuthenticationFlow, AuthorizationCode, CsrfToken, Nonce, OAuth2TokenResponse,
    PkceCodeChallenge, Scope, TokenResponse,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::future::{ready, Ready};
use std::sync::Arc;

use crate::errors::PageError;
use crate::AppState;

#[derive(Debug)]
pub struct OidcClient {
    pub client: CoreClient,
    pub active_requests: tokio::sync::Mutex<HashMap<String, (PkceCodeVerifier, Nonce)>>,
}

#[derive(Debug, Deserialize)]
pub struct RedirectResponse {
    pub state: String,
    pub code: String,
    pub scope: String,
}

#[derive(Debug, Clone)]
pub struct Token(pub Option<String>);

impl Token {
    pub fn into_inner(self) -> Option<String> {
        self.0
    }
}

impl FromRequest for Token {
    type Error = PageError;
    type Future = Ready<Result<Self, Self::Error>>;
    fn from_request(req: &actix_web::HttpRequest, _: &mut actix_web::dev::Payload) -> Self::Future {
        let token = req
            .headers()
            .get(actix_web::http::header::AUTHORIZATION)
            .and_then(|x| x.to_str().ok())
            .map(|x| x.to_owned());
        ready(Ok(Self(token)))
    }
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
        let client_id = ClientId::new(env::var("OIDC_CLIENT_ID").expect("Missing OIDC_CLIENT_ID"));

        let client_secret =
            ClientSecret::new(env::var("OIDC_CLIENT_SECRET").expect("Missing OIDC_CLIENT_SECRET"));

        let redirect_url = RedirectUrl::new("https://jari.sk4rd.com/auth/callback".to_string())
            .expect("Invalid redirect URL");

        let client =
            CoreClient::from_provider_metadata(provider_metadata, client_id, Some(client_secret))
                .set_redirect_uri(redirect_url);

        OidcClient {
            client,
            active_requests: tokio::sync::Mutex::new(HashMap::new()),
        }
    }
}

#[routes]
#[get("/auth/google/start")]
#[get("/auth/google/start/")]
pub async fn google_redirect(state: web::Data<Arc<AppState>>) -> impl Responder {
    let client = state.oidc_client.client.clone();
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    // Generate the full authorization URL.
    let (auth_url, csrf_token, nonce) = client
        .authorize_url(
            CoreAuthenticationFlow::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        )
        .add_scope(Scope::new("openid".to_string()))
        // Set the PKCE code challenge.
        .set_pkce_challenge(pkce_challenge)
        .url();

    state
        .oidc_client
        .active_requests
        .lock()
        .await
        .insert(csrf_token.secret().clone(), (pkce_verifier, nonce));

    HttpResponse::TemporaryRedirect()
        .insert_header(("Location", auth_url.as_str()))
        .body("")
}

#[routes]
#[get("/auth/callback")]
#[get("/auth/callback/")]
pub async fn google_callback(
    state: web::Data<Arc<AppState>>,
    query: web::Query<RedirectResponse>,
) -> impl Responder {
    let client = state.oidc_client.client.clone();
    let redirect_response = query.into_inner();
    let (pkce_verifier, nonce) = state
        .oidc_client
        .active_requests
        .lock()
        .await
        .remove(&redirect_response.state)
        .ok_or(PageError::NotFound)?;
    let token_response = client
        .exchange_code(AuthorizationCode::new(redirect_response.code))
        // Set the PKCE code verifier.
        .set_pkce_verifier(pkce_verifier)
        .request_async(async_http_client)
        .await
        .map_err(|_| PageError::AuthError)?;

    // Extract the ID token claims after verifying its authenticity and nonce.
    let id_token = token_response
        .id_token()
        .ok_or_else(|| PageError::AuthError)?;
    let claims = id_token
        .claims(&client.id_token_verifier(), &nonce)
        .map_err(|_| PageError::AuthError)?;

    // Verify the access token hash to ensure that the access token hasn't been substituted for
    // another user's.
    if let Some(expected_access_token_hash) = claims.access_token_hash() {
        let actual_access_token_hash = AccessTokenHash::from_token(
            token_response.access_token(),
            &id_token.signing_alg().map_err(|_| PageError::AuthError)?,
        )
        .map_err(|_| PageError::AuthError)?;
        if actual_access_token_hash != *expected_access_token_hash {
            return Err(PageError::AuthError);
        }
    }

    state
        .users
        .write()
        .await
        .insert(claims.subject().clone(), None);

    Ok(HttpResponse::Ok().body(
        "
        <!DOCTYPE html>
        <html>
            <body>
                <script>
                    const searchParams = new URLSearchParams(window.location.search);
                    localStorage.setItem('JWT', searchParams.get('code'));
                    window.location.href = '/';
                </script>
            </body>
        </html>
        ",
    ))
}
