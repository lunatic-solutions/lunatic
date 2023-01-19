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
    quic::{self, SendStream},
    DistributedCtx, DistributedProcessState,
};

use super::message::{ClientError, Spawn};

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
        Ok(crate::control::cert::TEST_ROOT_CERT.to_string())
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
    ca_cert: String,
    cert: String,
    key: String,
) -> Result<()>
where
    T: ProcessState + ResourceLimiter + DistributedCtx<E> + Send + 'static,
    E: Environment + 'static,
{
    let mut quic_server = quic::new_quic_server(socket, &cert, &key, &ca_cert)?;
    if let Err(e) = quic::handle_node_server(&mut quic_server, ctx.clone()).await {
        log::error!("Node server stopped {e}")
    };
    Ok(())
}

pub async fn handle_message<T, E>(
    ctx: ServerCtx<T, E>,
    send: &mut SendStream,
    msg_id: u64,
    msg: Request,
) where
    T: ProcessState + DistributedCtx<E> + ResourceLimiter + Send + 'static,
    E: Environment + 'static,
{
    if let Err(e) = handle_message_err(ctx, send, msg_id, msg).await {
        log::error!("Error handling message: {e}");
    }
}

async fn handle_message_err<T, E>(
    ctx: ServerCtx<T, E>,
    send: &mut SendStream,
    msg_id: u64,
    msg: Request,
) -> Result<()>
where
    T: ProcessState + DistributedCtx<E> + ResourceLimiter + Send + 'static,
    E: Environment + 'static,
{
    match msg {
        Request::Spawn(spawn) => {
            match handle_spawn(ctx, spawn).await {
                Ok(Ok(id)) => {
                    let mut data = super::message::pack_response(msg_id, Response::Spawned(id));
                    send.send(&mut data).await?;
                }
                Ok(Err(client_error)) => {
                    let mut data =
                        super::message::pack_response(msg_id, Response::Error(client_error));
                    send.send(&mut data).await?;
                }
                Err(error) => {
                    let mut data = super::message::pack_response(
                        msg_id,
                        Response::Error(ClientError::Unexpected(error.to_string())),
                    );
                    send.send(&mut data).await?
                }
            };
        }
        Request::Message {
            environment_id,
            process_id,
            tag,
            data,
        } => match handle_process_message(ctx, environment_id, process_id, tag, data).await {
            Ok(_) => {
                let mut data = super::message::pack_response(msg_id, Response::Sent);
                send.send(&mut data).await?;
            }
            Err(error) => {
                let mut data = super::message::pack_response(msg_id, Response::Error(error));
                send.send(&mut data).await?;
            }
        },
    };
    Ok(())
}

async fn handle_spawn<T, E>(ctx: ServerCtx<T, E>, spawn: Spawn) -> Result<Result<u64, ClientError>>
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

    let config: T::Config = rmp_serde::from_slice(&config[..])?;
    let config = Arc::new(config);

    let module = match ctx.modules.get(module_id) {
        Some(module) => module,
        None => {
            if let Ok(bytes) = ctx
                .distributed
                .control
                .get_module(module_id, environment_id)
                .await
            {
                let wasm = RawWasm::new(Some(module_id), bytes);
                ctx.modules.compile(ctx.runtime.clone(), wasm).await??
            } else {
                return Ok(Err(ClientError::ModuleNotFound));
            }
        }
    };

    let env = ctx.envs.get(environment_id).await;

    let env = match env {
        Some(env) => env,
        None => ctx.envs.create(environment_id).await,
    };

    let spawn_permit = env.can_spawn_next_process().await?;

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
        spawn_permit,
    )
    .await?;
    Ok(Ok(proc.id()))
}

async fn handle_process_message<T, E>(
    ctx: ServerCtx<T, E>,
    environment_id: u64,
    process_id: u64,
    tag: Option<i64>,
    data: Vec<u8>,
) -> std::result::Result<(), ClientError>
where
    T: ProcessState + DistributedCtx<E> + ResourceLimiter + Send + 'static,
    E: Environment,
{
    let env = ctx.envs.get(environment_id).await;
    if let Some(env) = env {
        if let Some(proc) = env.get_process(process_id) {
            proc.send(Signal::Message(Message::Data(DataMessage::new_from_vec(
                tag, data,
            ))));
        } else {
            return Err(ClientError::ProcessNotFound);
        }
    }
    Ok(())
}
