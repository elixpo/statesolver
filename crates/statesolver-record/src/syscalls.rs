//! Which bytes the kernel writes into the tracee, per syscall.
//!
//! Replay injects these bytes instead of re-executing the syscall, and
//! analysis treats them as external input. Only successful calls (`ret >= 0`)
//! write. Syscalls with no model here are counted as *uncaptured* in the
//! recording summary; replay must refuse to pass them silently.

/// Where a syscall's output lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Out {
    /// `args[buf]` receives `ret` bytes.
    RetBytes {
        /// Argument index of the buffer.
        buf: usize,
    },
    /// `args[buf]` receives a fixed number of bytes (skipped when the pointer is null).
    Fixed {
        /// Argument index of the buffer.
        buf: usize,
        /// Size in bytes.
        len: u64,
    },
    /// `args[buf]` receives `args[len_arg]` bytes when the buffer is non-null.
    SizedBy {
        /// Argument index of the buffer.
        buf: usize,
        /// Argument index of the length.
        len_arg: usize,
    },
    /// `readv`/`preadv`: `ret` bytes scattered over the iovec array at `args[0+1]`.
    Iovec,
    /// Writes nothing into user memory.
    Nothing,
}

const STAT: u64 = 144;
const RUSAGE: u64 = 144;

/// Output model for an x86-64 syscall number, or `None` if unmodelled.
///
/// One arm per syscall family, kept separate for auditability.
#[must_use]
#[allow(clippy::match_same_arms)]
pub fn model(nr: u64) -> Option<&'static [Out]> {
    use Out::{Fixed, Iovec, Nothing, RetBytes, SizedBy};
    Some(match nr {
        0 | 17 => &[RetBytes { buf: 1 }],        // read, pread64
        19 | 295 => &[Iovec],                    // readv, preadv
        45 => &[RetBytes { buf: 1 }],            // recvfrom (address ignored)
        4..=6 => &[Fixed { buf: 1, len: STAT }], // stat, fstat, lstat
        262 => &[Fixed { buf: 2, len: STAT }],   // newfstatat
        332 => &[Fixed { buf: 4, len: 256 }],    // statx
        217 => &[RetBytes { buf: 1 }],           // getdents64
        228 => &[Fixed { buf: 1, len: 16 }],     // clock_gettime
        229 => &[Fixed { buf: 1, len: 16 }],     // clock_getres
        96 => &[Fixed { buf: 0, len: 16 }],      // gettimeofday
        201 => &[Fixed { buf: 0, len: 8 }],      // time
        63 => &[Fixed { buf: 0, len: 390 }],     // uname
        318 => &[RetBytes { buf: 0 }],           // getrandom
        22 | 293 => &[Fixed { buf: 0, len: 8 }], // pipe, pipe2
        99 => &[Fixed { buf: 0, len: 112 }],     // sysinfo
        79 => &[RetBytes { buf: 0 }],            // getcwd
        89 => &[RetBytes { buf: 1 }],            // readlink
        267 => &[RetBytes { buf: 2 }],           // readlinkat
        13 => &[Fixed { buf: 2, len: 32 }],      // rt_sigaction (oldact, when non-null)
        14 => &[SizedBy { buf: 2, len_arg: 3 }], // rt_sigprocmask (oldset)
        302 => &[Fixed { buf: 3, len: 16 }],     // prlimit64 (old)
        97 => &[Fixed { buf: 1, len: 16 }],      // getrlimit
        61 => &[
            Fixed { buf: 1, len: 4 },
            Fixed {
                buf: 3,
                len: RUSAGE,
            },
        ], // wait4
        // Pure side effects or results only in rax:
        1 | 2 | 3 | 8 | 9 | 10 | 11 | 12 | 20 | 21 | 32 | 33 | 39 | 60 | 62 | 72 | 74 | 77 | 82
        | 83 | 84 | 87 | 90 | 102 | 104 | 107 | 108 | 110 | 158 | 186 | 218 | 231 | 234 | 257
        | 273 | 334 | 435 => &[Nothing],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_writes_ret_bytes_and_write_writes_nothing() {
        assert_eq!(model(0), Some(&[Out::RetBytes { buf: 1 }][..]));
        assert_eq!(model(1), Some(&[Out::Nothing][..]));
        assert_eq!(model(9999), None);
    }
}
