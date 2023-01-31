mod api;
mod routes;
mod server;

use std::net::ToSocketAddrs;

use lunatic::process::StartProcess;
use submillisecond::{
    response::Response, router, state::State, Application, Handler, RequestContext,
};

use crate::routes::{add_module, get_module, list_nodes, node_started, node_stopped, register};
use crate::server::ControlServer;

struct RequestBodyLimit {
    limit: usize,
}

impl RequestBodyLimit {
    pub fn new(limit: usize) -> Self {
        RequestBodyLimit { limit }
    }
}

impl Handler for RequestBodyLimit {
    fn handle(&self, req: RequestContext) -> Response {
        req.next_handler()
    }
}

fn main() -> std::io::Result<()> {
    ControlServer::start_link((), Some("ControlServer"));

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
    .serve(addrs.as_slice())
}
