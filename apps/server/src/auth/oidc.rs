use openidconnect::core::{CoreClient, CoreProviderMetadata, CoreResponseType};
use openidconnect::{
    reqwest, AuthenticationFlow, AuthorizationCode, ClientId, ClientSecret, CsrfToken, IssuerUrl,
    Nonce, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope,
};

use crate::config::OidcConfig;
use crate::error::{AppError, AppResult};

/// Values that must be retained in the encrypted browser session while the
/// user authenticates with the identity provider.
pub struct OidcAuthorization {
    pub url: String,
    pub state: String,
    pub nonce: String,
    pub pkce_verifier: String,
}

/// Validated identity claims used to resolve a local account.
pub struct OidcIdentity {
    pub issuer: String,
    pub subject: String,
    pub email: String,
}

#[derive(Clone)]
pub struct OidcService {
    metadata: CoreProviderMetadata,
    http_client: reqwest::Client,
    config: OidcConfig,
}

impl OidcService {
    /// Discover the provider once during startup. Startup fails when SSO is
    /// explicitly configured but its discovery document is unavailable or
    /// invalid, avoiding a login button that can never work.
    pub async fn discover(config: OidcConfig) -> AppResult<Self> {
        let issuer = IssuerUrl::new(config.issuer_url.clone())
            .map_err(|e| AppError::Internal(format!("Invalid OIDC issuer URL: {e}")))?;
        let http_client = reqwest::ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| AppError::Internal(format!("Failed to create OIDC HTTP client: {e}")))?;
        let metadata = CoreProviderMetadata::discover_async(issuer, &http_client)
            .await
            .map_err(|e| AppError::Internal(format!("OIDC discovery failed: {e}")))?;

        Ok(Self {
            metadata,
            http_client,
            config,
        })
    }

    pub fn provider_name(&self) -> &str {
        &self.config.provider_name
    }

    pub fn auto_provision(&self) -> bool {
        self.config.auto_provision
    }

    pub fn authorization_url(&self) -> AppResult<OidcAuthorization> {
        let client = CoreClient::from_provider_metadata(
            self.metadata.clone(),
            ClientId::new(self.config.client_id.clone()),
            Some(ClientSecret::new(self.config.client_secret.clone())),
        )
        .set_redirect_uri(
            RedirectUrl::new(self.config.redirect_url.clone())
                .map_err(|e| AppError::Internal(format!("Invalid OIDC redirect URL: {e}")))?,
        );

        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
        let mut request = client
            .authorize_url(
                AuthenticationFlow::<CoreResponseType>::AuthorizationCode,
                CsrfToken::new_random,
                Nonce::new_random,
            )
            .set_pkce_challenge(pkce_challenge);
        for scope in &self.config.scopes {
            if scope != "openid" {
                request = request.add_scope(Scope::new(scope.clone()));
            }
        }
        let (url, state, nonce) = request.url();

        Ok(OidcAuthorization {
            url: url.to_string(),
            state: state.secret().to_string(),
            nonce: nonce.secret().to_string(),
            pkce_verifier: pkce_verifier.secret().to_string(),
        })
    }

    pub async fn exchange_code(
        &self,
        code: String,
        pkce_verifier: String,
        nonce: String,
    ) -> AppResult<OidcIdentity> {
        let client = CoreClient::from_provider_metadata(
            self.metadata.clone(),
            ClientId::new(self.config.client_id.clone()),
            Some(ClientSecret::new(self.config.client_secret.clone())),
        )
        .set_redirect_uri(
            RedirectUrl::new(self.config.redirect_url.clone())
                .map_err(|e| AppError::Internal(format!("Invalid OIDC redirect URL: {e}")))?,
        );

        let token = client
            .exchange_code(AuthorizationCode::new(code))
            .map_err(|_| AppError::Internal("OIDC provider has no token endpoint".to_string()))?
            .set_pkce_verifier(PkceCodeVerifier::new(pkce_verifier))
            .request_async(&self.http_client)
            .await
            .map_err(|e| {
                log::warn!("OIDC code exchange failed: {e}");
                AppError::Unauthorized("SSO authentication failed".to_string())
            })?;
        let id_token = token.extra_fields().id_token().ok_or_else(|| {
            AppError::Unauthorized("OIDC provider did not return an ID token".to_string())
        })?;
        let claims = id_token
            .claims(&client.id_token_verifier(), &Nonce::new(nonce))
            .map_err(|e| {
                log::warn!("OIDC ID token validation failed: {e}");
                AppError::Unauthorized("SSO identity token is invalid".to_string())
            })?;

        let email = claims
            .email()
            .map(|value| value.as_str().trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                AppError::Forbidden("SSO provider did not supply an email address".to_string())
            })?;

        if self.config.require_email_verified && claims.email_verified() != Some(true) {
            return Err(AppError::Forbidden(
                "SSO provider did not verify the email address".to_string(),
            ));
        }

        if !self.config.allowed_domains.is_empty() {
            let domain = email.rsplit_once('@').map(|(_, domain)| domain);
            if !domain.is_some_and(|domain| {
                self.config
                    .allowed_domains
                    .iter()
                    .any(|allowed| allowed == domain)
            }) {
                return Err(AppError::Forbidden(
                    "Email domain is not allowed to use SSO".to_string(),
                ));
            }
        }

        Ok(OidcIdentity {
            issuer: claims.issuer().as_str().to_string(),
            subject: claims.subject().as_str().to_string(),
            email,
        })
    }
}
