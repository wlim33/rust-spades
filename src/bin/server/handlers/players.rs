use axum::{http::StatusCode, response::Json};
use spades::validation::validate_player_name;
use tower_sessions::Session;
use uuid::Uuid;

use super::super::dto::{ErrorResponse, SessionPlayerResponse, SetDisplayNameRequest, UserSession};
use super::super::SESSION_USER_KEY;

pub async fn get_player(session: Session) -> Result<Json<SessionPlayerResponse>, StatusCode> {
    let user = match session
        .get::<UserSession>(SESSION_USER_KEY)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    {
        Some(user) => user,
        None => {
            let user = UserSession {
                user_id: Uuid::new_v4(),
                display_name: None,
            };
            session
                .insert(SESSION_USER_KEY, user.clone())
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            user
        }
    };
    Ok(Json(SessionPlayerResponse {
        user_id: user.user_id,
        display_name: user.display_name,
    }))
}

pub async fn set_display_name(
    session: Session,
    Json(request): Json<SetDisplayNameRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let mut user: UserSession = session
        .get::<UserSession>(SESSION_USER_KEY)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Session error".to_string(),
                }),
            )
        })?
        .ok_or((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "No session. Call GET /player first.".to_string(),
            }),
        ))?;

    let validated_name = match request.name {
        Some(raw) => Some(validate_player_name(&raw).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: e.to_string(),
                }),
            )
        })?),
        None => None,
    };

    user.display_name = validated_name;
    session
        .insert(SESSION_USER_KEY, user)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Session error".to_string(),
                }),
            )
        })?;
    Ok(StatusCode::NO_CONTENT)
}
