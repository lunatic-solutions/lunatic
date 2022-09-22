use std::{net::SocketAddr, sync::Arc};

use anyhow::{anyhow, Result};

use lunatic_process::{
    env::Environments,
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
    let mut quic_server = quic::new_quic_server(socket, &cert, &key)?;
    quic::handle_node_server(&mut quic_server, ctx.clone()).await?;
    Ok(())
}

pub async fn handle_message<T>(ctx: ServerCtx<T>, send: &mut SendStream, msg_id: u64, msg: Request)
where
    T: ProcessState + DistributedCtx + ResourceLimiter + Send + 'static,
{
    if let Err(e) = handle_message_err(ctx, send, msg_id, msg).await {
        log::error!("Error handling message: {e}");
    }
}

async fn handle_message_err<T>(
    ctx: ServerCtx<T>,
    send: &mut SendStream,
    msg_id: u64,
    msg: Request,
) -> Result<()>
where
    T: ProcessState + DistributedCtx + ResourceLimiter + Send + 'static,
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

async fn handle_spawn<T>(mut ctx: ServerCtx<T>, spawn: Spawn) -> Result<Result<u64, ClientError>>
where
    T: ProcessState + DistributedCtx + ResourceLimiter + Send + 'static,
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
                return Ok(Err(ClientError::ModuleNotFound));
            }
        }
    };

    let env = ctx.envs.get_or_create(environment_id);
    let distributed = ctx.distributed.clone();
    let runtime = ctx.runtime.clone();
    let state = T::new_dist_state(env.clone(), distributed, runtime, module.clone(), config)?;
    let params: Vec<wasmtime::Val> = params.into_iter().map(Into::into).collect();
    let (_handle, proc) = env
        .spawn_wasm(ctx.runtime, module, state, &function, params, None)
        .await?;
    Ok(Ok(proc.id()))
}

async fn handle_process_message<T>(
    mut ctx: ServerCtx<T>,
    environment_id: u64,
    process_id: u64,
    tag: Option<i64>,
    data: Vec<u8>,
) -> std::result::Result<(), ClientError>
where
    T: ProcessState + DistributedCtx + ResourceLimiter + Send + 'static,
{
    let env = ctx.envs.get_or_create(environment_id);
    match env.get_process(process_id) {
        Some(proc) => {
            proc.send(Signal::Message(Message::Data(DataMessage::new_from_vec(
                tag, data,
            ))));
            Ok(())
        }
        None => Err(ClientError::ProcessNotFound),
    }
}
