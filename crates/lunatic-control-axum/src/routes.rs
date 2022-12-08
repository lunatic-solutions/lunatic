use std::sync::Arc;

use axum::{
    routing::{get, post},
    Extension, Json, Router,
};
use lunatic_distributed::{
    control::{api::*, cert::TEST_ROOT_CERT},
    NodeInfo,
};
use rcgen::CertificateSigningRequest;

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
        cert_pem,
        authentication_token,
        root_cert: TEST_ROOT_CERT.into(),
        urls: ControlUrls {
            api_base: format!("http://{host}/"),
            nodes: format!("http://{host}/api/control/nodes"),
            node_started: format!("http://{host}/api/control/started"),
            node_stopped: format!("http://{host}/api/control/stopped"),
            get_module: format!("http://{host}/api/control/module/{{id}}"),
            add_module: format!("http://{host}/api/control/module"),
            get_nodes: format!("http://{host}/api/control/nodes"),
        },
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
    node_auth: NodeAuth,
    control: Extension<Arc<ControlServer>>,
) -> ApiResponse<NodesList> {
    log::info!("Node {} list nodes", node_auth.node_name);

    let control = control.as_ref();
    let nds: Vec<_> = control
        .nodes
        .iter()
        .filter(|n| n.status < 2 && !n.node_address.is_empty())
        .collect();
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
    Json(data): Json<AddModule>,
) -> ApiResponse<()> {
    log::info!("Node {} add_module", node_auth.node_name);

    let control = control.as_ref();
    control.add_module(data.bytes);

    ok(())
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

async fn okay() -> ApiResponse<String> {
    ok("ok".to_string())
}

pub fn init_routes() -> Router {
    Router::new()
        .route("/ok", get(okay))
        .route("/register", post(register))
        .route("/stopped", post(node_stopped))
        .route("/started", post(node_started))
        .route("/nodes", get(list_nodes))
        .route("/module", post(add_module))
        .route("/module/:id", get(get_module))
}
