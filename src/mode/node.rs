use std::{
    net::{SocketAddr, UdpSocket},
    path::PathBuf,
};

use clap::Parser;

use std::{collections::HashMap, sync::Arc};

use anyhow::{anyhow, Context, Result};
use lunatic_distributed::{
    control::{self},
    distributed::{self, server::ServerCtx},
    quic,
};
use lunatic_process::{
    env::{Environments, LunaticEnvironments},
    runtimes::{self, Modules},
};
use lunatic_runtime::DefaultProcessState;
use uuid::Uuid;

use crate::mode::common::{run_wasm, RunWasm};

#[derive(Parser, Debug)]
pub(crate) struct Args {
    /// Control server register URL
    #[arg(
        index = 1,
        value_name = "CONTROL_URL",
        default_value = "http://127.0.0.1:3030/"
    )]
    control: String,

    #[arg(long, value_name = "NODE_SOCKET")]
    bind_socket: Option<SocketAddr>,

    #[arg(long, value_name = "WASM_MODULE")]
    wasm: Option<PathBuf>,

    /// Define key=value variable to store as node information
    #[arg(long, value_parser = parse_key_val, action = clap::ArgAction::Append)]
    tag: Vec<(String, String)>,

    #[cfg(feature = "prometheus")]
    #[command(flatten)]
    prometheus: super::common::PrometheusArgs,
}

pub(crate) async fn start(args: Args) -> Result<()> {
    #[cfg(feature = "prometheus")]
    if args.prometheus.prometheus {
        super::common::prometheus(args.prometheus.prometheus_http, None)?;
    }

    let socket = args
        .bind_socket
        .or_else(get_available_localhost)
        .ok_or_else(|| anyhow!("No available localhost UDP port"))?;
    let http_client = reqwest::Client::new();

    // TODO unwrap, better message
    let node_name = Uuid::new_v4();
    let node_name_str = node_name.as_hyphenated().to_string();
    let node_attributes: HashMap<String, String> = Default::default(); //args.tag.into_iter().collect(); TODO
    let node_cert = lunatic_distributed::distributed::server::gen_node_cert(&node_name_str)
        .with_context(|| "Failed to generate node CSR and PK")?;
    log::info!("Generate CSR for node name {node_name_str}");

    let reg = control::Client::register(
        &http_client,
        args.control
            .parse()
            .with_context(|| "Parsing control URL")?,
        node_name,
        node_cert.serialize_request_pem()?,
    )
    .await?;

    let control_client =
        control::Client::new(http_client.clone(), reg.clone(), socket, node_attributes).await?;

    let node_id = control_client.node_id();

    log::info!("Registration successful, node id {}", node_id);

    let quic_client = quic::new_quic_client(
        &reg.root_cert,
        &reg.cert_pem,
        &node_cert.serialize_private_key_pem(),
    )
    .with_context(|| "Failed to create mTLS QUIC client")?;

    let distributed_client =
        distributed::Client::new(node_id, control_client.clone(), quic_client.clone()).await?;

    let dist = lunatic_distributed::DistributedProcessState::new(
        node_id,
        control_client.clone(),
        distributed_client,
    )
    .await?;

    let wasmtime_config = runtimes::wasmtime::default_config();
    let runtime = runtimes::wasmtime::WasmtimeRuntime::new(&wasmtime_config)?;
    let envs = Arc::new(LunaticEnvironments::default());

    let node = tokio::task::spawn(lunatic_distributed::distributed::server::node_server(
        ServerCtx {
            envs: envs.clone(),
            modules: Modules::<DefaultProcessState>::default(),
            distributed: dist.clone(),
            runtime: runtime.clone(),
        },
        socket,
        reg.root_cert,
        reg.cert_pem,
        node_cert.serialize_private_key_pem(),
    ));

    if args.wasm.is_some() {
        let env = envs.create(1).await;
        tokio::task::spawn(async {
            if let Err(e) = run_wasm(RunWasm {
                path: args.wasm.unwrap(),
                wasm_args: vec![],
                dir: vec![],
                runtime,
                envs,
                env,
                distributed: Some(dist),
                tracer: todo!(),
            })
            .await
            {
                log::error!("Error running wasm: {e:?}");
            }
        });
    }

    let ctrl = control_client.clone();
    tokio::task::spawn(async move {
        async_ctrlc::CtrlC::new().unwrap().await;
        log::info!("Shutting down node");
        ctrl.notify_node_stopped().await.ok();
        std::process::exit(0);
    });

    node.await.ok();

    control_client.notify_node_stopped().await.ok();

    Ok(())
}

fn get_available_localhost() -> Option<SocketAddr> {
    for port in 1025..65535u16 {
        let addr = SocketAddr::new("127.0.0.1".parse().unwrap(), port);
        if UdpSocket::bind(addr).is_ok() {
            return Some(addr);
        }
    }

    None
}

fn parse_key_val(s: &str) -> Result<(String, String)> {
    if let Some((key, value)) = s.split_once('=') {
        Ok((key.to_string(), value.to_string()))
    } else {
        Err(anyhow!(format!("Tag '{s}' is not formatted as key=value")))
    }
}
