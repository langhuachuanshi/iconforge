use std::path::PathBuf;
use std::fs;

use chrono::Utc;
use parking_lot::Mutex;
use rusqlite::Connection;
use uuid::Uuid;

use crate::error::AppError;
use crate::models::IconMeta;

const INIT_SQL: &str = "
CREATE TABLE IF NOT EXISTS icons (
    id          TEXT PRIMARY KEY,
    created_at  TEXT NOT NULL,
    concept     TEXT,
    style       TEXT,
    provider    TEXT,
    filename    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_icons_created_at ON icons(created_at DESC);

CREATE TABLE IF NOT EXISTS config (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS providers (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    notes       TEXT NOT NULL DEFAULT '',
    website     TEXT NOT NULL DEFAULT '',
    api_key     TEXT NOT NULL DEFAULT '',
    endpoint    TEXT NOT NULL,
    model       TEXT NOT NULL DEFAULT '',
    is_builtin  INTEGER NOT NULL DEFAULT 0,
    enabled     INTEGER NOT NULL DEFAULT 1,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL
);
";

const MIGRATE_SQL: &str = "
ALTER TABLE providers ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1;
ALTER TABLE providers ADD COLUMN model TEXT NOT NULL DEFAULT '';
ALTER TABLE providers ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;
";

/// SQLite + 文件系统持久化层
pub struct Storage {
    base_dir: PathBuf,
    conn: Mutex<Connection>,
}

impl Storage {
    pub fn base_dir(&self) -> &std::path::PathBuf {
        &self.base_dir
    }

    /// 在指定目录创建/打开数据库，初始化表结构
    pub fn new(base_dir: PathBuf) -> Result<Self, AppError> {
        fs::create_dir_all(&base_dir)?;

        let db_path = base_dir.join("icons.db");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(INIT_SQL)?;

        // 兼容旧库：逐条加列，已存在则忽略
        for sql in MIGRATE_SQL.split(';') {
            let s = sql.trim();
            if !s.is_empty() { let _ = conn.execute(s, []); }
        }

        Ok(Self {
            base_dir,
            conn: Mutex::new(conn),
        })
    }

    /// 保存图标：写入 PNG 文件 + 插入数据库记录
    pub fn save_icon(
        &self,
        image_bytes: &[u8],
        concept: &str,
        style: &str,
        provider: &str,
    ) -> Result<IconMeta, AppError> {
        let icon_id = Uuid::new_v4().simple().to_string()[..12].to_string();
        let created_at = Utc::now().to_rfc3339();
        let filename = format!("{}.png", icon_id);

        // 写入文件
        let file_path = self.base_dir.join(&filename);
        fs::write(&file_path, image_bytes)?;

        // 插入数据库
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO icons (id, created_at, concept, style, provider, filename) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![icon_id, created_at, concept, style, provider, filename],
        )?;

        Ok(IconMeta {
            id: icon_id,
            created_at,
            concept: concept.to_string(),
            style: style.to_string(),
            provider: provider.to_string(),
        })
    }

    /// 列出图标历史（按创建时间倒序）
    pub fn list_icons(&self, limit: i64, offset: i64) -> Result<Vec<IconMeta>, AppError> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, created_at, concept, style, provider FROM icons ORDER BY created_at DESC LIMIT ?1 OFFSET ?2",
        )?;

        let rows = stmt.query_map(rusqlite::params![limit, offset], |row| {
            Ok(IconMeta {
                id: row.get(0)?,
                created_at: row.get(1)?,
                concept: row.get(2)?,
                style: row.get(3)?,
                provider: row.get(4)?,
            })
        })?;

        let mut icons = Vec::new();
        for row in rows {
            icons.push(row?);
        }
        Ok(icons)
    }

    /// 获取图标总数
    pub fn count_icons(&self) -> Result<usize, AppError> {
        let conn = self.conn.lock();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM icons", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    /// 根据 ID 获取图标文件名
    pub fn get_icon_filename(&self, icon_id: &str) -> Result<Option<String>, AppError> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT filename FROM icons WHERE id = ?1")?;
        let result: Option<String> = stmt
            .query_row(rusqlite::params![icon_id], |row| row.get(0))
            .optional()?;
        Ok(result)
    }

    /// 根据 ID 获取图标文件内容（PNG bytes）
    pub fn get_icon_bytes(&self, icon_id: &str) -> Result<Option<Vec<u8>>, AppError> {
        let filename = match self.get_icon_filename(icon_id)? {
            Some(f) => f,
            None => return Ok(None),
        };
        let file_path = self.base_dir.join(&filename);
        if !file_path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&file_path)?;
        Ok(Some(bytes))
    }

    /// 根据 ID 获取图标文件的绝对路径
    pub fn get_icon_path(&self, icon_id: &str) -> Result<Option<PathBuf>, AppError> {
        let filename = match self.get_icon_filename(icon_id)? {
            Some(f) => f,
            None => return Ok(None),
        };
        let file_path = self.base_dir.join(&filename);
        if file_path.exists() {
            Ok(Some(file_path))
        } else {
            Ok(None)
        }
    }

    /// 删除图标：删除文件 + 删除数据库记录
    pub fn delete_icon(&self, icon_id: &str) -> Result<bool, AppError> {
        let filename = match self.get_icon_filename(icon_id)? {
            Some(f) => f,
            None => return Ok(false),
        };

        // 删除文件
        let file_path = self.base_dir.join(&filename);
        if file_path.exists() {
            fs::remove_file(&file_path)?;
        }

        // 删除数据库记录
        let conn = self.conn.lock();
        conn.execute("DELETE FROM icons WHERE id = ?1", rusqlite::params![icon_id])?;

        Ok(true)
    }

    /// 获取配置值
    pub fn get_config(&self, key: &str, default: &str) -> String {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT value FROM config WHERE key = ?1",
            rusqlite::params![key],
            |row| row.get(0),
        )
        .unwrap_or_else(|_| default.to_string())
    }

    /// 设置配置值（upsert）
    pub fn set_config(&self, key: &str, value: &str) -> Result<(), AppError> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO config (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = ?2, updated_at = ?3",
            rusqlite::params![key, value, now],
        )?;
        Ok(())
    }

    /// 获取所有配置
    pub fn get_all_config(&self) -> Result<std::collections::HashMap<String, String>, AppError> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT key, value FROM config")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        let mut map = std::collections::HashMap::new();
        for row in rows {
            let (k, v) = row?;
            map.insert(k, v);
        }
        Ok(map)
    }

    // ── Provider CRUD ──

    /// 预置默认服务商（仅当 providers 表为空时执行）
    pub fn seed_default_providers(&self) -> Result<(), AppError> {
        let conn = self.conn.lock();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM providers", [], |r| r.get(0))?;
        if count > 0 {
            return Ok(());
        }

        let now = Utc::now().to_rfc3339();
        let defaults = [
            ("tongyi", "通义万相", "阿里云 DashScope（异步提交）", "https://bailian.console.aliyun.com", "https://dashscope.aliyuncs.com", "qwen-image-2.0-pro"),
            ("doubao", "豆包 Seedream", "字节跳动火山引擎", "https://console.volcengine.com/ark", "https://ark.cn-beijing.volces.com/api/v3/images/generations", "doubao-seedream-2.0"),
            ("cogview", "智谱 CogView", "智谱 AI 开放平台", "https://bigmodel.cn", "https://open.bigmodel.cn/api/paas/v4/images/generations", "cogview-3"),
        ];

        for (idx, (id, name, notes, website, endpoint, model)) in defaults.iter().enumerate() {
            conn.execute(
                "INSERT INTO providers (id, name, notes, website, api_key, endpoint, model, is_builtin, enabled, sort_order, created_at)
                 VALUES (?1, ?2, ?3, ?4, '', ?5, ?6, 1, 1, ?7, ?8)",
                rusqlite::params![id, name, notes, website, endpoint, model, idx, now],
            )?;
        }
        Ok(())
    }

    pub fn list_providers(&self) -> Result<Vec<crate::models::ProviderEntry>, AppError> {
        use crate::models::ProviderEntry;
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, notes, website, api_key, endpoint, model, is_builtin, enabled FROM providers ORDER BY sort_order ASC, created_at ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ProviderEntry {
                id: row.get(0)?,
                name: row.get(1)?,
                notes: row.get(2)?,
                website: row.get(3)?,
                api_key: row.get(4)?,
                endpoint: row.get(5)?,
                model: row.get(6)?,
                is_builtin: row.get::<_, i32>(7)? != 0,
                enabled: row.get::<_, i32>(8)? != 0,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn add_provider(&self, req: &crate::models::ProviderUpsertRequest) -> Result<crate::models::ProviderEntry, AppError> {
        use crate::models::ProviderEntry;
        let generated_id = Uuid::new_v4().simple().to_string()[..8].to_string();
        let id = req.id.as_deref().unwrap_or(&generated_id);
        let now = Utc::now().to_rfc3339();
        let conn = self.conn.lock();
        let max_order: i32 = conn.query_row("SELECT COALESCE(MAX(sort_order), -1) FROM providers", [], |r| r.get(0))?;
        conn.execute(
            "INSERT INTO providers (id, name, notes, website, api_key, endpoint, model, is_builtin, enabled, sort_order, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, 1, ?8, ?9)",
            rusqlite::params![id, req.name, req.notes.as_deref().unwrap_or(""), req.website.as_deref().unwrap_or(""), req.api_key, req.endpoint, req.model, max_order + 1, now],
        )?;
        Ok(ProviderEntry {
            id: id.to_string(),
            name: req.name.clone(),
            notes: req.notes.clone().unwrap_or_default(),
            website: req.website.clone().unwrap_or_default(),
            api_key: req.api_key.clone(),
            endpoint: req.endpoint.clone(),
            is_builtin: false,
            enabled: true,
            model: req.model.clone(),
        })
    }

    pub fn update_provider(&self, id: &str, req: &crate::models::ProviderUpsertRequest) -> Result<(), AppError> {
        let conn = self.conn.lock();
        let affected = conn.execute(
            "UPDATE providers SET name=?1, notes=?2, website=?3, api_key=?4, endpoint=?5, model=?6 WHERE id=?7",
            rusqlite::params![req.name, req.notes.as_deref().unwrap_or(""), req.website.as_deref().unwrap_or(""), req.api_key, req.endpoint, req.model, id],
        )?;
        if affected == 0 {
            return Err(AppError::NotFound(format!("服务商 {} 不存在", id)));
        }
        Ok(())
    }

    pub fn delete_provider(&self, id: &str) -> Result<(), AppError> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM providers WHERE id=?1", rusqlite::params![id])?;
        Ok(())
    }

    pub fn reorder_providers(&self, ids: &[String]) -> Result<(), AppError> {
        let conn = self.conn.lock();
        for (idx, id) in ids.iter().enumerate() {
            conn.execute("UPDATE providers SET sort_order=?1 WHERE id=?2", rusqlite::params![idx as i32, id])?;
        }
        Ok(())
    }

    pub fn toggle_provider(&self, id: &str, enabled: bool) -> Result<(), AppError> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE providers SET enabled=?1 WHERE id=?2",
            rusqlite::params![enabled as i32, id],
        )?;
        Ok(())
    }
}

/// `rusqlite::OptionalExtension` 的简单替代
trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
