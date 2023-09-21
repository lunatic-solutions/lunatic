use std::{future::Future, sync::Arc, time::Duration};

use anyhow::{anyhow, Result};
use asn1_rs::ToDer;
use lunatic_common_api::{get_memory, write_to_guest_vec, IntoTrap};
use lunatic_distributed::{
    distributed::message::{ClientError, Spawn, Val},
    CertAttrs, DistributedCtx, SUBJECT_DIR_ATTRS,
};
use lunatic_error_api::ErrorCtx;
use lunatic_process::{
    env::Environment,
    message::{DataMessage, Message},
};
use lunatic_process_api::ProcessCtx;
use rcgen::{Certificate, CertificateParams, CertificateSigningRequest, CustomExtension, KeyPair};
use tokio::time::timeout;
use wasmtime::{Caller, Linker, ResourceLimiter};

// Register the lunatic distributed APIs to the linker
pub fn register<T, E>(linker: &mut Linker<T>) -> Result<()>
where
    T: DistributedCtx<E> + ProcessCtx<T> + Send + ResourceLimiter + ErrorCtx + 'static,
    E: Environment + 'static,
    for<'a> &'a T: Send,
{
    linker.func_wrap("lunatic::distributed", "nodes_count", nodes_count)?;
    linker.func_wrap("lunatic::distributed", "get_nodes", get_nodes)?;
    linker.func_wrap("lunatic::distributed", "node_id", node_id)?;
    linker.func_wrap("lunatic::distributed", "module_id", module_id)?;
    linker.func_wrap8_async("lunatic::distributed", "spawn", spawn)?;
    linker.func_wrap2_async("lunatic::distributed", "send", send)?;
    linker.func_wrap4_async(
        "lunatic::distributed",
        "send_receive_skip_search",
        send_receive_skip_search,
    )?;
    linker.func_wrap5_async(
        "lunatic::distributed",
        "exec_lookup_nodes",
        exec_lookup_nodes,
    )?;
    linker.func_wrap(
        "lunatic::distributed",
        "copy_lookup_nodes_results",
        copy_lookup_nodes_results,
    )?;
    linker.func_wrap1_async("lunatic::distributed", "test_root_cert", test_root_cert)?;
    linker.func_wrap5_async(
        "lunatic::distributed",
        "default_server_certificates",
        default_server_certificates,
    )?;
    linker.func_wrap7_async("lunatic::distributed", "sign_node", sign_node)?;
    Ok(())
}

// Returns the number of registered nodes
fn nodes_count<T, E>(caller: Caller<T>) -> u32
where
    T: DistributedCtx<E>,
    E: Environment,
{
    caller
        .data()
        .distributed()
        .map(|d| d.control.node_count())
        .unwrap_or(0) as u32
}

// Copy node ids into guest memory. Returns the number of nodes copied.
//
// Traps:
// * If any memory outside the guest heap space is referenced.
fn get_nodes<T, E>(mut caller: Caller<T>, nodes_ptr: u32, nodes_len: u32) -> Result<u32>
where
    T: DistributedCtx<E>,
    E: Environment,
{
    let memory = get_memory(&mut caller)?;
    let node_ids = caller
        .data()
        .distributed()
        .map(|d| d.control.node_ids())
        .unwrap_or_else(|_| vec![]);
    let copy_nodes_len = node_ids.len().min(nodes_len as usize);
    memory
        .data_mut(&mut caller)
        .get_mut(
            nodes_ptr as usize..(nodes_ptr as usize + std::mem::size_of::<u64>() * copy_nodes_len),
        )
        .or_trap("lunatic::distributed::get_nodes::memory")?
        .copy_from_slice(unsafe { node_ids[..copy_nodes_len].align_to::<u8>().1 });
    Ok(copy_nodes_len as u32)
}

// Submits a lookup node query to the control server and waits for the results.
//
// Filtering is done based on tags which are `key=value` user defined node
// metadata, see CLI flag `tag`.
//
// Traps:
// * If the query is not a valid UTF-8 string
// * if any memory outside the guest heap space is referenced
fn exec_lookup_nodes<T, E>(
    mut caller: Caller<T>,
    query_ptr: u32,
    query_len: u32,
    query_id_ptr: u32,
    nodes_len_ptr: u32,
    error_ptr: u32,
) -> Box<dyn Future<Output = Result<u32>> + Send + '_>
where
    T: DistributedCtx<E> + ErrorCtx + Send + 'static,
    E: Environment + 'static,
    for<'a> &'a T: Send,
{
    Box::new(async move {
        let memory = get_memory(&mut caller)?;
        let query_str = memory
            .data(&caller)
            .get(query_ptr as usize..(query_ptr + query_len) as usize)
            .or_trap("lunatic::distributed::lookup_nodes::query_ptr")?;
        let query = std::str::from_utf8(query_str)
            .or_trap("lunatic::distributed::lookup_nodes::query_str_utf8")?;
        let distributed = caller.data().distributed()?;
        match distributed.control.lookup_nodes(query).await {
            Ok((query_id, nodes_len)) => {
                memory
                    .write(&mut caller, query_id_ptr as usize, &query_id.to_le_bytes())
                    .or_trap("lunatic::distributed::lookup_nodes::query_id")?;
                memory
                    .write(
                        &mut caller,
                        nodes_len_ptr as usize,
                        &nodes_len.to_le_bytes(),
                    )
                    .or_trap("lunatic::distributed::lookup_nodes::nodes_len")?;
                Ok(0)
            }
            Err(error) => {
                let error_id = caller.data_mut().error_resources_mut().add(error);
                memory
                    .write(&mut caller, error_ptr as usize, &error_id.to_le_bytes())
                    .or_trap("lunatic::distributed::lookup_nodes::error_ptr")?;
                Ok(1)
            }
        }
    })
}

// Copies node ids to guest memory from the lookup node query result, returns number of node ids copied.
//
// Traps:
// * If any memory outside the guest heap space is referenced.
fn copy_lookup_nodes_results<T, E>(
    mut caller: Caller<T>,
    query_id: u64,
    nodes_ptr: u32,
    nodes_len: u32,
    error_ptr: u32,
) -> Result<i32>
where
    T: DistributedCtx<E> + ErrorCtx,
    E: Environment,
{
    let memory = get_memory(&mut caller)?;
    if let Some(query_results) = caller
        .data()
        .distributed()
        .map(|d| d.control.query_result(&query_id))?
    {
        let nodes = query_results.1;
        let copy_nodes_len = nodes.len().min(nodes_len as usize);
        let memory = get_memory(&mut caller)?;
        memory
            .data_mut(&mut caller)
            .get_mut(
                nodes_ptr as usize
                    ..(nodes_ptr as usize + std::mem::size_of::<u64>() * copy_nodes_len),
            )
            .or_trap("lunatic::distributed::copy_lookup_nodes_results::memory")?
            .copy_from_slice(unsafe { nodes[..copy_nodes_len].align_to::<u8>().1 });
        Ok(copy_nodes_len as i32)
    } else {
        let error = anyhow!("Invalid query id");
        let error_id = caller.data_mut().error_resources_mut().add(error);
        memory
            .write(&mut caller, error_ptr as usize, &error_id.to_le_bytes())
            .or_trap("lunatic::distributed::copy_lookup_nodes_results::error_ptr")?;
        Ok(-1)
    }
}

fn test_root_cert<T, E>(
    mut caller: Caller<T>,
    len_ptr: u32,
) -> Box<dyn Future<Output = Result<u32>> + Send + '_>
where
    T: DistributedCtx<E> + Send,
    E: Environment,
{
    Box::new(async move {
        let memory = get_memory(&mut caller)?;
        let root_cert = lunatic_distributed::control::cert::test_root_cert()
            .or_trap("lunatic::distributed::test_root_cert")?;

        let cert_pem = root_cert
            .serialize_pem()
            .or_trap("lunatic::distributed::test_root_cert")?;
        let key_pair_pem = root_cert.serialize_private_key_pem();

        let data = bincode::serialize(&(cert_pem, key_pair_pem))
            .or_trap("lunatic::distributed::test_root_cert")?;
        let ptr = write_to_guest_vec(&mut caller, &memory, &data, len_ptr)
            .await
            .or_trap("lunatic::distributed::test_root_cert")?;

        Ok(ptr)
    })
}

fn default_server_certificates<T, E>(
    mut caller: Caller<T>,
    cert_pem_ptr: u32,
    cert_pem_len: u32,
    pk_pem_ptr: u32,
    pk_pem_len: u32,
    len_ptr: u32,
) -> Box<dyn Future<Output = Result<u32>> + Send + '_>
where
    T: DistributedCtx<E> + Send,
    E: Environment,
{
    Box::new(async move {
        let memory = get_memory(&mut caller)?;

        let cert_pem_bytes = memory
            .data(&caller)
            .get(cert_pem_ptr as usize..(cert_pem_ptr + cert_pem_len) as usize)
            .or_trap("lunatic::distributed::spawn::default_server_certificates")?;
        let cert_pem = std::str::from_utf8(cert_pem_bytes)
            .or_trap("lunatic::distributed::default_server_certificates")?;

        let pk_pem_bytes = memory
            .data(&caller)
            .get(pk_pem_ptr as usize..(pk_pem_ptr + pk_pem_len) as usize)
            .or_trap("lunatic::distributed::default_server_certificates")?;
        let pk_pem = std::str::from_utf8(pk_pem_bytes)
            .or_trap("lunatic::distributed::default_server_certificates")?;

        let key_pair = KeyPair::from_pem(pk_pem)
            .or_trap("lunatic::distributed::default_server_certificates")?;
        let cert_params = CertificateParams::from_ca_cert_pem(cert_pem, key_pair)
            .or_trap("lunatic::distributed::default_server_certificates")?;

        let root_cert = Certificate::from_params(cert_params)
            .or_trap("lunatic::distributed::default_server_certificates")?;

        let (ctrl_cert, ctrl_pk) =
            lunatic_distributed::control::cert::default_server_certificates(&root_cert)?;

        let data = bincode::serialize(&(ctrl_cert, ctrl_pk))
            .or_trap("lunatic::distributed::default_server_certificates")?;
        let ptr = write_to_guest_vec(&mut caller, &memory, &data, len_ptr)
            .await
            .or_trap("lunatic::distributed::default_server_certificates")?;

        Ok(ptr)
    })
}

#[allow(clippy::too_many_arguments)]
fn sign_node<T, E>(
    mut caller: Caller<T>,
    cert_pem_ptr: u32,
    cert_pem_len: u32,
    pk_pem_ptr: u32,
    pk_pem_len: u32,
    csr_pem_ptr: u32,
    csr_pem_len: u32,
    len_ptr: u32,
) -> Box<dyn Future<Output = Result<u32>> + Send + '_>
where
    T: DistributedCtx<E> + Send,
    E: Environment,
{
    Box::new(async move {
        let memory = get_memory(&mut caller)?;

        let cert_pem_bytes = memory
            .data(&caller)
            .get(cert_pem_ptr as usize..(cert_pem_ptr + cert_pem_len) as usize)
            .or_trap("lunatic::distributed::spawn::sign_node")?;
        let cert_pem =
            std::str::from_utf8(cert_pem_bytes).or_trap("lunatic::distributed::sign_node")?;

        let pk_pem_bytes = memory
            .data(&caller)
            .get(pk_pem_ptr as usize..(pk_pem_ptr + pk_pem_len) as usize)
            .or_trap("lunatic::distributed::sign_node")?;
        let pk_pem =
            std::str::from_utf8(pk_pem_bytes).or_trap("lunatic::distributed::sign_node")?;

        let csr_pem_bytes = memory
            .data(&caller)
            .get(csr_pem_ptr as usize..(csr_pem_ptr + csr_pem_len) as usize)
            .or_trap("lunatic::distributed::sign_node")?;
        let csr_pem =
            std::str::from_utf8(csr_pem_bytes).or_trap("lunatic::distributed::sign_node")?;

        let key_pair = KeyPair::from_pem(pk_pem).or_trap("lunatic::distributed::sign_node")?;
        let cert_params = CertificateParams::from_ca_cert_pem(cert_pem, key_pair)
            .or_trap("lunatic::distributed::sign_node")?;

        let ca_cert =
            Certificate::from_params(cert_params).or_trap("lunatic::distributed::sign_node")?;

        let mut csr = CertificateSigningRequest::from_pem(csr_pem)
            .or_trap("lunatic::distributed::sign_node")?;
        // Add json to custom certificate extension
        csr.params
            .custom_extensions
            .push(CustomExtension::from_oid_content(
                &SUBJECT_DIR_ATTRS,
                serde_json::to_string(&CertAttrs {
                    allowed_envs: vec![],
                    is_privileged: true,
                })
                .or_trap("lunatic::distributed::sign_node")?
                .to_der_vec()
                .or_trap("lunatic::distributed::sign_node")?,
            ));
        let cert_pem = csr.serialize_pem_with_signer(&ca_cert).or_trap("lunatic::distributed::sign_node")?;
        let data = bincode::serialize(&cert_pem).or_trap("lunatic::distributed::sign_node")?;
        let ptr = write_to_guest_vec(&mut caller, &memory, &data, len_ptr)
            .await
            .or_trap("lunatic::distributed::sign_node")?;

        Ok(ptr)
    })
}

// Similar to a local spawn, it spawns a new process using the passed in function inside a module
// as the entry point. The process is spawned on a node with id `node_id`.
//
// If `config_id` is 0, the same config is used as in the process calling this function.
//
// The function arguments are passed as an array with the following structure:
// [0 byte = type ID; 1..17 bytes = value as u128, ...]
// The type ID follows the WebAssembly binary convention:
//  - 0x7F => i32
//  - 0x7E => i64
//  - 0x7B => v128
// If any other value is used as type ID, this function will trap. If your type
// would ordinarily occupy fewer than 16 bytes (e.g. in an i32 or i64), you MUST
// first convert it to an i128.
//
// Returns:
// * 0      on success - The ID of the newly created process is written to `id_ptr`
// * 1      If node does not exist
// * 2      If module does not exist
// * 9027   If node connection error occurred
//
// Traps:
// * If the function string is not a valid utf8 string.
// * If the params array is in a wrong format.
// * If any memory outside the guest heap space is referenced.
#[allow(clippy::too_many_arguments)]
fn spawn<T, E>(
    mut caller: Caller<T>,
    node_id: u64,
    config_id: i64,
    module_id: u64,
    func_str_ptr: u32,
    func_str_len: u32,
    params_ptr: u32,
    params_len: u32,
    id_ptr: u32,
) -> Box<dyn Future<Output = Result<u32>> + Send + '_>
where
    T: DistributedCtx<E> + ResourceLimiter + Send + ErrorCtx + 'static,
    E: Environment,
    for<'a> &'a T: Send,
{
    Box::new(async move {
        if !caller.data().can_spawn() {
            return Err(anyhow!(
                "Process doesn't have permissions to spawn sub-processes"
            ));
        }
        let memory = get_memory(&mut caller)?;
        let func_str = memory
            .data(&caller)
            .get(func_str_ptr as usize..(func_str_ptr + func_str_len) as usize)
            .or_trap("lunatic::distributed::spawn::func_str")?;

        let function =
            std::str::from_utf8(func_str).or_trap("lunatic::distributed::spawn::func_str_utf8")?;

        let params = memory
            .data(&caller)
            .get(params_ptr as usize..(params_ptr + params_len) as usize)
            .or_trap("lunatic::distributed::spawn::params")?;
        let params = params
            .chunks_exact(17)
            .map(|chunk| {
                let value = u128::from_le_bytes(chunk[1..].try_into()?);
                let result = match chunk[0] {
                    0x7F => Val::I32(value as i32),
                    0x7E => Val::I64(value as i64),
                    0x7B => Val::V128(value),
                    _ => return Err(anyhow!("Unsupported type ID")),
                };
                Ok(result)
            })
            .collect::<Result<Vec<_>>>()?;

        let state = caller.data();

        let config = match config_id {
            -1 => state.config().clone(),
            config_id => Arc::new(
                caller
                    .data()
                    .config_resources()
                    .get(config_id as u64)
                    .or_trap("lunatic::distributed::spawn: Config ID doesn't exist")?
                    .clone(),
            ),
        };
        let config: Vec<u8> =
            rmp_serde::to_vec(config.as_ref()).map_err(|_| anyhow!("Error serializing config"))?;

        log::debug!("Spawn on node {node_id}, mod {module_id}, fn {function}, params {params:?}");

        let (process_or_error_id, ret) = match state
            .distributed()?
            .node_client
            .spawn(
                node_id,
                Spawn {
                    environment_id: state.environment_id(),
                    function: function.to_string(),
                    module_id,
                    params,
                    config,
                },
            )
            .await
        {
            Ok(process_id) => (process_id, 0),
            Err(error) => {
                let (code, message): (u32, String) = match error {
                    ClientError::Unexpected(cause) => Err(anyhow!(cause)),
                    ClientError::NodeNotFound => Ok((1, "Node does not exist.".to_string())),
                    ClientError::ModuleNotFound => Ok((2, "Module does not exist.".to_string())),
                    ClientError::Connection(cause) => Ok((9027, cause)),
                    _ => Err(anyhow!("unreachable")),
                }?;
                (
                    caller
                        .data_mut()
                        .error_resources_mut()
                        .add(anyhow!(message)),
                    code,
                )
            }
        };

        memory
            .write(
                &mut caller,
                id_ptr as usize,
                &process_or_error_id.to_le_bytes(),
            )
            .or_trap("lunatic::distributed::spawn::write_id")?;

        Ok(ret)
    })
}

// Sends the message in scratch area to a process running on a node with id `node_id`.
//
// There are no guarantees that the message will be received.
//
// Returns:
// * 0      If message sent
// * 1      If process_id does not exist
// * 2      If node_id does not exist
// * 9027   If node connection error occurred
//
// Traps:
// * If it's called before creating the next message.
// * If the message contains resources
fn send<T, E>(
    mut caller: Caller<T>,
    node_id: u64,
    process_id: u64,
) -> Box<dyn Future<Output = Result<u32>> + Send + '_>
where
    T: DistributedCtx<E> + ProcessCtx<T> + Send + ErrorCtx + 'static,
    E: Environment,
    for<'a> &'a T: Send,
{
    Box::new(async move {
        let message = caller
            .data_mut()
            .message_scratch_area()
            .take()
            .or_trap("lunatic::distributed::send::no_message")?;

        if let Message::Data(DataMessage {
            tag,
            buffer,
            resources,
            ..
        }) = message
        {
            if !resources.is_empty() {
                return Err(anyhow!("Cannot send resources to remote nodes."));
            }

            let state = caller.data();
            match state
                .distributed()?
                .node_client
                .message_process(node_id, state.environment_id(), process_id, tag, buffer)
                .await
            {
                Ok(_) => Ok(0),
                Err(error) => match error {
                    ClientError::Unexpected(cause) => Err(anyhow!(cause)),
                    ClientError::ProcessNotFound => Ok(1),
                    ClientError::NodeNotFound => Ok(2),
                    ClientError::Connection(_) => Ok(9027),
                    _ => Err(anyhow!("unreachable")),
                },
            }
        } else {
            Err(anyhow!("Only Message::Data can be sent across nodes."))
        }
    })
}

// Sends the message to a process on a node with id `node_id` and waits for a reply,
// but doesn't look through existing messages in the mailbox queue while waiting.
// This is an optimization that only makes sense with tagged messages.
// In a request/reply scenario we can tag the request message with an
// unique tag and just wait on it specifically.
//
// This operation needs to be an atomic host function, if we jumped back into the guest we could
// miss out on the incoming message before `receive` is called.
//
// If timeout is specified (value different from u64::MAX), the function will return on timeout
// expiration with value 9027.
//
// Returns:
// * 0    If message arrived.
// * 1    If process_id does not exist
// * 2    If node_id does not exist
// * 9027 If call timed out.
//
// Traps:
// * If it's called with wrong data in the scratch area.
// * If the message contains resources
fn send_receive_skip_search<T, E>(
    mut caller: Caller<T>,
    node_id: u64,
    process_id: u64,
    wait_on_tag: i64,
    timeout_duration: u64,
) -> Box<dyn Future<Output = Result<u32>> + Send + '_>
where
    T: DistributedCtx<E> + ProcessCtx<T> + Send + 'static,
    E: Environment,
    for<'a> &'a T: Send,
{
    Box::new(async move {
        let message = caller
            .data_mut()
            .message_scratch_area()
            .take()
            .or_trap("lunatic::distributed::send_receive_skip_search")?;

        if let Message::Data(DataMessage {
            tag,
            buffer,
            resources,
            ..
        }) = message
        {
            if !resources.is_empty() {
                return Err(anyhow!("Cannot send resources to remote nodes."));
            }

            let state = caller.data();
            let code = match state
                .distributed()?
                .node_client
                .message_process(node_id, state.environment_id(), process_id, tag, buffer)
                .await
            {
                Ok(_) => Ok(0),
                Err(error) => match error {
                    ClientError::ProcessNotFound => Ok(1),
                    ClientError::NodeNotFound => Ok(2),
                    ClientError::Unexpected(cause) => Err(anyhow!(cause)),
                    _ => Err(anyhow!("unreachable")),
                },
            }?;

            if code != 0 {
                return Ok(code);
            }

            let tags = [wait_on_tag];
            let pop_skip_search = caller.data_mut().mailbox().pop_skip_search(Some(&tags));
            if let Ok(message) = match timeout_duration {
                // Without timeout
                u64::MAX => Ok(pop_skip_search.await),
                // With timeout
                t => timeout(Duration::from_millis(t), pop_skip_search).await,
            } {
                // Put the message into the scratch area
                caller.data_mut().message_scratch_area().replace(message);
                Ok(0)
            } else {
                Ok(9027)
            }
        } else {
            Err(anyhow!("Only Message::Data can be sent across nodes."))
        }
    })
}

// Returns the id of the node that the current process is running on
fn node_id<T, E>(caller: Caller<T>) -> u64
where
    T: DistributedCtx<E>,
    E: Environment,
{
    caller
        .data()
        .distributed()
        .as_ref()
        .map(|d| d.node_id())
        .unwrap_or(0)
}

// Returns id of the module that the current process is spawned from
fn module_id<T, E>(caller: Caller<T>) -> u64
where
    T: DistributedCtx<E>,
    E: Environment,
{
    caller.data().module_id()
}
