use lunatic_control::{
    api::{
        ControlUrls, ModuleBytes, ModuleId, NodeStart, NodeStarted, NodesList, Register,
        Registration,
    },
    NodeInfo,
};
use lunatic_log::info;

use crate::{
    api::{
        ok, ApiError, ApiResponse, ControlServerExtractor, HostExtractor, JsonExtractor, NodeAuth,
        PathExtractor,
    },
    server::{ControlServer, ControlServerHandler},
};

pub fn register(
    ControlServerExtractor(control): ControlServerExtractor,
    HostExtractor(host): HostExtractor,
    JsonExtractor(reg): JsonExtractor<Register>,
) -> ApiResponse<Registration> {
    info!("Registration for node name {}", reg.node_name);

    // let cert_pem = CertificateSigningRequest::from_pem(&reg.csr_pem)
    //     .and_then(|sign_request| sign_request.serialize_pem_with_signer(&control.ca_cert))
    //     .map_err(|e| ApiError::custom("sign_error", e.to_string()))?;
    let cert_pem: String = todo!();

    let mut authentication_token = [0u8; 32];
    getrandom::getrandom(&mut authentication_token).map_err(|err| {
        ApiError::log_internal_err("Error generating random token for registration", err)
    })?;
    let authentication_token = base64_url::encode(&authentication_token);

    control.register(reg.clone(), cert_pem.clone(), authentication_token.clone());

    ok(Registration {
        node_name: reg.node_name,
        cert_pem,
        authentication_token,
        root_cert: todo!(), // TEST_ROOT_CERT.into(),
        urls: ControlUrls {
            api_base: format!("http://{host}/"),
            nodes: format!("http://{host}/nodes"),
            node_started: format!("http://{host}/started"),
            node_stopped: format!("http://{host}/stopped"),
            get_module: format!("http://{host}/module/{{id}}"),
            add_module: format!("http://{host}/module"),
            get_nodes: format!("http://{host}/nodes"),
        },
    })
}

pub fn node_stopped(
    node_auth: NodeAuth,
    ControlServerExtractor(control): ControlServerExtractor,
) -> ApiResponse<()> {
    info!("Node {} stopped", node_auth.node_name);

    control.stop_node(node_auth.registration_id as u64);

    ok(())
}

pub fn node_started(
    node_auth: NodeAuth,
    ControlServerExtractor(control): ControlServerExtractor,
    JsonExtractor(data): JsonExtractor<NodeStart>,
) -> ApiResponse<NodeStarted> {
    control.stop_node(node_auth.registration_id as u64);

    let (node_id, _node_address) = control.start_node(node_auth.registration_id as u64, data);

    info!("Node {} started with id {}", node_auth.node_name, node_id);

    // TODO spawn all modules on node

    ok(NodeStarted {
        node_id: node_id as i64,
    })
}

pub fn list_nodes(
    _node_auth: NodeAuth,
    ControlServerExtractor(control): ControlServerExtractor,
) -> ApiResponse<NodesList> {
    let all_nodes = control.get_nodes();
    let nds: Vec<_> = all_nodes
        .iter()
        .filter(|n| n.status < 2 && !n.node_address.is_empty())
        .collect();
    let nodes: Vec<_> = control
        .get_registrations()
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

pub fn add_module(
    body: Vec<u8>,
    node_auth: NodeAuth,
    ControlServerExtractor(control): ControlServerExtractor,
) -> ApiResponse<ModuleId> {
    info!("Node {} add_module", node_auth.node_name);

    let module_id = control.add_module(body);
    ok(ModuleId { module_id })
}

pub fn get_module(
    node_auth: NodeAuth,
    PathExtractor(id): PathExtractor<u64>,
    ControlServerExtractor(control): ControlServerExtractor,
) -> ApiResponse<ModuleBytes> {
    info!("Node {} get_module {}", node_auth.node_name, id);

    let all_modules = control.get_modules();
    let bytes = all_modules
        .iter()
        .find(|m| m.key() == &id)
        .map(|m| m.value().clone())
        .ok_or_else(|| ApiError::custom_code("error_reading_bytes"))?;

    ok(ModuleBytes { bytes })
}
