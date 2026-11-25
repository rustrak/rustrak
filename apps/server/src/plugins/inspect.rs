//! What a plugin's code declares, read from the component without running
//! it: its routes from the `rustrak:routes` custom section the SDK's route
//! macros write, its database from whether it imports `sql`.

use wasmparser::{Parser, Payload};

use super::manifest::Route;

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct InspectError(String);

/// What the code declares.
#[derive(Debug, Default)]
pub struct Code {
    pub routes: Vec<Route>,
    /// It imports its database.
    pub sql: bool,
}

/// Reads `wasm`, a component built with the `rustrak-plugin` SDK.
pub fn inspect(wasm: &[u8]) -> Result<Code, InspectError> {
    let mut code = Code::default();
    for payload in Parser::new(0).parse_all(wasm) {
        let payload =
            payload.map_err(|e| InspectError(format!("not a Rustrak plugin component: {e}")))?;
        match payload {
            Payload::CustomSection(section) if section.name() == "rustrak:routes" => {
                let text = std::str::from_utf8(section.data())
                    .map_err(|e| InspectError(format!("rustrak:routes: {e}")))?;
                for line in text.lines().filter(|l| !l.is_empty()) {
                    let route = serde_json::from_str(line)
                        .map_err(|e| InspectError(format!("rustrak:routes: {e}: {line}")))?;
                    code.routes.push(route);
                }
            }
            Payload::ComponentImportSection(imports) => {
                for import in imports {
                    let import =
                        import.map_err(|e| InspectError(format!("component imports: {e}")))?;
                    code.sql |= import.name.0.strip_prefix("rustrak:plugin/sql@").is_some();
                }
            }
            _ => {}
        }
    }
    Ok(code)
}
