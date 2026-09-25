use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::error::AppError;

/// Instance-wide role for a user.
///
/// `Admin` has full access to every project and to team management.
/// `Member` only sees projects they are a member of (see `ProjectRole`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    Admin,
    Member,
}

impl UserRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            UserRole::Admin => "admin",
            UserRole::Member => "member",
        }
    }

    /// Lenient parse from the DB string. Unknown values fall back to the
    /// least-privileged role (`Member`) so a bad value never grants admin.
    pub fn from_db(s: &str) -> Self {
        match s {
            "admin" => UserRole::Admin,
            _ => UserRole::Member,
        }
    }

    /// Strict parse for untrusted input (request bodies). Unknown → `None`.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "admin" => Some(UserRole::Admin),
            "member" => Some(UserRole::Member),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct User {
    pub id: i32,
    pub email: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub is_active: bool,
    pub role: String,
    pub created_at: DateTime<Utc>,
    pub last_login: Option<DateTime<Utc>>,
    /// The dashboard language this user chose, or `None` if they never have.
    ///
    /// `None` is not "English": it is what lets a consumer fall back to the
    /// reader's `Accept-Language` instead of forcing a default on someone who
    /// never opened the setting.
    pub language: Option<String>,
    /// The IANA timezone this user chose, or `None` if they never have.
    pub timezone: Option<String>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateUserRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

impl User {
    /// Canonical form for storage: trimmed, ASCII letters lowercased.
    ///
    /// ASCII-only so it agrees with SQLite's `LOWER()`, which the lookup in
    /// `UsersService::get_by_email` relies on.
    pub fn normalize_email(email: &str) -> String {
        email.trim().to_ascii_lowercase()
    }

    /// Hash a password using Argon2id
    pub fn hash_password(password: &str) -> Result<String, AppError> {
        let hash = Argon2::default()
            .hash_password(password.as_bytes())
            .map_err(|e| AppError::Internal(format!("Password hashing failed: {}", e)))?;
        Ok(hash.to_string())
    }

    /// Verify a password against the stored hash
    pub fn verify_password(&self, password: &str) -> Result<bool, AppError> {
        let parsed_hash = PasswordHash::new(&self.password_hash)
            .map_err(|e| AppError::Internal(format!("Invalid password hash: {}", e)))?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok())
    }

    /// Typed global role.
    pub fn role(&self) -> UserRole {
        UserRole::from_db(&self.role)
    }

    /// Convenience: is this user an instance admin?
    pub fn is_admin(&self) -> bool {
        matches!(self.role(), UserRole::Admin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_role_roundtrips_through_db_string() {
        assert_eq!(UserRole::from_db(UserRole::Admin.as_str()), UserRole::Admin);
        assert_eq!(
            UserRole::from_db(UserRole::Member.as_str()),
            UserRole::Member
        );
    }

    #[test]
    fn unknown_role_falls_back_to_member() {
        assert_eq!(UserRole::from_db("superuser"), UserRole::Member);
        assert_eq!(UserRole::from_db(""), UserRole::Member);
    }

    #[test]
    fn email_normalizes_to_trimmed_lowercase() {
        assert_eq!(
            User::normalize_email("User@Example.com"),
            "user@example.com"
        );
        assert_eq!(
            User::normalize_email("  USER@EXAMPLE.COM  "),
            "user@example.com"
        );
    }

    #[test]
    fn email_normalization_folds_ascii_only() {
        // Matches SQLite's LOWER(), which leaves non-ASCII letters untouched.
        assert_eq!(
            User::normalize_email("Üser@Example.com"),
            "Üser@example.com"
        );
    }

    fn user_with_hash(password_hash: &str) -> User {
        User {
            id: 1,
            email: "a@example.com".into(),
            password_hash: password_hash.into(),
            is_active: true,
            role: "member".into(),
            created_at: Utc::now(),
            last_login: None,
            language: None,
            timezone: None,
        }
    }

    #[test]
    fn password_roundtrips_through_hash_and_verify() {
        let hash = User::hash_password("hunter42").unwrap();
        assert!(hash.starts_with("$argon2id$v=19$"));
        let user = user_with_hash(&hash);
        assert!(user.verify_password("hunter42").unwrap());
        assert!(!user.verify_password("hunter43").unwrap());
    }

    #[test]
    fn hashes_written_by_argon2_0_5_still_verify() {
        // Produced with argon2 0.5.3 for "correct horse battery staple".
        let legacy = "$argon2id$v=19$m=19456,t=2,p=1$a3PYmB2M+IrrehO7MdwvLA$ctbNYOFfMLfV8AS53fAY+IiSE6FSVQ8f6f20o5C5pH4";
        let user = user_with_hash(legacy);
        assert!(user
            .verify_password("correct horse battery staple")
            .unwrap());
        assert!(!user.verify_password("wrong").unwrap());
    }

    #[test]
    fn malformed_stored_hash_is_an_internal_error() {
        let user = user_with_hash("not-a-phc-string");
        assert!(matches!(
            user.verify_password("anything"),
            Err(AppError::Internal(_))
        ));
    }
}
