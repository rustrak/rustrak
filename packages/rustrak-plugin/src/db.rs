use serde::de::DeserializeOwned;

use crate::__private::bindings::rustrak::plugin::sql;

pub use sql::Value;

/// A query failed: bad SQL, a constraint, a full quota, or the time budget.
#[derive(Debug, Clone)]
pub struct DbError(pub String);

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DbError {}

impl From<sql::SqlError> for DbError {
    fn from(e: sql::SqlError) -> Self {
        DbError(match e {
            sql::SqlError::Constraint(m) => format!("constraint failed: {m}"),
            sql::SqlError::DatabaseFull => "the plugin's storage is full".into(),
            sql::SqlError::Busy => "the database is busy".into(),
            sql::SqlError::Other(m) => m,
        })
    }
}

/// Anything that binds as a query parameter.
pub trait ToValue {
    fn to_value(&self) -> Value;
}

macro_rules! int {
    ($($t:ty),*) => {$(
        impl ToValue for $t {
            fn to_value(&self) -> Value {
                Value::Integer(*self as i64)
            }
        }
    )*};
}
int!(i8, i16, i32, i64, u8, u16, u32, u64, usize, isize);

impl ToValue for bool {
    fn to_value(&self) -> Value {
        Value::Integer(i64::from(*self))
    }
}

impl ToValue for f64 {
    fn to_value(&self) -> Value {
        Value::Real(*self)
    }
}

impl ToValue for &str {
    fn to_value(&self) -> Value {
        Value::Text((*self).to_string())
    }
}

impl ToValue for String {
    fn to_value(&self) -> Value {
        Value::Text(self.clone())
    }
}

impl ToValue for Vec<u8> {
    fn to_value(&self) -> Value {
        Value::Blob(self.clone())
    }
}

impl ToValue for Value {
    fn to_value(&self) -> Value {
        self.clone()
    }
}

impl<T: ToValue> ToValue for Option<T> {
    fn to_value(&self) -> Value {
        self.as_ref().map_or(Value::Null, ToValue::to_value)
    }
}

fn values(params: &[&dyn ToValue]) -> Vec<Value> {
    params.iter().map(|p| p.to_value()).collect()
}

fn json(v: Value) -> serde_json::Value {
    match v {
        Value::Null => serde_json::Value::Null,
        Value::Integer(i) => i.into(),
        Value::Real(f) => f.into(),
        Value::Text(s) => s.into(),
        Value::Blob(b) => b.into(),
    }
}

/// The plugin's own SQLite database. Write the SQL; rows come back as your
/// own types, matched to columns by name.
pub struct Db;

impl Db {
    /// Runs a statement and returns the rows it changed.
    pub fn execute(&self, sql: &str, params: &[&dyn ToValue]) -> Result<u64, DbError> {
        Ok(sql::execute(sql, &values(params))?)
    }

    /// Every row, each deserialized into `T` by column name.
    pub fn query<T: DeserializeOwned>(
        &self,
        sql: &str,
        params: &[&dyn ToValue],
    ) -> Result<Vec<T>, DbError> {
        let rows = sql::query(sql, &values(params))?;
        rows.rows
            .into_iter()
            .map(|row| {
                let object: serde_json::Map<String, serde_json::Value> = rows
                    .columns
                    .iter()
                    .cloned()
                    .zip(row.into_iter().map(json))
                    .collect();
                serde_json::from_value(object.into()).map_err(|e| DbError(e.to_string()))
            })
            .collect()
    }

    /// The first column of the first row.
    pub fn scalar<T: DeserializeOwned>(
        &self,
        sql: &str,
        params: &[&dyn ToValue],
    ) -> Result<T, DbError> {
        let rows = sql::query(sql, &values(params))?;
        let first = rows
            .rows
            .into_iter()
            .next()
            .and_then(|r| r.into_iter().next())
            .ok_or_else(|| DbError("the query returned no rows".into()))?;
        serde_json::from_value(json(first)).map_err(|e| DbError(e.to_string()))
    }

    /// Several statements in one transaction: all of them or none.
    pub fn batch(&self, statements: &[(&str, &[&dyn ToValue])]) -> Result<(), DbError> {
        let statements: Vec<(String, Vec<Value>)> = statements
            .iter()
            .map(|(s, p)| (s.to_string(), values(p)))
            .collect();
        Ok(sql::batch(&statements)?)
    }
}
