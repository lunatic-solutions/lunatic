use super::{FromWasmDirect, FromWasmSimple, ToWasmDirect, ToWasmSimple};

impl FromWasmSimple for bool {
    type From = u32;
    fn from(v: u32) -> Self {
        v != 0
    }
}

impl FromWasmSimple for u8 {
    type From = u32;
    fn from(v: Self::From) -> Self {
        v as u8
    }
}

impl FromWasmSimple for u16 {
    type From = u32;
    fn from(v: Self::From) -> Self {
        v as u16
    }
}

impl FromWasmDirect for u32 {}
impl FromWasmDirect for i32 {}
impl FromWasmDirect for u64 {}
impl FromWasmDirect for i64 {}
impl FromWasmDirect for f32 {}
impl FromWasmDirect for f64 {}
impl FromWasmDirect for () {}

impl ToWasmSimple for bool {
    type To = u32;
    fn to(v: bool) -> u32 {
        v as u32
    }
}

impl ToWasmSimple for u8 {
    type To = u32;
    fn to(v: u8) -> u32 {
        v as u32
    }
}

impl ToWasmSimple for u16 {
    type To = u32;
    fn to(v: u16) -> u32 {
        v as u32
    }
}

impl ToWasmDirect for u32 {}
impl ToWasmDirect for i32 {}
impl ToWasmDirect for u64 {}
impl ToWasmDirect for i64 {}
impl ToWasmDirect for f32 {}
impl ToWasmDirect for f64 {}
impl ToWasmDirect for () {}
