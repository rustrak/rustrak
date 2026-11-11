use actix_session::Session;
use actix_web::{dev::Payload, Error, FromRequest, HttpRequest};
use std::pin::Pin;

use crate::error::{AppError, AppResult};
use crate::models::User;
use crate::services::UsersService;

pub(crate) const SESSION_USER_ID_KEY: &str = "user_id";

/// Signs `user_id` in under a fresh session id, so an id planted before
/// sign-in is worthless after it (session fixation).
pub fn set_user_session(session: &Session, user_id: i32) -> AppResult<()> {
    session.renew();
    session
        .insert(SESSION_USER_ID_KEY, user_id)
        .map_err(|e| AppError::Internal(format!("Failed to set session: {}", e)))
}

pub fn get_user_id_from_session(session: &Session) -> Option<i32> {
    session.get::<i32>(SESSION_USER_ID_KEY).ok().flatten()
}

pub fn clear_session(session: &Session) {
    session.purge();
}

/// The signed-in user of a session cookie (session only, never a token).
pub struct AuthenticatedUser(pub User);

impl FromRequest for AuthenticatedUser {
    type Error = Error;
    type Future = Pin<Box<dyn std::future::Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let req = req.clone();
        Box::pin(async move {
            let session = Session::extract(&req)
                .await
                .map_err(|_| AppError::Unauthorized("Session error".to_string()))?;
            let user_id = get_user_id_from_session(&session)
                .ok_or_else(|| AppError::Unauthorized("Not authenticated".to_string()))?;
            let pool = crate::auth::extractors::pool(&req)?;
            let user = UsersService::get_by_id(pool.get_ref(), user_id)
                .await?
                .ok_or_else(|| AppError::Unauthorized("User not found".to_string()))?;
            user.ensure_active()?;
            Ok(AuthenticatedUser(user))
        })
    }
}
