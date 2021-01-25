use uptown_funk::{types::CReprWasmType, ToWasm};

#[derive(Copy, Clone)]
#[repr(u16)]
pub enum Status {
    /// No error occurred. System call completed successfully.
    Success = 0,
    /// Argument list too long.
    TooBig = 1,
    /// Permission denied.
    Acces = 2,
    /// Address in use.
    AddrInUse = 3,
    /// Address not available.
    AddrNotAvail = 4,
    /// Address family not supported.
    AddrFamilyNotSupported = 5,
    /// Resource unavailable, or operation would block.
    Again = 6,
    /// Connection already in progress.
    Already = 7,
    /// Bad file descriptor.
    Badf = 8,
    /// Bad message.
    BadMsg = 9,
    /// Device or resource busy.
    Busy = 10,
    /// Operation canceled.
    Canceled = 11,
    /// No child processes.
    Child = 12,
    /// Connection aborted.
    ConnAborted = 13,
    /// Connection refused.
    ConnRefused = 14,
    /// Connection reset.
    ConnReset = 15,
    /// Resource deadlock would occur.
    Deadlk = 16,
    /// Destination address required.
    DestAddrReq = 17,
    /// Mathematics argument out of domain of function.
    Dom = 18,
    /// Reserved.
    Dquot = 19,
    /// File exists.
    Exist = 20,
    /// Bad address.
    Fault = 21,
    /// File too large.
    Fbig = 22,
    /// Host is unreachable.
    HostUnreach = 23,
    /// Identifier removed.
    IdRemoved = 24,
    /// Illegal byte sequence.
    IllegalSeq = 25,
    /// Operation in progress.
    InProgress = 26,
    /// Interrupted function.
    Intr = 27,
    /// Invalid argument.
    Inval = 28,
    /// I/O error.
    Io = 29,
    /// Socket is connected.
    IsConn = 30,
    /// Is a directory.
    IsDir = 31,
    /// Too many levels of symbolic links.
    Loop = 32,
    /// File descriptor value too large.
    Mfile = 33,
    /// Too many links.
    Mlink = 34,
    /// Message too large.
    MsgSize = 35,
    /// Reserved.
    Multihop = 36,
    /// Filename too long.
    NameTooLong = 37,
    /// Network is down.
    NetDown = 38,
    /// Connection aborted by network.
    NetReset = 39,
    /// Network unreachable.
    NetUnreach = 40,
    /// Too many files open in system.
    Nfile = 41,
    /// No buffer space available.
    NoBufs = 42,
    /// No such device.
    NoDev = 43,
    /// No such file or directory.
    NoEnt = 44,
    /// Executable file format error.
    NoExec = 45,
    /// No locks available.
    NoLck = 46,
    /// Reserved.
    NoLink = 47,
    /// Not enough space.
    NoMem = 48,
    /// No message of the desired type.
    NoMsg = 49,
    /// Protocol not available.
    NoProtoOpt = 50,
    /// No space left on device.
    NoSpace = 51,
    /// Function not supported.
    NoSys = 52,
    /// The socket is not connected.
    NotConn = 53,
    /// Not a directory or a symbolic link to a directory.
    NotDir = 54,
    /// Directory not empty.
    NotEmpty = 55,
    /// State not recoverable.
    NotRecoverable = 56,
    /// Not a socket.
    NotSock = 57,
    /// Not supported, or operation not supported on socket.
    NotSup = 58,
    /// Inappropriate I/O control operation.
    NoTty = 59,
    /// No such device or address.
    Nxio = 60,
    /// Value too large to be stored in data type.
    Overflow = 61,
    /// Previous owner died.
    OwnerDead = 62,
    /// Operation not permitted.
    Perm = 63,
    /// Broken pipe.
    Pipe = 64,
    /// Protocol error.
    Proto = 65,
    /// Protocol not supported.
    ProtoNoSupport = 66,
    /// Protocol wrong type for socket.
    Prototype = 67,
    /// Result too large.
    Range = 68,
    /// Read-only file system.
    Rofs = 69,
    /// Invalid seek.
    Spipe = 70,
    /// No such process.
    Srch = 71,
    /// Reserved.
    Stale = 72,
    /// Connection timed out.
    TimedOut = 73,
    /// Text file busy.
    TxtBusy = 74,
    /// Cross-device link.
    Xdev = 75,
    /// Extension: Capabilities insufficient.
    NotCapable = 76,
}

impl CReprWasmType for Status {}

impl ToWasm for Status {
    type To = u32;
    type State = ();

    fn to(
        _state: &mut Self::State,
        _executor: &impl uptown_funk::Executor,
        host_value: Self,
    ) -> Result<Self::To, uptown_funk::Trap> {
        Ok(host_value as u32)
    }
}

impl From<std::io::Result<()>> for Status {
    fn from(r: std::io::Result<()>) -> Self {
        match r {
            Ok(_) => Self::Success,
            Err(e) => {
                match e.kind() {
                    std::io::ErrorKind::NotFound => Status::NoEnt,
                    std::io::ErrorKind::PermissionDenied => Status::Acces,
                    std::io::ErrorKind::ConnectionRefused => Status::ConnRefused,
                    std::io::ErrorKind::ConnectionReset => Status::ConnReset,
                    std::io::ErrorKind::ConnectionAborted => Status::ConnAborted,
                    std::io::ErrorKind::NotConnected => Status::NotConn,
                    std::io::ErrorKind::AddrInUse => Status::AddrInUse,
                    std::io::ErrorKind::AddrNotAvailable => Status::AddrNotAvail,
                    std::io::ErrorKind::BrokenPipe => Status::Pipe,
                    std::io::ErrorKind::AlreadyExists => Status::Exist,
                    std::io::ErrorKind::WouldBlock => Status::Again, // ??
                    std::io::ErrorKind::InvalidInput => Status::Inval,
                    std::io::ErrorKind::InvalidData => Status::Inval, // ??
                    std::io::ErrorKind::TimedOut => Status::TimedOut,
                    std::io::ErrorKind::WriteZero => Status::Inval, // ??{}
                    std::io::ErrorKind::Interrupted => Status::Intr,
                    std::io::ErrorKind::Other => Status::Inval, // ??
                    std::io::ErrorKind::UnexpectedEof => Status::Inval, // ??
                    _ => Status::Inval,                         // ??
                }
            }
        }
    }
}
