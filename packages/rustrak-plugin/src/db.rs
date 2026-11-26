use serde::de::value::{Error as DeError, MapDeserializer, SeqDeserializer};
use serde::de::{self, DeserializeOwned, IntoDeserializer};

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
                Value::Integer(i64::from(*self))
            }
        }
    )*};
}
int!(i8, i16, i32, i64, u8, u16, u32);

// SQLite's integers are i64: a larger value panics, which fails the request,
// rather than being stored as a different number.
macro_rules! wide {
    ($($t:ty),*) => {$(
        impl ToValue for $t {
            fn to_value(&self) -> Value {
                Value::Integer(
                    i64::try_from(*self)
                        .expect("an integer above i64::MAX does not fit in SQLite"),
                )
            }
        }
    )*};
}
wide!(u64, usize, isize);

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

/// One column's value, read the way SQLite stores it: a `bool` from 0 or 1.
struct Cell(Value);

impl<'de> de::Deserializer<'de> for Cell {
    type Error = DeError;

    fn deserialize_any<V: de::Visitor<'de>>(self, v: V) -> Result<V::Value, DeError> {
        match self.0 {
            Value::Null => v.visit_unit(),
            Value::Integer(i) => v.visit_i64(i),
            Value::Real(f) => v.visit_f64(f),
            Value::Text(s) => v.visit_string(s),
            Value::Blob(b) => v.visit_seq(SeqDeserializer::new(b.into_iter())),
        }
    }

    fn deserialize_bool<V: de::Visitor<'de>>(self, v: V) -> Result<V::Value, DeError> {
        match self.0 {
            Value::Integer(i) => v.visit_bool(i != 0),
            other => Cell(other).deserialize_any(v),
        }
    }

    fn deserialize_option<V: de::Visitor<'de>>(self, v: V) -> Result<V::Value, DeError> {
        if matches!(self.0, Value::Null) {
            v.visit_none()
        } else {
            v.visit_some(self)
        }
    }

    fn deserialize_newtype_struct<V: de::Visitor<'de>>(
        self,
        _: &'static str,
        v: V,
    ) -> Result<V::Value, DeError> {
        v.visit_newtype_struct(self)
    }

    fn deserialize_enum<V: de::Visitor<'de>>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        v: V,
    ) -> Result<V::Value, DeError> {
        match self.0 {
            Value::Text(s) => s.into_deserializer().deserialize_enum(name, variants, v),
            other => Cell(other).deserialize_any(v),
        }
    }

    serde::forward_to_deserialize_any! {
        i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf unit unit_struct seq tuple tuple_struct map struct
        identifier ignored_any
    }
}

impl IntoDeserializer<'_, DeError> for Cell {
    type Deserializer = Self;

    fn into_deserializer(self) -> Self {
        self
    }
}

/// A row as `T`, its fields matched to columns by name.
fn from_row<T: DeserializeOwned>(columns: &[String], row: Vec<Value>) -> Result<T, DbError> {
    let cells = columns.iter().cloned().zip(row.into_iter().map(Cell));
    T::deserialize(MapDeserializer::<_, DeError>::new(cells)).map_err(|e| DbError(e.to_string()))
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
            .map(|row| from_row(&rows.columns, row))
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
        T::deserialize(Cell(first)).map_err(|e| DbError(e.to_string()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize as _;

    #[derive(serde::Deserialize, Debug, PartialEq)]
    enum Kind {
        #[serde(rename = "a")]
        A,
    }

    #[derive(serde::Deserialize, Debug, PartialEq)]
    struct Id(i64);

    #[derive(serde::Deserialize, Debug, PartialEq)]
    struct Row {
        on: bool,
        off: bool,
        n: Option<i64>,
        none: Option<String>,
        bytes: Vec<u8>,
        ratio: f64,
        kind: Kind,
        id: Id,
    }

    #[test]
    fn a_row_reads_a_bool_from_the_integer_sqlite_stores() {
        let columns = ["on", "off", "n", "none", "bytes", "ratio", "kind", "id"].map(String::from);
        let row = vec![
            true.to_value(),
            false.to_value(),
            Value::Integer(7),
            Value::Null,
            Value::Blob(vec![1, 2]),
            Value::Integer(2),
            Value::Text("a".into()),
            Value::Integer(9),
        ];
        let row: Row = from_row(&columns, row).unwrap();
        assert_eq!(
            row,
            Row {
                on: true,
                off: false,
                n: Some(7),
                none: None,
                bytes: vec![1, 2],
                ratio: 2.0,
                kind: Kind::A,
                id: Id(9),
            }
        );
    }

    #[test]
    fn a_scalar_reads_a_bool_too() {
        assert!(bool::deserialize(Cell(Value::Integer(1))).unwrap());
    }

    #[test]
    #[should_panic(expected = "i64::MAX")]
    fn an_unsigned_value_sqlite_cannot_hold_is_refused() {
        u64::MAX.to_value();
    }
}
