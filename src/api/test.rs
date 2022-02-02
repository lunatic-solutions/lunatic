// use std::convert::TryInto;
// use std::future::Future;
// use std::io::IoSlice;
// use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
// use std::sync::Arc;
// use std::time::Duration;

use anyhow::Result;
// use async_std::io::{ReadExt, WriteExt};
// use async_std::net::{TcpListener, TcpStream, UdpSocket};
use wasmtime::{Caller, Linker, FuncType, ValType};
use wasmtime::{Trap};

use crate::api::error::IntoTrap;
// use crate::state::DnsIterator;
use crate::{api::get_memory, state::ProcessState};
use crate::test_node::{ TestNode, TESTS };
use super::link_if_match;

pub(crate) fn register(
    linker: &mut Linker<ProcessState>,
    namespace_filter: &[String],
) -> Result<()> {
    link_if_match(
        linker,
        "lunatic::test",
        "add_test",
        FuncType::new([ValType::I64, ValType::I32, ValType::I32], [ValType::I64]),
        add_test,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::test",
        "add_test_comment",
        FuncType::new([ValType::I64, ValType::I32, ValType::I32], [ValType::I64]),
        add_test_comment,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::test",
        "test_ok",
        FuncType::new([ValType::I64], []),
        test_ok,
        namespace_filter,
    )?;
    Ok(())
}


//% lunatic::test::add_test(parent_id: u64, name: u32, name_len: u32)
//%
//% Creates a global test node resource.
//%
//% Traps:
//% * If the buffer + name_len is outside of memory
//% * If the lock cannot be obtained
//% * If the parent TestNode doesn't exist
fn add_test(mut caller: Caller<ProcessState>,
    parent_id: u64,
    name: u32,
    name_len: u32,
) -> Result<u64, Trap> {
    let memory = get_memory(&mut caller)?;

    // get the buffer slice
    let buffer = memory
        .data_mut(&mut caller)
        .get(name as usize..(name + name_len) as usize)
        .or_trap("lunatic::test::add_test")?;

    let node = TestNode::new(buffer);
    let mut lock = TESTS.lock()
        .or_trap("lunatic::test::add_test")?;

    let child = lock.add(node);
    let parent = lock.get_mut(parent_id)
        .or_trap("lunatic::test::add_test")?;
    parent.push_child(child);
    Ok(child)
}

//% lunatic::test::add_test_comment(node_id: u64, name: u32, name_len: u32)
//%
//% Creates a global test node resource.
//%
//% Traps:
//% * If the buffer + name_len is outside of memory
//% * If the lock cannot be obtained
//% * If the parent TestNode doesn't exist
fn add_test_comment(mut caller: Caller<ProcessState>,
    node_id: u64,
    comment: u32,
    comment_len: u32,
) -> Result<(), Trap> {
    let memory = get_memory(&mut caller)?;

    // get the buffer slice
    let buffer = memory
        .data_mut(&mut caller)
        .get(comment as usize..(comment + comment_len) as usize)
        .or_trap("lunatic::test::add_test_comment")?;

    let mut lock = TESTS.lock()
        .or_trap("lunatic::test::add_test_comment")?;

    let parent = lock.get_mut(node_id)
        .or_trap("lunatic::test::add_test_comment")?;
    // parent.comments.push
    parent.add_comment(buffer);
    Ok(())
}


//% lunatic::test::test_ok(test_id: u64)
//%
//% Denotes a test as OK
//%
//% Traps:
//% * If the lock cannot be obtained
//% * If the parent TestNode doesn't exist
fn test_ok(mut _caller: Caller<ProcessState>,
    test_id: u64,
) -> Result<(), Trap> {

    // get the test, call ok()

    TESTS.lock()
        .or_trap("lunatic::test::test_ok")?
        .get_mut(test_id)
        .or_trap("lunatic::test::test_ok")?
        .ok();

    Ok(())
}

