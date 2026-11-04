//! A replay session: one `rr replay` server process plus an RSP connection.

use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use statesolver_trace::Reg;

use crate::rsp::{self, Connection};
use crate::{Error, Result};

/// Why execution stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stop {
    /// A software or hardware breakpoint was hit.
    Breakpoint,
    /// A watchpoint fired at `addr`.
    Watchpoint {
        /// Watched address that changed.
        addr: u64,
    },
    /// A single step finished.
    Step,
    /// The tracee received a signal.
    Signal(u8),
    /// Reverse execution reached the start of the recording.
    Begin,
    /// The process exited with this status.
    Exited(u8),
    /// The process was killed by this signal.
    Killed(u8),
}

/// gdb amd64 register numbers for the registers StateSolver queries.
fn regno(r: Reg) -> Option<u8> {
    Some(match r {
        Reg::Rax => 0,
        Reg::Rbx => 1,
        Reg::Rcx => 2,
        Reg::Rdx => 3,
        Reg::Rsi => 4,
        Reg::Rdi => 5,
        Reg::Rbp => 6,
        Reg::Rsp => 7,
        Reg::R8 => 8,
        Reg::R9 => 9,
        Reg::R10 => 10,
        Reg::R11 => 11,
        Reg::R12 => 12,
        Reg::R13 => 13,
        Reg::R14 => 14,
        Reg::R15 => 15,
        Reg::Rip => 16,
        Reg::Rflags => 17,
        Reg::Cs => 18,
        Reg::Ss => 19,
        Reg::Ds => 20,
        Reg::Es => 21,
        Reg::Fs => 22,
        Reg::Gs => 23,
        _ => return None,
    })
}

/// A live `rr replay` session.
pub struct Session {
    rr: Child,
    conn: Connection,
    trace: PathBuf,
}

fn free_port() -> Result<u16> {
    Ok(TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
}

fn parse_stop(r: &str) -> Result<Stop> {
    let bytes = r.as_bytes();
    let hex2 = |i: usize| -> Result<u8> {
        r.get(i..i + 2)
            .and_then(|h| u8::from_str_radix(h, 16).ok())
            .ok_or_else(|| Error::Protocol(format!("bad stop reply {r:?}")))
    };
    match bytes.first() {
        Some(b'W') => Ok(Stop::Exited(hex2(1)?)),
        Some(b'X') => Ok(Stop::Killed(hex2(1)?)),
        Some(b'T' | b'S') => {
            let sig = hex2(1)?;
            for field in r.get(3..).unwrap_or("").split(';') {
                let (k, v) = field.split_once(':').unwrap_or((field, ""));
                match k {
                    "replaylog" if v == "begin" => return Ok(Stop::Begin),
                    "watch" | "rwatch" | "awatch" => {
                        return Ok(Stop::Watchpoint {
                            addr: u64::from_str_radix(v, 16).unwrap_or(0),
                        });
                    }
                    "swbreak" | "hwbreak" => return Ok(Stop::Breakpoint),
                    _ => {}
                }
            }
            Ok(if sig == 5 {
                Stop::Step
            } else {
                Stop::Signal(sig)
            })
        }
        _ => Err(Error::Protocol(format!("unexpected stop reply {r:?}"))),
    }
}

impl Session {
    /// Start `rr replay` on a trace directory (or rr's latest trace) and connect.
    ///
    /// # Errors
    /// Fails if rr cannot be started or does not accept a connection in time.
    pub fn start(trace: Option<&Path>) -> Result<Self> {
        let port = free_port()?;
        let mut cmd = Command::new("rr");
        cmd.arg("replay").arg("-s").arg(port.to_string());
        if let Some(t) = trace {
            cmd.arg(t);
        }
        let rr = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| Error::Protocol(format!("cannot run rr: {e}")))?;
        let deadline = Instant::now() + Duration::from_secs(20);
        let stream = loop {
            match TcpStream::connect(("127.0.0.1", port)) {
                Ok(s) => break s,
                Err(_) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => return Err(Error::Protocol(format!("rr did not listen on {port}: {e}"))),
            }
        };
        let mut conn = Connection::new(stream)?;
        conn.request("?")?;
        Ok(Self {
            rr,
            conn,
            trace: trace.map(Path::to_path_buf).unwrap_or_default(),
        })
    }

    /// Trace directory this session replays (empty for rr's latest trace).
    #[must_use]
    pub fn trace(&self) -> &Path {
        &self.trace
    }

    fn resume(&mut self, packet: &str) -> Result<Stop> {
        let r = self.conn.request(packet)?;
        parse_stop(&r)
    }

    /// Continue forward.
    ///
    /// # Errors
    /// Fails on protocol errors.
    pub fn cont(&mut self) -> Result<Stop> {
        self.resume("vCont;c")
    }

    /// Continue backward.
    ///
    /// # Errors
    /// Fails on protocol errors.
    pub fn reverse_cont(&mut self) -> Result<Stop> {
        self.resume("bc")
    }

    /// Execute one instruction forward.
    ///
    /// # Errors
    /// Fails on protocol errors.
    pub fn step(&mut self) -> Result<Stop> {
        self.resume("vCont;s")
    }

    /// Execute one instruction backward.
    ///
    /// # Errors
    /// Fails on protocol errors.
    pub fn reverse_step(&mut self) -> Result<Stop> {
        self.resume("bs")
    }

    fn expect_ok(&mut self, packet: &str) -> Result<()> {
        match self.conn.request(packet)?.as_str() {
            "OK" => Ok(()),
            other => Err(Error::Protocol(format!("{packet}: {other}"))),
        }
    }

    /// Insert a software breakpoint.
    ///
    /// # Errors
    /// Fails if rr rejects it.
    pub fn set_breakpoint(&mut self, addr: u64) -> Result<()> {
        self.expect_ok(&format!("Z0,{addr:x},1"))
    }

    /// Remove a software breakpoint.
    ///
    /// # Errors
    /// Fails if rr rejects it.
    pub fn remove_breakpoint(&mut self, addr: u64) -> Result<()> {
        self.expect_ok(&format!("z0,{addr:x},1"))
    }

    /// Insert a hardware write watchpoint on `len` bytes (1, 2, 4, or 8).
    ///
    /// # Errors
    /// Fails if rr rejects it.
    pub fn set_write_watch(&mut self, addr: u64, len: u8) -> Result<()> {
        self.expect_ok(&format!("Z2,{addr:x},{len:x}"))
    }

    /// Remove a hardware write watchpoint.
    ///
    /// # Errors
    /// Fails if rr rejects it.
    pub fn remove_write_watch(&mut self, addr: u64, len: u8) -> Result<()> {
        self.expect_ok(&format!("z2,{addr:x},{len:x}"))
    }

    /// Read one register.
    ///
    /// # Errors
    /// Fails for registers rr does not expose by number, or on protocol errors.
    pub fn reg(&mut self, r: Reg) -> Result<u64> {
        let n = regno(r)
            .ok_or_else(|| Error::Protocol(format!("register {} unsupported", r.name())))?;
        let reply = self.conn.request(&format!("p{n:x}"))?;
        if reply.starts_with('E') {
            return Err(Error::Protocol(format!("read {}: {reply}", r.name())));
        }
        rsp::decode_le(&reply)
    }

    /// Read memory. Returns an error if any byte is unmapped.
    ///
    /// # Errors
    /// Fails on unmapped memory or protocol errors.
    pub fn mem(&mut self, addr: u64, len: usize) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(len);
        while out.len() < len {
            let chunk = (len - out.len()).min(64 * 1024);
            let at = addr + out.len() as u64;
            let reply = self.conn.request(&format!("m{at:x},{chunk:x}"))?;
            if reply.starts_with('E') || reply.is_empty() {
                return Err(Error::Unmapped(at));
            }
            out.extend(rsp::decode_hex(&reply)?);
        }
        Ok(out)
    }

    /// Read a little-endian u64.
    ///
    /// # Errors
    /// Fails on unmapped memory.
    pub fn read_u64(&mut self, addr: u64) -> Result<u64> {
        let b = self.mem(addr, 8)?;
        Ok(u64::from_le_bytes(b.try_into().unwrap_or_default()))
    }

    /// rr's current event number (`when`).
    ///
    /// # Errors
    /// Fails on protocol errors.
    pub fn when(&mut self) -> Result<u64> {
        let reply = self.conn.request("qRRCmd:when:-1")?;
        let text = String::from_utf8_lossy(&rsp::decode_hex(&reply)?).into_owned();
        text.rsplit(':')
            .next()
            .and_then(|n| n.trim().parse().ok())
            .ok_or_else(|| Error::Protocol(format!("unexpected `when` reply {text:?}")))
    }

    /// `/proc/<pid>/maps` of the replayed process (rr replays by executing
    /// the recorded program as its child, so the mappings match the recording).
    ///
    /// # Errors
    /// Fails if the replay process cannot be found.
    pub fn maps(&self) -> Result<String> {
        let children = std::fs::read_to_string(format!(
            "/proc/{}/task/{}/children",
            self.rr.id(),
            self.rr.id()
        ))?;
        let pid = children
            .split_whitespace()
            .next()
            .ok_or_else(|| Error::Protocol("rr has no replay process".into()))?;
        Ok(std::fs::read_to_string(format!("/proc/{pid}/maps"))?)
    }

    /// Step over a software breakpoint at the current `rip` and re-arm it.
    ///
    /// # Errors
    /// Fails on protocol errors.
    pub fn step_over_breakpoint(&mut self, addr: u64, rearm: bool) -> Result<()> {
        self.remove_breakpoint(addr)?;
        self.step()?;
        if rearm {
            self.set_breakpoint(addr)?;
        }
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.conn.send_only("k");
        std::thread::sleep(Duration::from_millis(20));
        let _ = self.rr.kill();
        let _ = self.rr.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stop_replies() {
        assert_eq!(
            parse_stop("T05thread:p1.1;swbreak:;").unwrap(),
            Stop::Breakpoint
        );
        assert_eq!(parse_stop("T05thread:p1.1;").unwrap(), Stop::Step);
        assert_eq!(parse_stop("T0bthread:p1.1;").unwrap(), Stop::Signal(11));
        assert_eq!(
            parse_stop("T00thread:p1.1;replaylog:begin;").unwrap(),
            Stop::Begin
        );
        assert_eq!(
            parse_stop("T05thread:p1.1;watch:7ff0;").unwrap(),
            Stop::Watchpoint { addr: 0x7ff0 }
        );
        assert_eq!(parse_stop("W00").unwrap(), Stop::Exited(0));
        assert_eq!(parse_stop("X0b").unwrap(), Stop::Killed(11));
    }
}
