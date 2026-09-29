// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if apply_silent_self_install() {
        return;
    }
    tauri_app_lib::run()
}

fn apply_silent_self_install() -> bool {
    let args: Vec<String> = std::env::args().collect();
    let silent = args.iter().any(|item| item.eq_ignore_ascii_case("/s"));
    if !silent {
        return false;
    }
    let Ok(src) = std::env::current_exe() else {
        return false;
    };
    let dest_dir = args
        .iter()
        .find_map(|item| {
            item.strip_prefix("/D=")
                .or_else(|| item.strip_prefix("/d="))
                .filter(|path| !path.is_empty())
                .map(std::path::PathBuf::from)
        })
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA")
                .map(|base| std::path::PathBuf::from(base).join("Rebased"))
        });
    let Some(dest_dir) = dest_dir else {
        return false;
    };
    if std::fs::create_dir_all(&dest_dir).is_err() {
        return false;
    }
    let dest = dest_dir.join("rebased.exe");
    for _ in 0..8 {
        if std::fs::copy(&src, &dest).is_ok() {
            let _ = std::process::Command::new("cmd")
                .args(["/C", "start", "", &dest.to_string_lossy()])
                .spawn();
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(400));
    }
    false
}
