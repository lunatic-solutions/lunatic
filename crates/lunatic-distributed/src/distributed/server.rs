use std::net::SocketAddr;

use anyhow::{anyhow, Result};

use lunatic_process::{
    env::Environments,
    runtimes::{wasmtime::WasmtimeRuntime, Modules, RawWasm},
    state::ProcessState,
};
use tokio::net::TcpListener;
use wasmtime::ResourceLimiter;

use crate::{
    connection::Connection,
    distributed::message::{Request, Response},
    DistributedCtx, DistributedProcessState,
};

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

pub async fn node_server<T>(ctx: ServerCtx<T>, socket: SocketAddr) -> Result<()>
where
    T: ProcessState + ResourceLimiter + DistributedCtx + Send + 'static,
{
    let listener = TcpListener::bind(socket).await?;
    while let Ok((conn, _addr)) = listener.accept().await {
        log::info!("New connection {_addr}");
        tokio::task::spawn(handle_connection(ctx.clone(), Connection::new(conn)));
    }
    Ok(())
}

async fn handle_connection<T>(ctx: ServerCtx<T>, conn: Connection)
where
    T: ProcessState + DistributedCtx + ResourceLimiter + Send + 'static,
{
    while let Ok((msg_id, msg)) = conn.receive::<Request>().await {
        tokio::task::spawn(handle_message(ctx.clone(), conn.clone(), msg_id, msg));
    }
}

async fn handle_message<T>(
    mut ctx: ServerCtx<T>,
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

            conn.send(msg_id, Response::Spawned(proc.id())).await?;
        }
    }
    Ok(())
}
