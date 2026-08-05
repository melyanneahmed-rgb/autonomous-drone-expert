#![forbid(unsafe_code)]

//! # `m1b` — interactive Windows hardware validation runner
//!
//! **READ-ONLY BY CONSTRUCTION.** This binary contains **no code path that writes bytes
//! to any port**: it never calls `write_some`, `write_all_with_deadline`, `flush`, or
//! any discard/purge function. It enumerates, opens, reads, and closes. Nothing else.
//!
//! It is spike code for milestone M1B and must never be merged into `main`.
//!
//! Honesty notes printed to the operator:
//! - Opening a serial port asserts control lines (DTR/RTS) exactly as every serial tool
//!   does, including the official configurators. That is a line state, not data.
//! - The board's identity is recorded from USB descriptors only. Firmware identity is
//!   `UNKNOWN — MSP PROHIBITED IN M1B`.
//!
//! Every port-opening command requires `--confirm-usb-only`, by which the operator
//! attests: LiPo disconnected, USB the only power source, no other serial application
//! open, propellers treated as live.

use std::collections::BTreeMap;
use std::io::Read as _;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use spike_windows_serial_transport::backends::serial2_backend::Serial2Backend;
use spike_windows_serial_transport::backends::serialport_backend::SerialportBackend;
use spike_windows_serial_transport::error::{Op, classify_io_error};
use spike_windows_serial_transport::reconnect;
use spike_windows_serial_transport::{OpenConfig, PortInfo, SpikeTransport, TransportError};

const SAFETY: &str = "\
SAFETY GATE — M1B\n\
  * LiPo battery DISCONNECTED. USB is the only power source.\n\
  * No Betaflight Configurator, SpeedyBee App, or any other serial tool running.\n\
  * Propellers are installed on this aircraft: treat them as live anyway.\n\
  * This tool is read-only: it never sends a byte to the board.\n\
  * Opening a port asserts DTR/RTS line state (as every serial tool does). No data.\n";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("help");
    let flags = parse_flags(&args[1.min(args.len())..]);

    println!("== m1b :: {cmd} ==");
    println!("{SAFETY}");

    let result = match cmd {
        "enumerate" => cmd_enumerate(),
        "watch" => cmd_watch(&flags),
        "open-close" => gated(&flags, cmd_open_close),
        "hold" => gated(&flags, cmd_hold),
        "busy" => gated(&flags, cmd_busy),
        "read-timeout" => gated(&flags, cmd_read_timeout),
        "unplug-read" => gated(&flags, cmd_unplug_read),
        "drop-cancel" => gated(&flags, cmd_drop_cancel),
        _ => {
            print_help();
            Ok(())
        }
    };

    match result {
        Ok(()) => println!("\n== m1b :: {cmd} :: done =="),
        Err(e) => {
            println!("\n== m1b :: {cmd} :: ABORTED :: {e} ==");
            std::process::exit(2);
        }
    }
}

fn print_help() {
    println!(
        "commands:\n\
         enumerate                                  list ports via both backends (no open)\n\
         watch [--interval-ms 1000]                 poll enumeration, print deltas (no open)\n\
         open-close  --port COMx --backend B --cycles N --confirm-usb-only\n\
         hold        --port COMx --backend B --hold-secs N --confirm-usb-only\n\
         busy        --port COMx --confirm-usb-only\n\
         read-timeout --port COMx --backend B --timeout-ms 250 --samples 100 --confirm-usb-only\n\
         unplug-read --port COMx --backend B --confirm-usb-only\n\
         drop-cancel --port COMx --backend B --confirm-usb-only\n\
         (B = serialport | serial2)"
    );
}

// ------------------------------------------------------------------ plumbing

type Flags = BTreeMap<String, String>;

fn parse_flags(rest: &[String]) -> Flags {
    let mut flags = Flags::new();
    let mut i = 0;
    while i < rest.len() {
        let key = rest[i].trim_start_matches("--").to_string();
        if i + 1 < rest.len() && !rest[i + 1].starts_with("--") {
            flags.insert(key, rest[i + 1].clone());
            i += 2;
        } else {
            flags.insert(key, "true".to_string());
            i += 1;
        }
    }
    flags
}

fn gated(flags: &Flags, run: fn(&Flags) -> Result<(), String>) -> Result<(), String> {
    if flags.get("confirm-usb-only").map(String::as_str) != Some("true") {
        return Err(
            "refusing to open any port without --confirm-usb-only (see the safety gate above)"
                .into(),
        );
    }
    run(flags)
}

fn need<'f>(flags: &'f Flags, key: &str) -> Result<&'f str, String> {
    flags
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| format!("missing required flag --{key}"))
}

fn num(flags: &Flags, key: &str, default: u64) -> u64 {
    flags
        .get(key)
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[derive(Clone, Copy)]
enum Backend {
    Serialport,
    Serial2,
}

fn backend(flags: &Flags) -> Result<Backend, String> {
    match need(flags, "backend")? {
        "serialport" => Ok(Backend::Serialport),
        "serial2" => Ok(Backend::Serial2),
        other => Err(format!("unknown backend '{other}' (serialport | serial2)")),
    }
}

fn config(read_timeout: Duration) -> OpenConfig {
    OpenConfig {
        baud: 115_200,
        read_timeout,
        write_timeout: read_timeout,
    }
}

fn show(info: &PortInfo) -> String {
    format!(
        "{} vid={} pid={} mfr={:?} product={:?} serial={:?} bare={}",
        info.port_name,
        info.vid
            .map(|v| format!("{v:04x}"))
            .unwrap_or_else(|| "-".into()),
        info.pid
            .map(|v| format!("{v:04x}"))
            .unwrap_or_else(|| "-".into()),
        info.manufacturer,
        info.product,
        info.serial_number,
        info.is_bare()
    )
}

// ------------------------------------------------------------------ commands

fn cmd_enumerate() -> Result<(), String> {
    for (name, support, ports) in [
        (
            SerialportBackend::backend_name(),
            SerialportBackend::metadata_support(),
            SerialportBackend::enumerate(),
        ),
        (
            Serial2Backend::backend_name(),
            Serial2Backend::metadata_support(),
            Serial2Backend::enumerate(),
        ),
    ] {
        match ports {
            Ok(list) => {
                println!(
                    "[HW_RUN] {name} :: {} port(s) :: metadata={support:?}",
                    list.len()
                );
                for p in &list {
                    println!("[HW_RUN]   {}", show(p));
                }
            }
            Err(e) => println!("[HW_RUN] {name} :: enumeration error :: {e}"),
        }
    }
    println!("[HW_RUN] firmware identity: UNKNOWN — MSP PROHIBITED IN M1B (descriptors only)");
    Ok(())
}

fn cmd_watch(flags: &Flags) -> Result<(), String> {
    let interval = Duration::from_millis(num(flags, "interval-ms", 1000));
    println!("[HW_RUN] watching enumeration every {interval:?}; Ctrl+C to stop");
    let mut previous: Vec<PortInfo> = SerialportBackend::enumerate().map_err(|e| e.to_string())?;
    println!("[HW_RUN] baseline: {} port(s)", previous.len());
    for p in &previous {
        println!("[HW_RUN]   {}", show(p));
    }
    loop {
        thread::sleep(interval);
        let current = match SerialportBackend::enumerate() {
            Ok(c) => c,
            Err(e) => {
                println!("[HW_RUN] enumeration error (continuing): {e}");
                continue;
            }
        };
        let delta = reconnect::diff(&previous, &current);
        for name in &delta.appeared {
            let info = current.iter().find(|p| &p.port_name == name);
            println!(
                "[HW_RUN] APPEARED     {}",
                info.map(show).unwrap_or_else(|| name.clone())
            );
        }
        for name in &delta.disappeared {
            println!("[HW_RUN] DISAPPEARED  {name}");
        }
        for (old, new) in &delta.renamed {
            println!("[HW_RUN] RENAMED      {old} -> {new}  (UNIQUE_IDENTITY_MATCH: serial)");
        }
        for (old, new) in &delta.possible_renames {
            println!(
                "[HW_RUN] POSSIBLE     {old} -> {new}  (model-level only — NOT identity; \
                 no automatic rename)"
            );
        }
        previous = current;
    }
}

fn open_by(which: Backend, port: &str, cfg: OpenConfig) -> Result<Opened, TransportError> {
    match which {
        Backend::Serialport => SerialportBackend::open(port, cfg).map(Opened::A),
        Backend::Serial2 => Serial2Backend::open(port, cfg).map(Opened::B),
    }
}

enum Opened {
    A(SerialportBackend),
    B(Serial2Backend),
}

impl Opened {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, TransportError> {
        match self {
            Opened::A(b) => b.read(buf),
            Opened::B(b) => b.read(buf),
        }
    }
    fn close(self) -> Result<(), TransportError> {
        match self {
            Opened::A(b) => b.close(),
            Opened::B(b) => b.close(),
        }
    }
}

fn cmd_open_close(flags: &Flags) -> Result<(), String> {
    let port = need(flags, "port")?;
    let which = backend(flags)?;
    let cycles = num(flags, "cycles", 20);
    let mut ok = 0u64;
    for i in 1..=cycles {
        let started = Instant::now();
        match open_by(which, port, config(Duration::from_millis(250))) {
            Ok(handle) => {
                let opened_in = started.elapsed();
                match handle.close() {
                    Ok(()) => {
                        ok += 1;
                        println!("[HW_RUN] cycle {i}/{cycles} :: open {opened_in:?} :: close ok");
                    }
                    Err(e) => println!("[HW_RUN] cycle {i}/{cycles} :: close error :: {e}"),
                }
            }
            Err(e) => println!(
                "[HW_RUN] cycle {i}/{cycles} :: open error after {:?} :: {e}",
                started.elapsed()
            ),
        }
    }
    println!("[HW_RUN] open/close summary: {ok}/{cycles} clean cycles");
    Ok(())
}

fn cmd_hold(flags: &Flags) -> Result<(), String> {
    let port = need(flags, "port")?;
    let which = backend(flags)?;
    let secs = num(flags, "hold-secs", 120);
    let mut handle =
        open_by(which, port, config(Duration::from_millis(500))).map_err(|e| e.to_string())?;
    println!(
        "[HW_RUN] holding {port} open for {secs}s :: PID {} :: kill me from Task Manager or \
         `taskkill /PID {} /F` for the process-kill test",
        std::process::id(),
        std::process::id()
    );
    let started = Instant::now();
    let mut buf = [0u8; 256];
    while started.elapsed() < Duration::from_secs(secs) {
        match handle.read(&mut buf) {
            Ok(n) if n > 0 => {
                println!("[HW_RUN] board sent {n} unsolicited byte(s) — recorded, not answered")
            }
            Ok(_) => {}
            Err(TransportError::ReadTimeout) => {}
            Err(e) => {
                println!(
                    "[HW_RUN] hold interrupted by {e} after {:?}",
                    started.elapsed()
                );
                return Ok(());
            }
        }
    }
    handle.close().map_err(|e| e.to_string())?;
    println!("[HW_RUN] hold complete, handle released cleanly");
    Ok(())
}

fn cmd_busy(flags: &Flags) -> Result<(), String> {
    let port = need(flags, "port")?;
    let cfg = config(Duration::from_millis(250));

    println!("[HW_RUN] phase 1: first open (this process) via serialport");
    match SerialportBackend::open(port, cfg) {
        Err(e) => {
            println!(
                "[HW_RUN] first open failed :: {e} :: if a `hold` runs in another \
                 terminal this IS the cross-process busy result"
            );
            let e2 = Serial2Backend::open(port, cfg).err();
            println!(
                "[HW_RUN] cross-process probe :: serialport={e} :: serial2={}",
                e2.map(|x| x.to_string())
                    .unwrap_or_else(|| "opened?!".into())
            );
            return Ok(());
        }
        Ok(holder) => {
            println!("[HW_RUN] phase 2: second open attempts while held (same process)");
            let a = SerialportBackend::open(port, cfg).err();
            let b = Serial2Backend::open(port, cfg).err();
            println!(
                "[HW_RUN] in-process busy :: serialport={} :: serial2={}",
                a.map(|x| x.to_string())
                    .unwrap_or_else(|| "opened?!".into()),
                b.map(|x| x.to_string())
                    .unwrap_or_else(|| "opened?!".into())
            );
            holder.close().map_err(|e| e.to_string())?;
            println!("[HW_RUN] holder released; expected classification is PORT_BUSY on both");
        }
    }
    Ok(())
}

fn cmd_read_timeout(flags: &Flags) -> Result<(), String> {
    let port = need(flags, "port")?;
    let which = backend(flags)?;
    let timeout = Duration::from_millis(num(flags, "timeout-ms", 250));
    let samples = num(flags, "samples", 100);

    let mut handle = open_by(which, port, config(timeout)).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 256];
    let mut elapsed_ms: Vec<f64> = Vec::new();
    let mut data_events = 0u64;
    let mut other_errors = 0u64;

    for _ in 0..samples {
        let started = Instant::now();
        match handle.read(&mut buf) {
            Ok(n) if n > 0 => {
                data_events += 1;
                let shown = n.min(16);
                println!(
                    "[HW_RUN] unsolicited data: {n} byte(s), first {shown}: {:02x?}",
                    &buf[..shown]
                );
            }
            Ok(_) => {}
            Err(TransportError::ReadTimeout) => {
                elapsed_ms.push(started.elapsed().as_secs_f64() * 1000.0);
            }
            Err(e) => {
                other_errors += 1;
                println!("[HW_RUN] non-timeout error during sampling :: {e}");
            }
        }
    }
    handle.close().map_err(|e| e.to_string())?;

    elapsed_ms.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
    if elapsed_ms.is_empty() {
        println!(
            "[HW_RUN] no timeout samples collected (data_events={data_events}, \
             other_errors={other_errors})"
        );
    } else {
        let pick = |q: f64| elapsed_ms[((elapsed_ms.len() - 1) as f64 * q) as usize];
        println!(
            "[HW_RUN] read-timeout accuracy over {} samples (target {:?}):",
            elapsed_ms.len(),
            timeout
        );
        println!(
            "[HW_RUN]   min={:.1}ms median={:.1}ms p95={:.1}ms max={:.1}ms \
             data_events={data_events} other_errors={other_errors}",
            elapsed_ms[0],
            pick(0.5),
            pick(0.95),
            elapsed_ms[elapsed_ms.len() - 1]
        );
    }
    Ok(())
}

fn cmd_unplug_read(flags: &Flags) -> Result<(), String> {
    let port = need(flags, "port")?;
    let which = backend(flags)?;
    let mut handle =
        open_by(which, port, config(Duration::from_millis(1000))).map_err(|e| e.to_string())?;
    println!(
        "[HW_RUN] reading in 1s timeout slices. UNPLUG THE USB CABLE NOW. \
         Reporting the first non-timeout outcome."
    );
    let started = Instant::now();
    let mut buf = [0u8; 256];
    loop {
        let slice_started = Instant::now();
        match handle.read(&mut buf) {
            Ok(n) if n > 0 => println!("[HW_RUN] data ({n} bytes) — still connected"),
            Ok(_) => println!("[HW_RUN] zero-byte read result — recorded"),
            Err(TransportError::ReadTimeout) => {
                println!(
                    "[HW_RUN] timeout slice ({:?} total) — still waiting",
                    started.elapsed()
                );
            }
            Err(e) => {
                println!(
                    "[HW_RUN] UNPLUG RESULT :: {e} :: surfaced {:?} into the slice, {:?} \
                     after start",
                    slice_started.elapsed(),
                    started.elapsed()
                );
                println!(
                    "[HW_RUN] expected classification: DEVICE_DISCONNECTED — record what \
                     you actually saw"
                );
                return Ok(());
            }
        }
        if started.elapsed() > Duration::from_secs(120) {
            return Err("no unplug observed within 120s; aborting".into());
        }
    }
}

fn cmd_drop_cancel(flags: &Flags) -> Result<(), String> {
    let port = need(flags, "port")?;
    let which = backend(flags)?;
    let long = Duration::from_secs(30);
    println!(
        "[HW_RUN] drop-cancel: a clone blocks in read (timeout {long:?}); after 3s the \
         ORIGINAL handle is dropped. Question: does the sibling's read return early?"
    );

    let (tx, rx) = mpsc::channel::<(String, Duration)>();

    match which {
        Backend::Serialport => {
            let original =
                SerialportBackend::open(port, config(long)).map_err(|e| e.to_string())?;
            let mut clone = original.try_clone_handle().map_err(|e| e.to_string())?;
            let started = Instant::now();
            thread::spawn(move || {
                let mut buf = [0u8; 256];
                let outcome = match clone.read(&mut buf) {
                    Ok(n) => format!("read returned Ok({n})"),
                    Err(e) => format!("read returned {}", classify_io_error(&e, Op::Read)),
                };
                let _ = tx.send((outcome, started.elapsed()));
            });
            thread::sleep(Duration::from_secs(3));
            original.close().map_err(|e| e.to_string())?;
            println!("[HW_RUN] original handle dropped at t=3s");
        }
        Backend::Serial2 => {
            let original = Serial2Backend::open(port, config(long)).map_err(|e| e.to_string())?;
            let clone = original.try_clone_handle().map_err(|e| e.to_string())?;
            let started = Instant::now();
            thread::spawn(move || {
                let mut buf = [0u8; 256];
                let outcome = match clone.read(&mut buf) {
                    Ok(n) => format!("read returned Ok({n})"),
                    Err(e) => format!("read returned {}", classify_io_error(&e, Op::Read)),
                };
                let _ = tx.send((outcome, started.elapsed()));
            });
            thread::sleep(Duration::from_secs(3));
            original.close().map_err(|e| e.to_string())?;
            println!("[HW_RUN] original handle dropped at t=3s");
        }
    }

    match rx.recv_timeout(long + Duration::from_secs(10)) {
        Ok((outcome, at)) => {
            println!("[HW_RUN] DROP-CANCEL RESULT :: {outcome} at t={at:?}");
            if at < Duration::from_secs(28) {
                println!(
                    "[HW_RUN] interpretation: closing the sibling handle ENDED the read \
                     early — a viable cancellation architecture on this backend"
                );
            } else {
                println!(
                    "[HW_RUN] interpretation: the read ran to its own timeout — sibling \
                     close does NOT cancel; timeout-only architecture confirmed here"
                );
            }
        }
        Err(_) => println!(
            "[HW_RUN] DROP-CANCEL RESULT :: read did not return even after timeout + 10s \
             — the worker is stuck; this is the worst outcome and must be recorded"
        ),
    }
    Ok(())
}
