use std::net::SocketAddr;

use anyhow::{anyhow, Result};

use lunatic_process::{
    env::Environments,
    message::{DataMessage, Message},
    runtimes::{wasmtime::WasmtimeRuntime, Modules, RawWasm},
    state::ProcessState,
    Signal,
};
use rcgen::*;
use s2n_quic::Connection as QuicConnection;
use wasmtime::ResourceLimiter;

use crate::{
    connection::{new_quic_server, Connection},
    distributed::message::{Request, Response},
    DistributedCtx, DistributedProcessState,
};

use super::message::Val;

pub struct ServerCtx<T> {
    pub envs: Environments,
    pub modules: Modules<T>,
    pub distributed: DistributedProcessState,
    pub runtime: WasmtimeRuntime,
}

impl<T: 'static> Clone for ServerCtx<T> {
    fn clone(&self) -> Self {
        Self {
            envs: self.envs.clone(),
            modules: self.modules.clone(),
            distributed: self.distributed.clone(),
            runtime: self.runtime.clone(),
        }
    }
}

pub fn root_cert(test_ca: bool, ca_cert: Option<&str>) -> Result<String> {
    if test_ca {
        Ok(crate::control::server::TEST_ROOT_CERT.to_string())
    } else {
        let cert = std::fs::read(
            ca_cert.ok_or_else(|| anyhow::anyhow!("Missing public root certificate."))?,
        )?;
        Ok(std::str::from_utf8(&cert)?.to_string())
    }
}

pub fn gen_node_cert(node_name: &str) -> Result<Certificate> {
    let mut params = CertificateParams::new(vec![node_name.to_string()]);
    params
        .distinguished_name
        .push(DnType::OrganizationName, "Lunatic Inc.");
    params.distinguished_name.push(DnType::CommonName, "Node");
    Certificate::from_params(params)
        .map_err(|_| anyhow!("Error while generating node certificate."))
}

pub async fn node_server<T>(
    ctx: ServerCtx<T>,
    socket: SocketAddr,
    cert: String,
    key: String,
) -> Result<()>
where
    T: ProcessState + ResourceLimiter + DistributedCtx + Send + 'static,
{
    let mut quic_server = new_quic_server(socket, &cert, &key)?;
    while let Some(connection) = quic_server.accept().await {
        let addr = connection.remote_addr()?;
        log::info!("New connection {addr}");
        tokio::task::spawn(handle_quic_connection(ctx.clone(), connection));
    }
    Ok(())
}

async fn handle_quic_connection<T>(ctx: ServerCtx<T>, mut conn: QuicConnection)
where
    T: ProcessState + DistributedCtx + ResourceLimiter + Send + 'static,
{
    while let Ok(Some(stream)) = conn.accept_bidirectional_stream().await {
        tokio::spawn(handle_quic_stream(ctx.clone(), Connection::new(stream)));
    }
}

async fn handle_quic_stream<T>(ctx: ServerCtx<T>, conn: Connection)
where
    T: ProcessState + ResourceLimiter + DistributedCtx + Send + 'static,
{
    while let Ok((msg_id, request)) = conn.receive::<Request>().await {
        tokio::spawn(handle_message(ctx.clone(), conn.clone(), msg_id, request));
    }
}

async fn handle_message<T>(
    ctx: ServerCtx<T>,
    conn: Connection,
    msg_id: u64,
    msg: Request,
) -> Result<()>
where
    T: ProcessState + DistributedCtx + ResourceLimiter + Send + 'static,
{
    match msg {
        Request::Spawn {
            environment_id,
            module_id,
            function,
            params,
        } => {
            let id = handle_spawn(ctx, environment_id, module_id, function, params).await?;
            conn.send(msg_id, Response::Spawned(id)).await?;
        }
        Request::Message {
            environment_id,
            process_id,
            tag,
            data,
        } => handle_process_message(ctx, environment_id, process_id, tag, data).await?,
    }
    Ok(())
}

async fn handle_spawn<T>(
    mut ctx: ServerCtx<T>,
    environment_id: u64,
    module_id: u64,
    function: String,
    params: Vec<Val>,
) -> Result<u64>
where
    T: ProcessState + DistributedCtx + ResourceLimiter + Send + 'static,
{
    let module = match ctx.modules.get(module_id) {
        Some(module) => module,
        None => {
            if let Some(bytes) = ctx.distributed.control.get_module(module_id).await {
                let wasm = RawWasm::new(Some(module_id), bytes);
                ctx.modules.compile(ctx.runtime.clone(), wasm).await??
            } else {
                return Err(anyhow!("Cannot get the module from control"));
            }
        }
    };

    let env = ctx.envs.get_or_create(environment_id);
    let distributed = ctx.distributed.clone();
    let runtime = ctx.runtime.clone();
    let state = T::new_dist_state(
        env.clone(),
        distributed,
        runtime,
        module.clone(),
        Default::default(),
    )?;
    let params: Vec<wasmtime::Val> = params.into_iter().map(Into::into).collect();
    let (_handle, proc) = env
        .spawn_wasm(ctx.runtime, module, state, &function, params, None)
        .await?;
    Ok(proc.id())
}

async fn handle_process_message<T>(
    mut ctx: ServerCtx<T>,
    environment_id: u64,
    process_id: u64,
    tag: Option<i64>,
    data: Vec<u8>,
) -> Result<()>
where
    T: ProcessState + DistributedCtx + ResourceLimiter + Send + 'static,
{
    let env = ctx.envs.get_or_create(environment_id);
    if let Some(proc) = env.get_process(process_id) {
        proc.send(Signal::Message(Message::Data(DataMessage::new_from_vec(
            tag, data,
        ))))
    }
    Ok(())
}
