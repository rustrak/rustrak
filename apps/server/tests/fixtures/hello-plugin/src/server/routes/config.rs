use rustrak_plugin::prelude::*;
use serde::Serialize;

use crate::Settings;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Seen {
    greeting: String,
    max_path_length: i64,
    has_token: bool,
}

/// What this project's settings are, as the plugin reads them: the secret
/// only as whether it is set.
#[get("/config")]
fn config(ctx: &Ctx) -> Result<Json<Seen>> {
    let s = Settings::get(ctx)?;
    Ok(Json(Seen {
        greeting: s.greeting,
        max_path_length: s.max_path_length,
        has_token: s.token.is_some(),
    }))
}
