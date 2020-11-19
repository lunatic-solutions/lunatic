use crate::process::ProcessEnvironment;
use crate::wasi::types::*;

use anyhow::Result;
use smol::net;
use wasmtime::{ExternRef, Func, FuncType, Linker, Trap, Val, ValType::*};

use std::cell::RefCell;

pub fn add_to_linker(linker: &mut Linker, environment: &ProcessEnvironment) -> Result<()> {
    // tcp_bind_str
    let env = environment.clone();
    let tcp_bind_str = Func::new(
        linker.store(),
        FuncType::new(vec![I32, I32], vec![I32, ExternRef]),
        move |_caller, params, result| -> Result<(), Trap> {
            let str_ptr = params[0].unwrap_i32();
            let str_len = params[1].unwrap_i32();
            let addr = WasiString::from(env.memory(), str_ptr as usize, str_len as usize);
            let listener = env.async_(net::TcpListener::bind(addr.get())).unwrap();

            result[0] = Val::I32(0); // success
            result[1] = Val::ExternRef(Some(ExternRef::new(listener)));

            Ok(())
        },
    );
    linker.define("lunatic", "tcp_bind_str", tcp_bind_str)?;

    // tcp_accept
    let env = environment.clone();
    let tcp_accept = Func::new(
        linker.store(),
        FuncType::new(vec![ExternRef], vec![I32, ExternRef, ExternRef]),
        move |_caller, params, result| -> Result<(), Trap> {
            let listener = params[0].unwrap_externref().unwrap();
            let listener = listener.data();
            if let Some(listener) = listener.downcast_ref::<net::TcpListener>() {
                let (stream, addr) = env.async_(listener.accept()).unwrap();
                result[0] = Val::I32(0); // success
                result[1] = Val::ExternRef(Some(ExternRef::new(RefCell::new(stream))));
                result[2] = Val::ExternRef(Some(ExternRef::new(addr)));
                Ok(())
            } else {
                Err(Trap::new("lunatic::tcp_accept only accepts TcpListener"))
            }
        },
    );
    linker.define("lunatic", "tcp_accept", tcp_accept)?;

    Ok(())
}
