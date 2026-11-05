//! StateSolver tool server for agents (MCP over stdio).
//!
//! One server is bound to one rr trace and keeps a replay session alive.
//! Every tool result is stamped with a query id `Q<n>` (and evidence inside
//! it with `Q<n>.F<k>` / `Q<n>.I<k>`), so an agent can cite exactly which
//! query supports each claim and a checker can re-run it.
//!
//! Study arms (docs/DIRECTION.md §2): the default server is arm **A4**
//! (allocation-lifetime-aware). With `address_only` it is arm **A5**: the
//! lifetime tools are removed and explanations omit allocation and epoch
//! reasoning. Ground-truth `ss_mark` labels are never shown to agents.

#![cfg(target_os = "linux")]

use std::io::{BufRead, Write};
use std::path::PathBuf;

use serde_json::{Value, json};
use statesolver_analysis::{Resolution, TemporalHeap};
use statesolver_replay::provenance::{self, Carrier, Origin};
use statesolver_replay::{Session, explain, queries};
use statesolver_trace::{Event, EventKind, Reg};

/// MCP protocol version this server speaks.
pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// Server configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// rr trace directory (already indexed).
    pub trace: PathBuf,
    /// Arm A5: no allocation-lifetime information.
    pub address_only: bool,
}

/// The tool server.
pub struct Server {
    cfg: Config,
    index: Vec<Event>,
    session: Option<Session>,
    next_query: u64,
}

fn text_result(text: &str, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
}

fn schema(props: &Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": props, "required": required })
}

fn parse_u64(v: &Value) -> Option<u64> {
    match v {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => {
            let s = s.trim();
            s.strip_prefix("0x")
                .map_or_else(|| s.parse().ok(), |h| u64::from_str_radix(h, 16).ok())
        }
        _ => None,
    }
}

impl Server {
    /// A server over an already-loaded allocation index.
    #[must_use]
    pub fn new(cfg: Config, index: Vec<Event>) -> Self {
        Self {
            cfg,
            index,
            session: None,
            next_query: 0,
        }
    }

    fn session(&mut self) -> Result<&mut Session, String> {
        if self.session.is_none() {
            self.session = Some(Session::start(Some(&self.cfg.trace)).map_err(|e| e.to_string())?);
        }
        Ok(self.session.as_mut().expect("just set"))
    }

    /// Index events an agent may see: no ground-truth markers, and no
    /// allocation events in address-only mode.
    fn visible_events(&self) -> impl Iterator<Item = &Event> {
        let address_only = self.cfg.address_only;
        self.index.iter().filter(move |e| match e.kind {
            EventKind::Mark { .. } => false,
            EventKind::Alloc { .. } | EventKind::Free { .. } => !address_only,
            _ => true,
        })
    }

    fn pos_of(&self, event: u64) -> Result<statesolver_trace::RrPos, String> {
        self.visible_events()
            .find(|e| e.id.0 == event)
            .and_then(|e| e.rr)
            .ok_or_else(|| format!("no event E{event} (use the `events` tool to list events)"))
    }

    /// Tool definitions for `tools/list`.
    #[must_use]
    pub fn tools(&self) -> Vec<Value> {
        let mut t = vec![
            json!({"name": "trace_summary", "description": "Program, outcome, crash signal and pc, and event counts of the recorded execution.", "inputSchema": schema(&json!({}), &[])}),
            json!({"name": "events", "description": "List recorded events (id, kind, details). Only these ids are valid `event` arguments for other tools; ids are not contiguous.", "inputSchema": schema(&json!({"kind": {"type": "string", "description": "Optional filter: alloc, free, syscall, signal, exit"}}), &[])}),
            json!({"name": "explain_crash", "description": "START HERE. Automatically analyses the crash: finds the write that produced the bad value, traces where the involved pointer and value came from, and returns a classification with cited evidence (facts and inferences) and provenance chains.", "inputSchema": schema(&json!({}), &[])}),
            json!({"name": "why", "description": "Provenance chain of a register or 8-byte memory value at an event: which instructions produced it, back to its origin.", "inputSchema": schema(&json!({"event": {"type": "integer"}, "reg": {"type": "string"}, "addr": {"type": "string", "description": "hex address"}}), &["event"])}),
            json!({"name": "last_writer", "description": "The most recent instruction that wrote a memory location before an event (address, 1/2/4/8 bytes), with values before and after.", "inputSchema": schema(&json!({"event": {"type": "integer"}, "addr": {"type": "string"}, "len": {"type": "integer"}}), &["event", "addr"])}),
            json!({"name": "state", "description": "Registers, and optionally memory, at an event.", "inputSchema": schema(&json!({"event": {"type": "integer"}, "mem_addr": {"type": "string"}, "mem_len": {"type": "integer"}}), &["event"])}),
        ];
        if !self.cfg.address_only {
            t.push(json!({"name": "allocations", "description": "Heap allocation lifetimes: id, address, size, epoch (generation of that address), created and freed events.", "inputSchema": schema(&json!({"reused_only": {"type": "boolean"}}), &[])}));
            t.push(json!({"name": "lifetime", "description": "Every allocation that occupied an address over time, and which one (if any) owned it at a given event.", "inputSchema": schema(&json!({"addr": {"type": "string"}, "event": {"type": "integer"}}), &["addr"])}));
        }
        t
    }

    /// Run one tool; returns the MCP `tools/call` result.
    pub fn call(&mut self, name: &str, args: &Value) -> Value {
        self.next_query += 1;
        let q = format!("Q{}", self.next_query);
        match self.dispatch(name, args, &q) {
            Ok(body) => {
                // Addresses are shown in hex, as a debugger would.
                let body = serde_json::from_str::<Value>(&body)
                    .map(|mut v| {
                        hexify(&mut v, None);
                        serde_json::to_string_pretty(&v).unwrap_or_default()
                    })
                    .unwrap_or(body);
                text_result(&format!("[{q}] {body}"), false)
            }
            Err(e) => text_result(&format!("[{q}] error: {e}"), true),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn dispatch(&mut self, name: &str, a: &Value, q: &str) -> Result<String, String> {
        let address_only = self.cfg.address_only;
        match name {
            "trace_summary" => {
                let mut counts = std::collections::BTreeMap::new();
                for e in self.visible_events() {
                    *counts.entry(e.kind.name()).or_insert(0u64) += 1;
                }
                let end = self.index.iter().rev().find(|e| {
                    matches!(
                        e.kind,
                        EventKind::Signal { fatal: true, .. } | EventKind::Exit { .. }
                    )
                });
                let end = end.map(|e| match &e.kind {
                    EventKind::Signal { signo, pc, .. } => {
                        format!("killed by signal {signo} at pc {pc:#x} (E{})", e.id.0)
                    }
                    EventKind::Exit { code } => format!("exited with status {code} (E{})", e.id.0),
                    _ => String::new(),
                });
                Ok(serde_json::to_string_pretty(
                    &json!({"trace": self.cfg.trace, "outcome": end, "event_counts": counts}),
                )
                .unwrap_or_default())
            }
            "events" => {
                let kind = a.get("kind").and_then(Value::as_str);
                let list: Vec<Value> = self
                    .visible_events()
                    .filter(|e| kind.is_none_or(|k| e.kind.name() == k))
                    .map(|e| json!({"id": e.id.0, "kind": e.kind.name(), "rr_event": e.rr.map(|p| p.event), "detail": e.kind}))
                    .collect();
                Ok(serde_json::to_string(&list).unwrap_or_default())
            }
            "explain_crash" => {
                let index = self.index.clone();
                let s = self.session()?;
                let mut x =
                    explain::explain_crash(s, &index, address_only).map_err(|e| e.to_string())?;
                for e in &mut x.evidence {
                    e.id = format!("{q}.{}", e.id);
                    for b in &mut e.because {
                        *b = format!("{q}.{b}");
                    }
                }
                x.summary = prefix_ids(&x.summary, q);
                Ok(serde_json::to_string_pretty(&x).unwrap_or_default())
            }
            "why" => {
                let at = self.pos_of(
                    a.get("event")
                        .and_then(parse_u64)
                        .ok_or("`event` is required")?,
                )?;
                let start = if let Some(r) = a.get("reg").and_then(Value::as_str) {
                    Carrier::Reg {
                        reg: Reg::from_name(r).ok_or_else(|| format!("unknown register {r}"))?,
                    }
                } else if let Some(addr) = a.get("addr").and_then(parse_u64) {
                    Carrier::Mem {
                        addr: addr & !7,
                        len: 8,
                    }
                } else {
                    return Err("give `reg` or `addr`".into());
                };
                let index = self.index.clone();
                let s = self.session()?;
                let mut chain =
                    provenance::trace(s, &index, start, at, 16, 4096).map_err(|e| e.to_string())?;
                if address_only && let Origin::Allocation { pos, .. } = chain.origin {
                    chain.origin = Origin::Unknown {
                        reason: format!(
                            "value returned by a call at rr {}:{}",
                            pos.event, pos.ticks
                        ),
                    };
                }
                Ok(serde_json::to_string_pretty(&chain).unwrap_or_default())
            }
            "last_writer" => {
                let at = self.pos_of(
                    a.get("event")
                        .and_then(parse_u64)
                        .ok_or("`event` is required")?,
                )?;
                let addr = a
                    .get("addr")
                    .and_then(parse_u64)
                    .ok_or("`addr` is required")?;
                let len = a.get("len").and_then(parse_u64).unwrap_or(8);
                let len = u8::try_from(len)
                    .ok()
                    .filter(|l| [1, 2, 4, 8].contains(l))
                    .ok_or("`len` must be 1, 2, 4, or 8")?;
                let aligned = addr & !(u64::from(len) - 1);
                let s = self.session()?;
                let w = queries::last_writer(s, aligned, len, at).map_err(|e| e.to_string())?;
                Ok(w.map_or_else(
                    || "no write to that location during the recording".to_string(),
                    |w| serde_json::to_string_pretty(&w).unwrap_or_default(),
                ))
            }
            "state" => {
                let at = self.pos_of(
                    a.get("event")
                        .and_then(parse_u64)
                        .ok_or("`event` is required")?,
                )?;
                let s = self.session()?;
                s.goto(at).map_err(|e| e.to_string())?;
                let mut regs = serde_json::Map::new();
                for r in [
                    Reg::Rax,
                    Reg::Rbx,
                    Reg::Rcx,
                    Reg::Rdx,
                    Reg::Rsi,
                    Reg::Rdi,
                    Reg::Rbp,
                    Reg::Rsp,
                    Reg::R8,
                    Reg::R9,
                    Reg::R10,
                    Reg::R11,
                    Reg::R12,
                    Reg::R13,
                    Reg::R14,
                    Reg::R15,
                    Reg::Rip,
                    Reg::Rflags,
                ] {
                    if let Ok(v) = s.reg(r) {
                        regs.insert(r.name().into(), json!(format!("{v:#x}")));
                    }
                }
                let mut out = json!({"registers": regs});
                if let Some(addr) = a.get("mem_addr").and_then(parse_u64) {
                    let len = a.get("mem_len").and_then(parse_u64).unwrap_or(64).min(4096);
                    let bytes = s
                        .mem(addr, usize::try_from(len).unwrap_or(64))
                        .map_err(|e| e.to_string())?;
                    out["memory"] = json!({"addr": format!("{addr:#x}"), "hex": statesolver_trace::bytes::to_hex(&bytes)});
                }
                Ok(serde_json::to_string_pretty(&out).unwrap_or_default())
            }
            "allocations" if !address_only => {
                let heap = TemporalHeap::from_events(
                    self.index
                        .iter()
                        .cloned()
                        .map(Ok::<_, std::convert::Infallible>),
                )
                .map_err(|e| e.to_string())?;
                let reused: std::collections::HashSet<u64> =
                    heap.reused_addresses().into_iter().collect();
                let only = a
                    .get("reused_only")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let list: Vec<_> = heap
                    .allocations()
                    .iter()
                    .filter(|x| !only || reused.contains(&x.addr))
                    .collect();
                Ok(serde_json::to_string_pretty(&list).unwrap_or_default())
            }
            "lifetime" if !address_only => {
                let addr = a
                    .get("addr")
                    .and_then(parse_u64)
                    .ok_or("`addr` is required")?;
                let heap = TemporalHeap::from_events(
                    self.index
                        .iter()
                        .cloned()
                        .map(Ok::<_, std::convert::Infallible>),
                )
                .map_err(|e| e.to_string())?;
                let at = a.get("event").and_then(parse_u64).map(|e| match heap.resolve(addr, statesolver_trace::EventId(e)) {
                    Resolution::Live { allocation, offset } => json!({"state": "live", "alloc": allocation.id.0, "epoch": allocation.epoch.0, "offset": offset}),
                    Resolution::Freed { allocation, offset } => json!({"state": "freed", "alloc": allocation.id.0, "epoch": allocation.epoch.0, "offset": offset}),
                    Resolution::Unknown => json!({"state": "unknown"}),
                });
                Ok(serde_json::to_string_pretty(&json!({"address": format!("{addr:#x}"), "history": heap.history(addr), "at_event": at})).unwrap_or_default())
            }
            other => Err(format!("unknown tool {other}")),
        }
    }

    /// Handle one JSON-RPC message; `None` for notifications.
    pub fn handle(&mut self, msg: &Value) -> Option<Value> {
        let id = msg.get("id").cloned();
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        let result = match method {
            "initialize" => json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "statesolver", "version": env!("CARGO_PKG_VERSION")},
                "instructions": if self.cfg.address_only {
                    "Query a recorded crash. Cite the [Q<n>] id of the tool result that supports each claim."
                } else {
                    "Query a recorded crash, including heap allocation lifetimes and epochs. Cite the [Q<n>] id (or Q<n>.F<k>) that supports each claim."
                },
            }),
            "ping" => json!({}),
            "tools/list" => json!({"tools": self.tools()}),
            "tools/call" => {
                let p = msg.get("params").cloned().unwrap_or(Value::Null);
                let name = p
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                let args = p.get("arguments").cloned().unwrap_or(json!({}));
                self.call(&name, &args)
            }
            _ if id.is_none() => return None, // notification (e.g. notifications/initialized)
            _ => {
                return Some(
                    json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": format!("method not found: {method}")}}),
                );
            }
        };
        id.map(|id| json!({"jsonrpc": "2.0", "id": id, "result": result}))
    }

    /// Serve newline-delimited JSON-RPC on stdin/stdout until EOF.
    ///
    /// # Errors
    /// Fails on I/O errors.
    pub fn serve_stdio(&mut self) -> std::io::Result<()> {
        let stdin = std::io::stdin();
        let mut out = std::io::stdout().lock();
        for line in stdin.lock().lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let reply = match serde_json::from_str::<Value>(&line) {
                Ok(msg) => self.handle(&msg),
                Err(e) => Some(
                    json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": e.to_string()}}),
                ),
            };
            if let Some(r) = reply {
                writeln!(out, "{r}")?;
                out.flush()?;
            }
        }
        Ok(())
    }
}

/// Field names whose integer values are addresses or raw machine words.
const HEX_FIELDS: [&str; 9] = [
    "addr",
    "address",
    "pc",
    "sp",
    "caller",
    "value",
    "start",
    "end",
    "realloc_of_addr",
];

/// Rewrite address-like integers as `0x…` strings, recursively.
fn hexify(v: &mut Value, key: Option<&str>) {
    match v {
        Value::Object(m) => {
            for (k, x) in m.iter_mut() {
                hexify(x, Some(k));
            }
        }
        Value::Array(a) => {
            // Byte arrays (memory contents) become one hex string.
            if matches!(key, Some("before" | "after"))
                && a.iter().all(|x| x.as_u64().is_some_and(|b| b < 256))
            {
                let bytes: Vec<u8> = a
                    .iter()
                    .filter_map(|x| x.as_u64().and_then(|b| u8::try_from(b).ok()))
                    .collect();
                *v = Value::String(statesolver_trace::bytes::to_hex(&bytes));
                return;
            }
            for x in a.iter_mut() {
                hexify(x, key);
            }
        }
        Value::Number(n) if key.is_some_and(|k| HEX_FIELDS.contains(&k)) => {
            if let Some(x) = n.as_u64() {
                *v = Value::String(format!("{x:#x}"));
            }
        }
        _ => {}
    }
}

/// Prefix bare evidence ids (`F3`, `I1`) in a summary with the query id.
fn prefix_ids(text: &str, q: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let boundary = i == 0 || !chars[i - 1].is_alphanumeric();
        if boundary && (c == 'F' || c == 'I') && chars.get(i + 1).is_some_and(char::is_ascii_digit)
        {
            out.push_str(q);
            out.push('.');
        }
        out.push(c);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(address_only: bool) -> Server {
        Server::new(
            Config {
                trace: PathBuf::from("/nonexistent"),
                address_only,
            },
            Vec::new(),
        )
    }

    #[test]
    fn initialize_and_list_tools() {
        let mut s = server(false);
        let r = s
            .handle(&json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}))
            .unwrap();
        assert_eq!(r["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert!(
            s.handle(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
                .is_none()
        );
        let tools = s
            .handle(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}))
            .unwrap();
        let names: Vec<&str> = tools["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"lifetime") && names.contains(&"explain_crash"));
    }

    #[test]
    fn address_only_arm_hides_lifetime_tools() {
        let mut s = server(true);
        let tools = s
            .handle(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}))
            .unwrap();
        let names: Vec<&str> = tools["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(!names.contains(&"lifetime") && !names.contains(&"allocations"));
        let r = s.call("lifetime", &json!({"addr": "0x10"}));
        assert_eq!(r["isError"], true);
    }

    #[test]
    fn addresses_are_rendered_in_hex() {
        let mut v = json!({"addr": 4_199_108, "id": 3, "detail": {"pc": 255, "size": 32}, "before": [1, 255]});
        hexify(&mut v, None);
        assert_eq!(
            v,
            json!({"addr": "0x4012c4", "id": 3, "detail": {"pc": "0xff", "size": 32}, "before": "01ff"})
        );
    }

    #[test]
    fn results_carry_query_ids_and_marks_are_hidden() {
        let mut s = server(false);
        s.index = vec![Event {
            id: statesolver_trace::EventId(0),
            rr: None,
            kind: EventKind::Mark {
                label: "root-cause:secret".into(),
                pc: 0,
            },
        }];
        let r = s.call("events", &json!({}));
        let text = r["content"][0]["text"].as_str().unwrap();
        assert!(text.starts_with("[Q1]"));
        assert!(
            !text.contains("root-cause"),
            "ground truth must not leak: {text}"
        );
        assert_eq!(
            prefix_ids("Crash (F1) by F2; I1.", "Q4"),
            "Crash (Q4.F1) by Q4.F2; Q4.I1."
        );
    }
}
