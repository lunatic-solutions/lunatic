mod api;
mod host;
mod routes;
mod server;

use std::net::ToSocketAddrs;

use api::RequestBodyLimit;
use lunatic::AbstractProcess;
use server::ControlServerRequests;
use submillisecond::{router, Application};

use crate::routes::{add_module, get_module, list_nodes, node_started, node_stopped, register};
use crate::server::ControlServer;

fn main() -> anyhow::Result<()> {
    let root_cert = host::test_root_cert();
    let ca_cert = host::default_server_certificates(&root_cert.cert, &root_cert.pk);

    ControlServer::link()
        .start_as("ControlServer", ca_cert)
        .unwrap();

    let addrs: Vec<_> = (3030..3999_u16)
        .flat_map(|port| ("127.0.0.1", port).to_socket_addrs().unwrap())
        .collect();

    Application::new(router! {
        with RequestBodyLimit::new(50 * 1024 * 1024); // 50 mb

        POST "/" => register
        POST "/stopped" => node_stopped
        POST "/started" => node_started
        GET "/nodes" => list_nodes
        POST "/module" => add_module
        GET "/module/:id" => get_module
    })
    .serve(addrs.as_slice())?;

    Ok(())
}
