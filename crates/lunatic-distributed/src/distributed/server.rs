use std::{net::SocketAddr, sync::Arc};

use anyhow::{anyhow, Result};

use lunatic_process::{
    env::{Environment, Environments},
    message::{DataMessage, Message},
    runtimes::{wasmtime::WasmtimeRuntime, Modules, RawWasm},
    state::ProcessState,
    Signal,
};
use rcgen::*;
use wasmtime::ResourceLimiter;

use crate::{
    distributed::message::{Request, Response},
    quic, DistributedCtx, DistributedProcessState,
};

use super::message::Spawn;

pub struct ServerCtx<T, E: Environment> {
    pub envs: Arc<dyn Environments<Env = E>>,
    pub modules: Modules<T>,
    pub distributed: DistributedProcessState,
    pub runtime: WasmtimeRuntime,
}

impl<T: 'static, E: Environment> Clone for ServerCtx<T, E> {
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

pub async fn node_server<T, E>(
    ctx: ServerCtx<T, E>,
    socket: SocketAddr,
    cert: String,
    key: String,
) -> Result<()>
where
    T: ProcessState + ResourceLimiter + DistributedCtx<E> + Send + 'static,
    E: Environment + 'static,
{
    let mut quic_server = quic::new_quic_server(socket, &cert, &key)?;
    quic::handle_node_server(&mut quic_server, ctx.clone()).await?;
    Ok(())
}

pub async fn handle_message<T, E>(
    ctx: ServerCtx<T, E>,
    conn: quic::Connection,
    msg_id: u64,
    msg: Request,
) where
    T: ProcessState + DistributedCtx<E> + ResourceLimiter + Send + 'static,
    E: Environment + 'static,
{
    if let Err(e) = handle_message_err(ctx, conn, msg_id, msg).await {
        log::error!("Error handling message: {e}");
    }
}

async fn handle_message_err<T, E>(
    ctx: ServerCtx<T, E>,
    conn: quic::Connection,
    msg_id: u64,
    msg: Request,
) -> Result<()>
where
    T: ProcessState + DistributedCtx<E> + ResourceLimiter + Send + 'static,
    E: Environment + 'static,
{
    match msg {
        Request::Spawn(spawn) => {
            let id = handle_spawn(ctx, spawn).await?;
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

async fn handle_spawn<T, E>(ctx: ServerCtx<T, E>, spawn: Spawn) -> Result<u64>
where
    T: ProcessState + DistributedCtx<E> + ResourceLimiter + Send + 'static,
    E: Environment + 'static,
{
    let Spawn {
        environment_id,
        module_id,
        function,
        params,
        config,
    } = spawn;

    let config: T::Config = bincode::deserialize(&config[..])?;
    let config = Arc::new(config);

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

    let env = ctx
        .envs
        .get(environment_id)
        .unwrap_or_else(|| ctx.envs.create(environment_id));
    let distributed = ctx.distributed.clone();
    let runtime = ctx.runtime.clone();
    let state = T::new_dist_state(env.clone(), distributed, runtime, module.clone(), config)?;
    let params: Vec<wasmtime::Val> = params.into_iter().map(Into::into).collect();
    let (_handle, proc) = lunatic_process::wasm::spawn_wasm(
        env,
        ctx.runtime,
        &module,
        state,
        &function,
        params,
        None,
    )
    .await?;
    Ok(proc.id())
}

async fn handle_process_message<T, E>(
    ctx: ServerCtx<T, E>,
    environment_id: u64,
    process_id: u64,
    tag: Option<i64>,
    data: Vec<u8>,
) -> Result<()>
where
    T: ProcessState + DistributedCtx<E> + ResourceLimiter + Send + 'static,
    E: Environment,
{
    let env = ctx.envs.get(environment_id);
    if let Some(env) = env {
        if let Some(proc) = env.get_process(process_id) {
            proc.send(Signal::Message(Message::Data(DataMessage::new_from_vec(
                tag, data,
            ))))
        }
    }
    Ok(())
}
