use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::ai::AiConfig;

const RECENT_LIMIT: usize = 30;

pub struct AppStore {
    conn: Mutex<Connection>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RecentProject {
    pub path: String,
    pub name: String,
    pub last_opened: i64,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionTab {
    pub path: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionState {
    pub tabs: Vec<SessionTab>,
    pub active_path: String,
}

impl AppStore {
    pub fn open() -> Self {
        match Self::try_open() {
            Ok(store) => store,
            Err(err) => {
                eprintln!("打开 SQLite 配置库失败，改用内存库：{err}");
                Self::memory()
            }
        }
    }

    fn try_open() -> Result<Self, String> {
        let path = db_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("无法创建配置目录：{e}"))?;
        }
        let conn = Connection::open(&path).map_err(|e| format!("无法打开配置库：{e}"))?;
        init_schema(&conn)?;
        migrate_ai_json(&conn);
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn memory() -> Self {
        let conn = Connection::open_in_memory().expect("无法创建内存配置库");
        let _ = init_schema(&conn);
        Self {
            conn: Mutex::new(conn),
        }
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, String> {
        self.conn.lock().map_err(|_| "配置库状态锁失败".to_string())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, String> {
        let conn = self.lock()?;
        conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| format!("读取配置失败：{e}"))
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), String> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO settings(key, value) VALUES(?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )
        .map_err(|e| format!("保存配置失败：{e}"))?;
        Ok(())
    }

    pub fn get_json<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, String> {
        match self.get_setting(key)? {
            Some(text) => serde_json::from_str(&text)
                .map(Some)
                .map_err(|e| format!("配置解析失败：{e}")),
            None => Ok(None),
        }
    }

    pub fn set_json<T: Serialize>(&self, key: &str, value: &T) -> Result<(), String> {
        let text = serde_json::to_string(value).map_err(|e| format!("配置序列化失败：{e}"))?;
        self.set_setting(key, &text)
    }

    pub fn recent_projects(&self) -> Result<Vec<RecentProject>, String> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT path, name, last_opened FROM recent_projects
                 ORDER BY last_opened DESC LIMIT ?1",
            )
            .map_err(|e| format!("读取最近项目失败：{e}"))?;
        let rows = stmt
            .query_map(params![RECENT_LIMIT as i64], |row| {
                Ok(RecentProject {
                    path: row.get(0)?,
                    name: row.get(1)?,
                    last_opened: row.get(2)?,
                })
            })
            .map_err(|e| format!("读取最近项目失败：{e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("读取最近项目失败：{e}"))
    }

    pub fn touch_recent(&self, path: &str, name: &str) -> Result<Vec<RecentProject>, String> {
        let path = path.trim();
        if path.is_empty() {
            return self.recent_projects();
        }
        let key = normalize_path(path);
        let display_name = if name.trim().is_empty() {
            name_from_path(path)
        } else {
            name.trim().to_string()
        };
        let now = now_ms();
        {
            let conn = self.lock()?;
            conn.execute(
                "INSERT INTO recent_projects(path_key, path, name, last_opened)
                 VALUES(?1, ?2, ?3, ?4)
                 ON CONFLICT(path_key) DO UPDATE SET
                    path = excluded.path,
                    name = excluded.name,
                    last_opened = excluded.last_opened",
                params![key, path, display_name, now],
            )
            .map_err(|e| format!("记录最近项目失败：{e}"))?;
            conn.execute(
                "DELETE FROM recent_projects WHERE path_key NOT IN (
                    SELECT path_key FROM recent_projects ORDER BY last_opened DESC LIMIT ?1
                 )",
                params![RECENT_LIMIT as i64],
            )
            .map_err(|e| format!("清理最近项目失败：{e}"))?;
        }
        self.recent_projects()
    }

    pub fn remove_recent(&self, path: &str) -> Result<Vec<RecentProject>, String> {
        {
            let conn = self.lock()?;
            conn.execute(
                "DELETE FROM recent_projects WHERE path_key = ?1 OR path = ?2",
                params![normalize_path(path), path],
            )
            .map_err(|e| format!("移除最近项目失败：{e}"))?;
        }
        self.recent_projects()
    }

    pub fn get_session(&self) -> Result<SessionState, String> {
        Ok(self.get_json("session")?.unwrap_or_default())
    }

    pub fn save_session(&self, session: SessionState) -> Result<SessionState, String> {
        self.set_json("session", &session)?;
        Ok(session)
    }
}

fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS recent_projects (
            path_key TEXT PRIMARY KEY,
            path TEXT NOT NULL,
            name TEXT NOT NULL,
            last_opened INTEGER NOT NULL
        );
        ",
    )
    .map_err(|e| format!("初始化配置库失败：{e}"))
}

fn migrate_ai_json(conn: &Connection) {
    let exists: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = 'ai'", [], |row| row.get(0))
        .optional()
        .ok()
        .flatten();
    if exists.is_some() {
        return;
    }
    let Some(path) = data_dir().map(|dir| dir.join("ai.json")) else {
        return;
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    if serde_json::from_str::<AiConfig>(&text).is_err() {
        return;
    }
    let _ = conn.execute(
        "INSERT INTO settings(key, value) VALUES('ai', ?1)",
        params![text],
    );
}

fn db_path() -> Result<PathBuf, String> {
    Ok(data_dir().ok_or_else(|| "无法确定配置目录".to_string())?.join("rebased.db"))
}

fn data_dir() -> Option<PathBuf> {
    if let Ok(appdata) = std::env::var("APPDATA") {
        return Some(PathBuf::from(appdata).join("com.rebased.app"));
    }
    if let Ok(home) = std::env::var("HOME") {
        return Some(PathBuf::from(home).join(".rebased"));
    }
    None
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

fn name_from_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
        .to_string()
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
