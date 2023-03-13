use std::{fmt::Display, sync::Arc};

use axum::{
    async_trait,
    extract::{FromRequest, FromRequestParts, Host, Path},
    http::{self, request::Parts, Request},
    response::{IntoResponse, Response},
    Extension, Json,
};
use http::header;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::json;

use crate::server::ControlServer;

pub type ApiResponse<D> = Result<Json<D>, ApiError>;

pub fn ok<D: Serialize>(data: D) -> ApiResponse<D> {
    Ok(Json(data))
}

#[derive(Debug)]
pub enum ApiError {
    Internal,
    NotAuthenticated,
    NotAuthorized,
    InvalidData(String),
    InvalidPathArg(String),
    InvalidQueryArg(String),
    ProcessNotFound,
    Custom {
        code: &'static str,
        message: Option<String>,
    },
}

impl ApiError {
    pub fn code(&self) -> &str {
        match self {
            ApiError::Internal => "internal",
            ApiError::NotAuthenticated => "unauthenticated",
            ApiError::NotAuthorized => "unauthorized",
            ApiError::InvalidData(_) => "invalid_data",
            ApiError::InvalidPathArg(_) => "invalid_path_arg",
            ApiError::InvalidQueryArg(_) => "invalid_query_arg",
            ApiError::ProcessNotFound => "process_not_found",
            ApiError::Custom { code, .. } => code,
        }
    }

    pub fn message(&self) -> String {
        match self {
            ApiError::Internal => "".into(),
            ApiError::NotAuthenticated => "Not authenticated".into(),
            ApiError::NotAuthorized => "Not authorized".into(),
            ApiError::ProcessNotFound => "No such process".into(),
            ApiError::InvalidData(msg) => msg.clone(),
            ApiError::InvalidPathArg(msg) => msg.clone(),
            ApiError::InvalidQueryArg(msg) => msg.clone(),
            ApiError::Custom { message, .. } => message.clone().unwrap_or_else(|| "".into()),
        }
    }

    #[allow(dead_code)]
    pub fn log_internal(msg: &str, e: impl std::fmt::Debug) -> Self {
        log::error!("{}: {:?}", msg, e);
        Self::Internal
    }

    pub fn custom(code: &'static str, message: String) -> Self {
        Self::Custom {
            code,
            message: Some(message),
        }
    }

    pub fn custom_code(code: &'static str) -> Self {
        Self::Custom {
            code,
            message: None,
        }
    }
}

impl Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Error ")?;
        f.write_str(self.code())?;
        let msg = self.message();
        if !msg.is_empty() {
            f.write_str(": ")?;
            f.write_str(&msg)?;
        }
        Ok(())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        use http::StatusCode as S;
        use ApiError::*;

        let body = Json(json!({
            "message": self.message(),
            "code": self.code(),
        }));

        let status = match self {
            Self::Internal => S::INTERNAL_SERVER_ERROR,
            Self::NotAuthenticated => S::UNAUTHORIZED,
            Self::NotAuthorized => S::FORBIDDEN,
            Self::ProcessNotFound => S::NOT_FOUND,
            InvalidData(_) | InvalidPathArg(_) | InvalidQueryArg(_) | Custom { .. } => {
                S::BAD_REQUEST
            }
        };

        (status, body).into_response()
    }
}

pub struct JsonExtractor<T>(pub T);

#[async_trait]
impl<S, B, T> FromRequest<S, B> for JsonExtractor<T>
where
    axum::Json<T>: FromRequest<S, B, Rejection = axum::extract::rejection::JsonRejection>,
    S: Send + Sync,
    B: Send + 'static,
{
    type Rejection = ApiError;

    async fn from_request(req: Request<B>, state: &S) -> Result<JsonExtractor<T>, Self::Rejection> {
        match Json::from_request(req, state).await {
            Ok(Json(value)) => Ok(JsonExtractor(value)),
            Err(e) => Err(ApiError::InvalidData(e.to_string())),
        }
    }
}

pub struct PathExtractor<T>(pub T);

#[async_trait]
impl<S, T> FromRequestParts<S> for PathExtractor<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Send,
{
    type Rejection = ApiError;

    async fn from_request_parts(
        req: &mut Parts,
        state: &S,
    ) -> Result<PathExtractor<T>, Self::Rejection> {
        match Path::from_request_parts(req, state).await {
            Ok(Path(value)) => Ok(PathExtractor(value)),
            Err(e) => Err(ApiError::InvalidPathArg(e.to_string())),
        }
    }
}

pub struct HostExtractor(pub String);

#[async_trait]
impl<S> FromRequestParts<S> for HostExtractor
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(
        req: &mut Parts,
        state: &S,
    ) -> Result<HostExtractor, Self::Rejection> {
        match Host::from_request_parts(req, state).await {
            Ok(Host(host)) => Ok(HostExtractor(host)),
            Err(e) => Err(ApiError::Custom {
                code: "no_host",
                message: Some(e.to_string()),
            }),
        }
    }
}

#[derive(Debug)]
pub struct NodeAuth {
    pub registration_id: i64,
    pub node_name: uuid::Uuid,
}

#[async_trait]
impl<S> FromRequestParts<S> for NodeAuth
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(req: &mut Parts, state: &S) -> Result<NodeAuth, Self::Rejection> {
        let headers = req.headers.clone();
        let auth_header = headers
            .get(header::AUTHORIZATION)
            .ok_or_else(|| {
                ApiError::custom("no_auth_header", "Missing node authorization header".into())
            })?
            .to_str()
            .map_err(|_| {
                ApiError::custom(
                    "invalid_auth_header",
                    "Invalid authorization header value".into(),
                )
            })?;

        let token = auth_header
            .strip_prefix("Bearer ")
            .to_owned()
            .ok_or_else(|| {
                ApiError::custom(
                    "invalid_auth_token",
                    "Header value doesn't start with Bearer".into(),
                )
            })?;

        let node_name = headers
            .get("x-lunatic-node-name")
            .ok_or_else(|| {
                ApiError::custom(
                    "no_lunatic_node_name_header",
                    "Missing x-lunatic-node-name header".into(),
                )
            })?
            .to_str()
            .map_err(|_| {
                ApiError::custom(
                    "invalid_lunatic_node_name_header",
                    "Invalid x-lunatic-node-name header value".into(),
                )
            })?;

        let node_name: uuid::Uuid = node_name.parse().map_err(|_| {
            ApiError::custom(
                "invalid_lunatic_node_name_header",
                format!("Invalid x-lunatic-node-name header: {node_name} not a valid UUID"),
            )
        })?;

        let cs: Extension<Arc<ControlServer>> = Extension::from_request_parts(req, state)
            .await
            .map_err(|e| ApiError::log_internal("Error getting cs in registration auth", e))?;

        let (registration_id, reg) = cs
            .registrations
            .iter()
            .find(|r| r.node_name == node_name && r.authentication_token == token)
            .map(|r| (*r.key(), r.value().clone()))
            .ok_or(ApiError::NotAuthenticated)?;
        let node_auth = NodeAuth {
            registration_id: registration_id as i64,
            node_name: reg.node_name,
        };

        Ok(node_auth)
    }
}
