use actix_web::web;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::schemars::{self, JsonSchema};
use rmcp::{tool, tool_router};
use serde::{Deserialize, Serialize};

use super::{done, reply, NoArgs, RustrakMcp};
use crate::routes::tokens;

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct TokenArgs {
    /// Token ID
    token_id: i32,
}

#[tool_router(router = token_tools, vis = "pub(super)")]
impl RustrakMcp {
    #[tool(
        description = "List all API tokens. Token values are masked — only the prefix is shown.",
        annotations(read_only_hint = true)
    )]
    async fn list_tokens(&self, Parameters(_): Parameters<NoArgs>) -> CallToolResult {
        reply(tokens::list_tokens(self.pool(), self.actor())).await
    }

    #[tool(
        description = "Get a token by ID: its prefix, scope and expiry. The value itself is stored hashed and only ever shown on creation.",
        annotations(read_only_hint = true)
    )]
    async fn get_token(&self, Parameters(a): Parameters<TokenArgs>) -> CallToolResult {
        reply(tokens::get_token(
            self.pool(),
            self.actor(),
            web::Path::from(a.token_id),
        ))
        .await
    }

    #[tool(
        description = "Permanently revoke an API token. This action cannot be undone.",
        annotations(destructive_hint = true)
    )]
    async fn revoke_token(&self, Parameters(a): Parameters<TokenArgs>) -> CallToolResult {
        done(
            tokens::delete_token(self.pool(), self.actor(), web::Path::from(a.token_id)),
            format!("Token {} revoked successfully.", a.token_id),
        )
        .await
    }
}
