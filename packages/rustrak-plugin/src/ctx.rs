use crate::__private::bindings::Request;
use crate::__private::bindings::rustrak::plugin::settings;
use crate::db::Db;
use crate::http::Error;

/// Everything a handler can reach: the request, the plugin's database and
/// its settings.
pub struct Ctx {
    pub(crate) request: Request,
}

impl Ctx {
    /// The plugin's own SQLite database.
    pub fn db(&self) -> Db {
        Db
    }

    /// The project the request is for, on project routes.
    pub fn project_id(&self) -> Option<u64> {
        self.request.project_id
    }

    /// The project the request is for; not found outside one.
    pub fn project(&self) -> Result<u64, Error> {
        self.project_id().ok_or(Error::NotFound)
    }

    /// The client's address as Rustrak saw it.
    pub fn client_ip(&self) -> Option<&str> {
        self.request.client_ip.as_deref()
    }

    /// A request header, case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.request
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// The plugin's settings, typed: `Settings::get(ctx)` from
    /// `rustrak_plugin::config!()` calls this. On a project route, the
    /// project's values, then the instance's, then the defaults.
    pub fn settings<T: serde::de::DeserializeOwned>(&self) -> Result<T, Error> {
        let json = match self.project_id() {
            Some(id) => settings::project(id),
            None => settings::instance(),
        };
        serde_json::from_str(&json)
            .map_err(|e| Error::Unavailable(format!("the plugin is not configured: {e}")))
    }
}
