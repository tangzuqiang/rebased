use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TermChunk {
    pub id: String,
    pub data: String,
}

struct Session {
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    child: Mutex<Box<dyn portable_pty::Child + Send + Sync>>,
}

pub struct TerminalHub {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
}

impl TerminalHub {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }
}

fn decode_output(bytes: &[u8]) -> String {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_string();
    }
    encoding_rs::GBK.decode(bytes).0.into_owned()
}

fn shell_command(cwd: &Path) -> CommandBuilder {
    #[cfg(windows)]
    {
        let exe = {
            let mut probe = std::process::Command::new("pwsh");
            probe.arg("-NoLogo").arg("-Command").arg("exit");
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                probe.creation_flags(0x0800_0000);
            }
            if probe.output().map(|out| out.status.success()).unwrap_or(false) {
                "pwsh"
            } else {
                "powershell.exe"
            }
        };
        let mut cmd = CommandBuilder::new(exe);
        cmd.cwd(cwd);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("POWERSHELL_TELEMETRY_OPTOUT", "1");
        cmd.args(["-NoLogo", "-NoProfile"]);
        cmd
    }
    #[cfg(not(windows))]
    {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        let mut cmd = CommandBuilder::new(shell);
        cmd.cwd(cwd);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd
    }
}

fn lock_map(hub: &TerminalHub) -> Result<std::sync::MutexGuard<'_, HashMap<String, Arc<Session>>>, String> {
    hub.sessions.lock().map_err(|_| "终端状态锁失败".to_string())
}

pub fn start(hub: &TerminalHub, app: AppHandle, id: String, cwd: String, cols: u16, rows: u16) -> Result<(), String> {
    {
        let mut map = lock_map(hub)?;
        if let Some(session) = map.get(&id) {
            let dead = session
                .child
                .lock()
                .ok()
                .and_then(|mut child| child.try_wait().ok().flatten())
                .is_some();
            if !dead {
                if let Ok(master) = session.master.lock() {
                    let _ = master.resize(PtySize {
                        rows: rows.max(24),
                        cols: cols.max(80),
                        pixel_width: 0,
                        pixel_height: 0,
                    });
                }
                return Ok(());
            }
            map.remove(&id);
        }
    }

    let cwd = Path::new(&cwd);
    if !cwd.is_dir() {
        return Err("终端工作目录不存在".to_string());
    }

    let pair = native_pty_system()
        .openpty(PtySize {
            rows: rows.max(24),
            cols: cols.max(80),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("无法创建终端：{e}"))?;

    let child = pair
        .slave
        .spawn_command(shell_command(cwd))
        .map_err(|e| format!("无法启动终端：{e}"))?;
    drop(pair.slave);

    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("无法读取终端输出：{e}"))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|e| format!("无法写入终端：{e}"))?;

    let session = Arc::new(Session {
        master: Mutex::new(pair.master),
        writer: Mutex::new(writer),
        child: Mutex::new(child),
    });

    lock_map(hub)?.insert(id.clone(), session);

    let emit_id = id.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let _ = app.emit(
                        "term-data",
                        TermChunk {
                            id: emit_id.clone(),
                            data: decode_output(&buf[..n]),
                        },
                    );
                }
                Err(_) => break,
            }
        }
        let _ = app.emit("term-exit", emit_id);
    });

    Ok(())
}

pub fn write(hub: &TerminalHub, id: &str, data: &str) -> Result<(), String> {
    let map = lock_map(hub)?;
    let session = map.get(id).ok_or_else(|| "终端尚未启动".to_string())?;
    let mut writer = session.writer.lock().map_err(|_| "终端写入锁失败".to_string())?;
    writer
        .write_all(data.as_bytes())
        .and_then(|_| writer.flush())
        .map_err(|e| format!("写入终端失败：{e}"))
}

pub fn resize(hub: &TerminalHub, id: &str, cols: u16, rows: u16) -> Result<(), String> {
    let map = lock_map(hub)?;
    let session = map.get(id).ok_or_else(|| "终端尚未启动".to_string())?;
    let master = session.master.lock().map_err(|_| "终端尺寸锁失败".to_string())?;
    master
        .resize(PtySize {
            rows: rows.max(24),
            cols: cols.max(80),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("调整终端尺寸失败：{e}"))
}

pub fn close(hub: &TerminalHub, id: &str) -> Result<(), String> {
    let mut map = lock_map(hub)?;
    if let Some(session) = map.remove(id) {
        if let Ok(mut child) = session.child.lock() {
            let _ = child.kill();
        }
    }
    Ok(())
}
