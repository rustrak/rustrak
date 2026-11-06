---
"@rustrak/server": minor
---

Add roles built from granular permissions, custom permissions per member and a member page to manage them. API tokens are stored hashed and can be limited to permissions, projects and an expiry date. Invitations can carry a role and a scope and can be sent by email (@LiJoeAllen). The login page is redesigned. Breaking: creating a project now takes `projects:create` and no longer makes the creator its admin, creating a token takes `api-tokens:create` and a signed-in session, the `create_token` MCP tool is gone, `PUT /api/projects/{id}/members` takes `role_ids`, and `DELETE /api/invitations/{id}` takes the invitation id. See the upgrade guide at https://rustrak.dev/upgrading/0-25.
