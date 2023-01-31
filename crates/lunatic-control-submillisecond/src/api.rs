use std::fmt::Debug;
use std::fmt::Display;

use lunatic::process::ProcessRef;
use lunatic_log::error;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::json;
use submillisecond::{
    extract::FromRequest,
    http::{self, header},
    response::{IntoResponse, Response},
    state::State,
    RequestContext,
};

use crate::server::ControlServer;

pub type ApiResponse<D> = Result<submillisecond::Json<D>, ApiError>;

pub fn ok<D: Serialize>(data: D) -> ApiResponse<D> {
    Ok(submillisecond::Json(data))
}

#[derive(Debug)]
pub enum ApiError {
    Internal,
    NotAuthenticated,
    NotAuthorized,
    InvalidData(String),
    InvalidPathArg(String),
    InvalidQueryArg(String),
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
            ApiError::Custom { code, .. } => code,
        }
    }

    pub fn message(&self) -> Option<&str> {
        match self {
            ApiError::Internal => None,
            ApiError::NotAuthenticated => Some("Not authenticated"),
            ApiError::NotAuthorized => Some("Not authorized"),
            ApiError::InvalidData(msg) => Some(msg),
            ApiError::InvalidPathArg(msg) => Some(msg),
            ApiError::InvalidQueryArg(msg) => Some(msg),
            ApiError::Custom { message, .. } => message.as_deref(),
        }
    }

    pub fn log_internal(msg: &str) -> Self {
        error!("{msg}");
        Self::Internal
    }

    pub fn log_internal_err(msg: &str, err: impl std::fmt::Debug) -> Self {
        error!("{msg}: {err:?}");
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
        if let Some(msg) = msg {
            f.write_str(": ")?;
            f.write_str(msg)?;
        }
        Ok(())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        use http::StatusCode as S;
        use ApiError::*;

        let body = submillisecond::Json(json!({
            "message": self.message(),
            "code": self.code(),
        }));

        let status = match self {
            Self::Internal => S::INTERNAL_SERVER_ERROR,
            Self::NotAuthenticated => S::UNAUTHORIZED,
            Self::NotAuthorized => S::FORBIDDEN,
            InvalidData(_) | InvalidPathArg(_) | InvalidQueryArg(_) | Custom { .. } => {
                S::BAD_REQUEST
            }
        };

        (status, body).into_response()
    }
}

pub struct ControlServerExtractor(pub ProcessRef<ControlServer>);

impl FromRequest for ControlServerExtractor {
    type Rejection = ApiError;

    fn from_request(_req: &mut RequestContext) -> Result<Self, Self::Rejection> {
        ControlServer::lookup()
            .map(ControlServerExtractor)
            .ok_or_else(|| ApiError::log_internal("ControlServer lookup not found"))
    }
}

pub struct JsonExtractor<T>(pub T);

impl<T> FromRequest for JsonExtractor<T>
where
    T: DeserializeOwned + Debug,
{
    type Rejection = ApiError;

    fn from_request(req: &mut RequestContext) -> Result<Self, Self::Rejection> {
        match submillisecond::Json::from_request(req) {
            Ok(submillisecond::Json(value)) => Ok(JsonExtractor(value)),
            Err(err) => Err(ApiError::InvalidData(err.to_string())),
        }
    }
}

pub struct PathExtractor<T>(pub T);

impl<T> FromRequest for PathExtractor<T>
where
    T: DeserializeOwned,
{
    type Rejection = ApiError;

    fn from_request(req: &mut RequestContext) -> Result<Self, Self::Rejection> {
        match submillisecond::extract::Path::from_request(req) {
            Ok(submillisecond::extract::Path(value)) => Ok(PathExtractor(value)),
            Err(err) => Err(ApiError::InvalidPathArg(err.to_string())),
        }
    }
}

pub struct HostExtractor(pub String);

impl FromRequest for HostExtractor {
    type Rejection = ApiError;

    fn from_request(req: &mut RequestContext) -> Result<Self, Self::Rejection> {
        match submillisecond::extract::Host::from_request(req) {
            Ok(submillisecond::extract::Host(value)) => Ok(HostExtractor(value)),
            Err(err) => Err(ApiError::Custom {
                code: "no_host",
                message: Some(err.to_string()),
            }),
        }
    }
}

#[derive(Debug)]
pub struct NodeAuth {
    pub registration_id: i64,
    pub node_name: uuid::Uuid,
}

impl FromRequest for NodeAuth {
    type Rejection = ApiError;

    fn from_request(req: &mut RequestContext) -> Result<Self, Self::Rejection> {
        let headers = req.headers();
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

        todo!()

        // let cs: State<ControlServer> = State::from_request(req)
        //     .map_err(|e| ApiError::log_internal("Error getting cs in registration auth", e))?;

        // let (registration_id, reg) = cs
        //     .registrations
        //     .iter()
        //     .find(|r| r.node_name == node_name && r.authentication_token == token)
        //     .map(|r| (*r.key(), r.value().clone()))
        //     .ok_or(ApiError::NotAuthenticated)?;
        // let node_auth = NodeAuth {
        //     registration_id: registration_id as i64,
        //     node_name: reg.node_name,
        // };

        // Ok(node_auth)
    }
}
