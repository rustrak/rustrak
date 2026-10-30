//! The global user roster and roles, invitations, and per-project membership.
//! Team and invitation tools need an instance admin; project member tools a
//! global admin or an admin of that project.

use actix_web::web;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::schemars::{self, JsonSchema};
use rmcp::{tool, tool_router};
use serde::{Deserialize, Serialize};

use super::{args, done, reply, NoArgs, ProjectArgs, RustrakMcp};
use crate::routes::{invitations, members, team};

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct RoleArgs {
    /// User ID to update
    user_id: i32,
    /// New global role: admin or member
    role: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct UserArgs {
    /// User ID to remove
    user_id: i32,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct InviteArgs {
    /// Email address of the invitee
    email: String,
    /// Global role the invited user will receive: admin or member
    role: String,
    /// Email the invite link to the recipient instead of returning it for
    /// manual sharing. Requires an enabled email integration or global SMTP
    /// configuration; the call fails with a field error when unconfigured.
    #[serde(default)]
    send_email: bool,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct InvitationArgs {
    /// Invitation token to revoke
    token: String,
}

/// Arguments for re-sending an invitation email.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ResendInvitationArgs {
    /// Invitation token to re-send
    token: String,
    /// Re-send even when an email was already delivered (e.g. it landed in
    /// spam and the recipient asks for a fresh copy)
    force: Option<bool>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct MemberArgs {
    /// Project ID
    project_id: i32,
    /// User ID to add or update
    user_id: i32,
    /// Per-project role to assign: viewer, editor or admin
    role: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct RemoveMemberArgs {
    /// Project ID
    project_id: i32,
    /// User ID to remove from the project
    user_id: i32,
}

#[tool_router(router = team_tools, vis = "pub(super)")]
impl RustrakMcp {
    #[tool(
        description = "List all users on this Rustrak instance with their global role (admin/member) and status. Requires an admin token.",
        annotations(read_only_hint = true)
    )]
    async fn list_team_members(&self, Parameters(_): Parameters<NoArgs>) -> CallToolResult {
        reply(team::list_team(self.pool(), self.actor())).await
    }

    #[tool(
        description = "Change a user's global role. Admins manage the team and all projects; members only see projects they belong to. The primary user and the last admin cannot be demoted. Requires an admin token.",
        annotations(destructive_hint = false)
    )]
    async fn update_member_role(&self, Parameters(a): Parameters<RoleArgs>) -> CallToolResult {
        done(
            async {
                team::update_team_role(
                    self.pool(),
                    self.actor(),
                    web::Path::from(a.user_id),
                    web::Json(args(&a)?),
                )
                .await
            },
            format!("User {} is now {}.", a.user_id, a.role),
        )
        .await
    }

    #[tool(
        description = "Permanently remove a user from the instance, revoking their access and tokens. The primary user, yourself, and the last admin cannot be removed. Requires an admin token.",
        annotations(destructive_hint = true)
    )]
    async fn remove_team_member(&self, Parameters(a): Parameters<UserArgs>) -> CallToolResult {
        done(
            team::delete_user(self.pool(), self.actor(), web::Path::from(a.user_id)),
            format!("User {} removed.", a.user_id),
        )
        .await
    }

    #[tool(
        description = "Invite a person by email with a global role. By default returns a single-use, expiring invite token to share as the link `<app-url>/invite/<token>`. With send_email=true the link is emailed to the recipient instead (their mailbox receiving it is the address verification); email delivery must be configured via an enabled email integration or the global SMTP_* environment. Requires an admin token.",
        annotations(destructive_hint = false)
    )]
    async fn create_invitation(&self, Parameters(a): Parameters<InviteArgs>) -> CallToolResult {
        reply(async {
            let mut create_args: crate::models::CreateInvitation = args(&a)?;
            create_args.send_email = a.send_email;
            invitations::create_invitation(
                self.pool(),
                self.actor(),
                self.config(),
                web::Json(create_args),
            )
            .await
        })
        .await
    }

    #[tool(
        description = "List invitations (pending, accepted, revoked) with their tokens, roles and expiry. Requires an admin token.",
        annotations(read_only_hint = true)
    )]
    async fn list_invitations(&self, Parameters(_): Parameters<NoArgs>) -> CallToolResult {
        reply(invitations::list_invitations(self.pool(), self.actor())).await
    }

    #[tool(
        description = "Revoke a pending invitation by its token so it can no longer be accepted. Requires an admin token.",
        annotations(destructive_hint = true)
    )]
    async fn revoke_invitation(&self, Parameters(a): Parameters<InvitationArgs>) -> CallToolResult {
        done(
            invitations::revoke_invitation(self.pool(), self.actor(), web::Path::from(a.token)),
            "Invitation revoked.".to_string(),
        )
        .await
    }

    #[tool(
        description = "Re-send the invitation email for a pending invitation. Needs force=true when an email was already delivered. Fails when email delivery is not configured or the SMTP send fails. Requires an admin token.",
        annotations(destructive_hint = false)
    )]
    async fn resend_invitation(
        &self,
        Parameters(a): Parameters<ResendInvitationArgs>,
    ) -> CallToolResult {
        done(
            invitations::resend_invitation(
                self.pool(),
                self.actor(),
                self.config(),
                web::Path::from(a.token),
                Some(web::Json(crate::routes::invitations::ResendInvitation {
                    force: a.force.unwrap_or(false),
                })),
            ),
            "Invitation email sent.".to_string(),
        )
        .await
    }

    #[tool(
        description = "List the members of a project with their per-project role (viewer/editor/admin). Requires global admin or project-admin access.",
        annotations(read_only_hint = true)
    )]
    async fn list_project_members(&self, Parameters(a): Parameters<ProjectArgs>) -> CallToolResult {
        reply(members::list_members(
            self.pool(),
            self.actor(),
            web::Path::from(a.project_id),
        ))
        .await
    }

    #[tool(
        description = "Add a user to a project or change their per-project role. viewer = read issues/events; editor = also resolve/delete issues and update the project; admin = also delete the project and manage its members. Requires global admin or project-admin access.",
        annotations(destructive_hint = false)
    )]
    async fn set_project_member(&self, Parameters(a): Parameters<MemberArgs>) -> CallToolResult {
        done(
            async {
                members::upsert_member(
                    self.pool(),
                    self.actor(),
                    web::Path::from(a.project_id),
                    web::Json(args(&a)?),
                )
                .await
            },
            format!(
                "User {} is now {} on project {}.",
                a.user_id, a.role, a.project_id
            ),
        )
        .await
    }

    #[tool(
        description = "Remove a user's access to a project. The last project admin cannot be removed. Requires global admin or project-admin access.",
        annotations(destructive_hint = true)
    )]
    async fn remove_project_member(
        &self,
        Parameters(a): Parameters<RemoveMemberArgs>,
    ) -> CallToolResult {
        done(
            members::remove_member(
                self.pool(),
                self.actor(),
                web::Path::from((a.project_id, a.user_id)),
            ),
            format!("User {} removed from project {}.", a.user_id, a.project_id),
        )
        .await
    }
}
