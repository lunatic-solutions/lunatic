use std::{collections::HashMap, sync::Arc};

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Query},
    routing::{get, post},
    Extension, Json, Router,
};
use lunatic_control::{api::*, NodeInfo};
use lunatic_distributed::control::cert::TEST_ROOT_CERT;
use rcgen::CertificateSigningRequest;
use tower_http::limit::RequestBodyLimitLayer;

use crate::{
    api::{ok, ApiError, ApiResponse, HostExtractor, JsonExtractor, NodeAuth, PathExtractor},
    server::ControlServer,
};

pub async fn register(
    control: Extension<Arc<ControlServer>>,
    HostExtractor(host): HostExtractor,
    JsonExtractor(reg): JsonExtractor<Register>,
) -> ApiResponse<Registration> {
    log::info!("Registration for node name {}", reg.node_name);

    let control = control.as_ref();
    let cert_pem = CertificateSigningRequest::from_pem(&reg.csr_pem)
        .and_then(|sign_request| sign_request.serialize_pem_with_signer(&control.ca_cert))
        .map_err(|e| ApiError::custom("sign_error", e.to_string()))?;

    let mut authentication_token = [0u8; 32];
    getrandom::getrandom(&mut authentication_token)
        .map_err(|e| ApiError::log_internal("Error generating random token for registration", e))?;
    let authentication_token = base64_url::encode(&authentication_token);

    control.register(&reg, &cert_pem, &authentication_token);

    ok(Registration {
        node_name: reg.node_name,
        cert_pem_chain: vec![cert_pem],
        authentication_token,
        root_cert: TEST_ROOT_CERT.into(),
        urls: ControlUrls {
            api_base: format!("http://{host}/"),
            nodes: format!("http://{host}/nodes"),
            node_started: format!("http://{host}/started"),
            node_stopped: format!("http://{host}/stopped"),
            get_module: format!("http://{host}/module/{{id}}"),
            add_module: format!("http://{host}/module"),
            get_nodes: format!("http://{host}/nodes"),
        },
        envs: Vec::new(),
        is_privileged: true,
    })
}

pub async fn node_stopped(
    node_auth: NodeAuth,
    control: Extension<Arc<ControlServer>>,
) -> ApiResponse<()> {
    log::info!("Node {} stopped", node_auth.node_name);

    let control = control.as_ref();
    control.stop_node(node_auth.registration_id as u64);

    ok(())
}

pub async fn node_started(
    node_auth: NodeAuth,
    control: Extension<Arc<ControlServer>>,
    Json(data): Json<NodeStart>,
) -> ApiResponse<NodeStarted> {
    let control = control.as_ref();
    control.stop_node(node_auth.registration_id as u64);

    let (node_id, _node_address) = control.start_node(node_auth.registration_id as u64, data);

    log::info!("Node {} started with id {}", node_auth.node_name, node_id);

    // TODO spawn all modules on node

    ok(NodeStarted {
        node_id: node_id as i64,
    })
}

pub async fn list_nodes(
    _node_auth: NodeAuth,
    Query(query): Query<HashMap<String, String>>,
    control: Extension<Arc<ControlServer>>,
) -> ApiResponse<NodesList> {
    let control = control.as_ref();
    let nds: Vec<_> = control
        .nodes
        .iter()
        .filter(|n| n.status < 2 && !n.node_address.is_empty())
        .collect();
    // Filter nodes based on query params and node attributes
    let nds: Vec<_> = if !query.is_empty() {
        nds.into_iter()
            .filter(|node| query.iter().all(|(k, v)| node.attributes.get(k) == Some(v)))
            .collect()
    } else {
        nds
    };
    let nodes: Vec<_> = control
        .registrations
        .iter()
        .filter_map(|r| {
            nds.iter()
                .find(|n| n.registration_id == *r.key())
                .map(|n| NodeInfo {
                    id: *n.key(),
                    address: n.node_address.parse().unwrap(),
                    name: r.node_name.to_string(),
                })
        })
        .collect();

    ok(NodesList { nodes })
}

pub async fn add_module(
    node_auth: NodeAuth,
    control: Extension<Arc<ControlServer>>,
    body: Bytes,
) -> ApiResponse<ModuleId> {
    log::info!("Node {} add_module", node_auth.node_name);

    let control = control.as_ref();
    let module_id = control.add_module(body.to_vec());
    ok(ModuleId { module_id })
}

pub async fn get_module(
    node_auth: NodeAuth,
    PathExtractor(id): PathExtractor<u64>,
    control: Extension<Arc<ControlServer>>,
) -> ApiResponse<ModuleBytes> {
    log::info!("Node {} get_module {}", node_auth.node_name, id);

    let bytes = control
        .modules
        .iter()
        .find(|m| m.key() == &id)
        .map(|m| m.value().clone())
        .ok_or_else(|| ApiError::custom_code("error_reading_bytes"))?;

    ok(ModuleBytes { bytes })
}

pub fn init_routes() -> Router {
    Router::new()
        .route("/", post(register))
        .route("/stopped", post(node_stopped))
        .route("/started", post(node_started))
        .route("/nodes", get(list_nodes))
        .route("/module", post(add_module))
        .route("/module/:id", get(get_module))
        .layer(DefaultBodyLimit::disable())
        .layer(RequestBodyLimitLayer::new(50 * 1024 * 1024)) // 50 mb
}
