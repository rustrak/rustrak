use actix_web::web;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::{tool, tool_router};

use super::{reply, ProjectArgs, RustrakMcp};
use crate::models::CreateProject;
use crate::routes::projects;

#[tool_router(router = project_tools, vis = "pub(super)")]
impl RustrakMcp {
    #[tool(
        description = "List all Rustrak projects you have access to.",
        annotations(read_only_hint = true)
    )]
    async fn list_projects(
        &self,
        Parameters(a): Parameters<projects::ListProjectsQuery>,
    ) -> CallToolResult {
        reply(async {
            projects::list_projects(self.pool(), self.config(), web::Query(a), self.actor()).await
        })
        .await
    }

    #[tool(
        description = "Get details for a single Rustrak project.",
        annotations(read_only_hint = true)
    )]
    async fn get_project(&self, Parameters(a): Parameters<ProjectArgs>) -> CallToolResult {
        reply(projects::get_project(
            self.pool(),
            self.config(),
            web::Path::from(a.project_id),
            self.actor(),
        ))
        .await
    }

    #[tool(
        description = "Create a new Rustrak project.",
        annotations(destructive_hint = false)
    )]
    async fn create_project(&self, Parameters(a): Parameters<CreateProject>) -> CallToolResult {
        reply(async {
            projects::create_project(self.pool(), self.config(), Ok(web::Json(a)), self.actor())
                .await
        })
        .await
    }
}
