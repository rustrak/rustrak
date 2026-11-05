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
    /// Also email the invite link to the recipient. Needs an enabled email
    /// integration or SMTP_HOST, and DASHBOARD_URL or PUBLIC_URL for the link.
    #[serde(default)]
    send_email: bool,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct InvitationArgs {
    /// Invitation id, as `list_invitations` returns it
    id: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct MemberArgs {
    /// Project ID
    project_id: i32,
    /// User ID to add or update
    user_id: i32,
    /// The user's roles on this project, by role id (see `GET /api/roles`).
    /// Replaces the roles they hold there; empty removes them.
    role_ids: Vec<i32>,
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
        description = "Invite a person by email with a global role. By default returns a single-use, expiring invite token to share as the link `<app-url>/invite/<token>`. With send_email=true the link is also emailed to the recipient, which needs an enabled email integration or SMTP_HOST, and DASHBOARD_URL or PUBLIC_URL. Requires invitations:create.",
        annotations(destructive_hint = false)
    )]
    async fn create_invitation(&self, Parameters(a): Parameters<InviteArgs>) -> CallToolResult {
        reply(async {
            invitations::create_invitation(
                self.pool(),
                self.actor(),
                self.config(),
                web::Json(args(&a)?),
            )
            .await
        })
        .await
    }

    #[tool(
        description = "List invitations (pending, accepted, revoked) with their ids, roles and expiry, and the token of each one the caller could have created.",
        annotations(read_only_hint = true)
    )]
    async fn list_invitations(&self, Parameters(_): Parameters<NoArgs>) -> CallToolResult {
        reply(invitations::list_invitations(self.pool(), self.actor())).await
    }

    #[tool(
        description = "Revoke a pending invitation by its id so it can no longer be accepted. Requires invitations:delete.",
        annotations(destructive_hint = true)
    )]
    async fn revoke_invitation(&self, Parameters(a): Parameters<InvitationArgs>) -> CallToolResult {
        done(
            invitations::revoke_invitation(self.pool(), self.actor(), web::Path::from(a.id)),
            "Invitation revoked.".to_string(),
        )
        .await
    }

    #[tool(
        description = "Email the invite link of a pending invitation again. Fails when invitation emails are not configured or the send fails. Requires invitations:create.",
        annotations(destructive_hint = false)
    )]
    async fn resend_invitation(&self, Parameters(a): Parameters<InvitationArgs>) -> CallToolResult {
        reply(invitations::resend_invitation(
            self.pool(),
            self.actor(),
            self.config(),
            web::Path::from(a.id),
        ))
        .await
    }

    #[tool(
        description = "List the members of a project with the roles they hold on it. Requires the project-members:read permission on the project.",
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
        description = "Set the roles a user holds on a project, by role id (see `GET /api/roles`). Requires the project-members:update permission, only grants roles within the caller's own permissions, and never changes the caller's own membership.",
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
                "User {} now holds roles {:?} on project {}.",
                a.user_id, a.role_ids, a.project_id
            ),
        )
        .await
    }

    #[tool(
        description = "Remove every role a user holds on a project. Requires the project-members:update permission; the caller cannot remove themselves.",
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
