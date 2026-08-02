use std::path::PathBuf;
use std::fs;

use chrono::Utc;
use parking_lot::Mutex;
use rusqlite::Connection;
use uuid::Uuid;

use crate::error::AppError;
use crate::models::{IconMeta, VersionMeta};

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

CREATE TABLE IF NOT EXISTS icon_versions (
    id          TEXT PRIMARY KEY,
    icon_id     TEXT NOT NULL,
    version_no  INTEGER NOT NULL,
    created_at  TEXT NOT NULL,
    filename    TEXT NOT NULL,
    note        TEXT NOT NULL DEFAULT ''
);

CREATE INDEX IF NOT EXISTS idx_versions_icon ON icon_versions(icon_id, version_no);
";

const MIGRATE_SQL: &str = "
ALTER TABLE providers ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1;
ALTER TABLE providers ADD COLUMN model TEXT NOT NULL DEFAULT '';
ALTER TABLE providers ADD COLUMN sort_order INTEGER NOT NULL DEFAULT 0;
ALTER TABLE providers ADD COLUMN supported_sizes TEXT NOT NULL DEFAULT '1024x1024';
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

    // ── 图标编辑版本（工程文件存档点）──
    // 每个图标可存多个编辑版本（手动保存时落盘），上限 MAX_VERSIONS_PER_ICON。
    // 版本 PNG 存 base_dir/icon_versions/ 子目录，与原图 {id}.png 分开。

    const MAX_VERSIONS_PER_ICON: usize = 10;

    /// 版本文件目录（懒创建）
    fn versions_dir(&self) -> PathBuf {
        let d = self.base_dir.join("icon_versions");
        if !d.exists() { let _ = fs::create_dir_all(&d); }
        d
    }

    /// 保存一个新版本：写 PNG + 插 DB，超上限时淘汰最早的（连文件）
    pub fn save_version(
        &self,
        icon_id: &str,
        image_bytes: &[u8],
        note: &str,
    ) -> Result<VersionMeta, AppError> {
        let dir = self.versions_dir();
        let conn = self.conn.lock();

        // 当前最大版本号 + 1
        let next_no: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(version_no), 0) + 1 FROM icon_versions WHERE icon_id = ?1",
                rusqlite::params![icon_id],
                |row| row.get(0),
            )
            .unwrap_or(1);

        let version_id = Uuid::new_v4().simple().to_string()[..12].to_string();
        let created_at = Utc::now().to_rfc3339();
        let filename = format!("{}_v{}.png", icon_id, next_no);
        let file_path = dir.join(&filename);
        fs::write(&file_path, image_bytes)?;

        conn.execute(
            "INSERT INTO icon_versions (id, icon_id, version_no, created_at, filename, note) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![version_id, icon_id, next_no, created_at, filename, note],
        )?;

        // 淘汰超额的最早版本
        let overflow: i64 = conn.query_row(
            "SELECT COUNT(*) FROM icon_versions WHERE icon_id = ?1",
            rusqlite::params![icon_id], |row| row.get(0),
        ).unwrap_or(0);
        if overflow > Self::MAX_VERSIONS_PER_ICON as i64 {
            // 取最早的版本文件名并删文件
            let old_file: Option<String> = conn.query_row(
                "SELECT filename FROM icon_versions WHERE icon_id = ?1 ORDER BY version_no ASC LIMIT 1",
                rusqlite::params![icon_id], |row| row.get(0),
            ).optional().ok().flatten();
            if let Some(f) = old_file {
                let p = dir.join(&f);
                if p.exists() { let _ = fs::remove_file(&p); }
            }
            conn.execute(
                "DELETE FROM icon_versions WHERE id = (
                    SELECT id FROM icon_versions WHERE icon_id = ?1 ORDER BY version_no ASC LIMIT 1
                )",
                rusqlite::params![icon_id],
            )?;
        }

        Ok(VersionMeta {
            id: version_id,
            icon_id: icon_id.to_string(),
            version_no: next_no,
            created_at,
            note: note.to_string(),
        })
    }

    /// 列出某图标所有版本（最新在前）
    pub fn list_versions(&self, icon_id: &str) -> Result<Vec<VersionMeta>, AppError> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, icon_id, version_no, created_at, note FROM icon_versions WHERE icon_id = ?1 ORDER BY version_no DESC",
        )?;
        let rows = stmt.query_map(rusqlite::params![icon_id], |row| {
            Ok(VersionMeta {
                id: row.get(0)?,
                icon_id: row.get(1)?,
                version_no: row.get(2)?,
                created_at: row.get(3)?,
                note: row.get(4)?,
            })
        })?;
        let mut v = Vec::new();
        for r in rows { v.push(r?); }
        Ok(v)
    }

    /// 取某版本最新一条的文件字节（用于"载入最新版本"）
    pub fn latest_version_bytes(&self, icon_id: &str) -> Result<Option<Vec<u8>>, AppError> {
        let conn = self.conn.lock();
        let filename: Option<String> = conn.query_row(
            "SELECT filename FROM icon_versions WHERE icon_id = ?1 ORDER BY version_no DESC LIMIT 1",
            rusqlite::params![icon_id], |row| row.get(0),
        ).optional().ok().flatten();
        drop(conn);
        match filename {
            Some(f) => {
                let p = self.versions_dir().join(&f);
                if p.exists() { Ok(Some(fs::read(&p)?)) } else { Ok(None) }
            }
            None => Ok(None),
        }
    }

    /// 按 version_id 加载版本文件字节
    pub fn version_bytes_by_id(&self, version_id: &str) -> Result<Option<Vec<u8>>, AppError> {
        let conn = self.conn.lock();
        let filename: Option<String> = conn.query_row(
            "SELECT filename FROM icon_versions WHERE id = ?1",
            rusqlite::params![version_id], |row| row.get(0),
        ).optional().ok().flatten();
        drop(conn);
        match filename {
            Some(f) => {
                let p = self.versions_dir().join(&f);
                if p.exists() { Ok(Some(fs::read(&p)?)) } else { Ok(None) }
            }
            None => Ok(None),
        }
    }

    /// 删除某版本（文件 + DB 行）
    pub fn delete_version(&self, version_id: &str) -> Result<bool, AppError> {
        let conn = self.conn.lock();
        let filename: Option<String> = conn.query_row(
            "SELECT filename FROM icon_versions WHERE id = ?1",
            rusqlite::params![version_id], |row| row.get(0),
        ).optional().ok().flatten();
        if let Some(f) = filename {
            let p = self.versions_dir().join(&f);
            if p.exists() { let _ = fs::remove_file(&p); }
            conn.execute("DELETE FROM icon_versions WHERE id = ?1", rusqlite::params![version_id])?;
            Ok(true)
        } else {
            Ok(false)
        }
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
            ("tongyi", "通义万相", "阿里云百炼 通义万象（DashScope）", "https://bailian.console.aliyun.com", "https://dashscope.aliyuncs.com", "wanx2.1-t2i-turbo", "1024x1024,720x1280,1280x720"),
            ("doubao", "豆包 Seedream", "字节跳动火山引擎", "https://console.volcengine.com/ark", "https://ark.cn-beijing.volces.com/api/v3/images/generations", "doubao-seedream-2.0", "1024x1024,1280x720,720x1280"),
            ("cogview", "智谱 CogView", "智谱 AI 开放平台", "https://bigmodel.cn", "https://open.bigmodel.cn/api/paas/v4/images/generations", "cogview-3", "1024x1024,768x768"),
        ];

        for (idx, (id, name, notes, website, endpoint, model, sizes)) in defaults.iter().enumerate() {
            conn.execute(
                "INSERT INTO providers (id, name, notes, website, api_key, endpoint, model, is_builtin, enabled, sort_order, created_at, supported_sizes)
                 VALUES (?1, ?2, ?3, ?4, '', ?5, ?6, 1, 1, ?7, ?8, ?9)",
                rusqlite::params![id, name, notes, website, endpoint, model, idx, now, sizes],
            )?;
        }
        Ok(())
    }

    pub fn list_providers(&self) -> Result<Vec<crate::models::ProviderEntry>, AppError> {
        use crate::models::ProviderEntry;
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, notes, website, api_key, endpoint, model, is_builtin, enabled, supported_sizes FROM providers ORDER BY sort_order ASC, created_at ASC",
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
                supported_sizes: row.get(9)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn add_provider(&self, req: &crate::models::ProviderUpsertRequest) -> Result<crate::models::ProviderEntry, AppError> {
        use crate::models::ProviderEntry;
        let generated_id = Uuid::new_v4().simple().to_string()[..8].to_string();
        let id = req.id.as_deref().unwrap_or(&generated_id);
        let now = Utc::now().to_rfc3339();
        let sizes = req.supported_sizes.as_deref().unwrap_or("1024x1024");
        let conn = self.conn.lock();
        let max_order: i32 = conn.query_row("SELECT COALESCE(MAX(sort_order), -1) FROM providers", [], |r| r.get(0))?;
        conn.execute(
            "INSERT INTO providers (id, name, notes, website, api_key, endpoint, model, is_builtin, enabled, sort_order, created_at, supported_sizes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, 1, ?8, ?9, ?10)",
            rusqlite::params![id, req.name, req.notes.as_deref().unwrap_or(""), req.website.as_deref().unwrap_or(""), req.api_key, req.endpoint, req.model, max_order + 1, now, sizes],
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
            supported_sizes: sizes.to_string(),
        })
    }

    pub fn update_provider(&self, id: &str, req: &crate::models::ProviderUpsertRequest) -> Result<(), AppError> {
        let conn = self.conn.lock();
        let sizes = req.supported_sizes.as_deref().unwrap_or("1024x1024");
        let affected = conn.execute(
            "UPDATE providers SET name=?1, notes=?2, website=?3, api_key=?4, endpoint=?5, model=?6, supported_sizes=?7 WHERE id=?8",
            rusqlite::params![req.name, req.notes.as_deref().unwrap_or(""), req.website.as_deref().unwrap_or(""), req.api_key, req.endpoint, req.model, sizes, id],
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn open_test_storage() -> (TempDir, Storage) {
        let dir = TempDir::new().unwrap();
        let storage = Storage::new(dir.path().to_path_buf()).unwrap();
        (dir, storage)
    }

    fn fake_png() -> Vec<u8> {
        // 最小 PNG 字节（1x1 透明），只为写文件用
        vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
    }

    #[test]
    fn test_save_and_list_versions() {
        let (_dir, storage) = open_test_storage();
        // 先建一个 icon 记录（外键约束不强制，但语义上需要）
        storage.save_icon(&fake_png(), "test", "", "tongyi").unwrap();
        let icon_id = storage.list_icons(10, 0).unwrap()[0].id.clone();

        // 存 3 个版本
        for i in 0..3 {
            storage.save_version(&icon_id, &fake_png(), &format!("edit {i}")).unwrap();
        }
        let versions = storage.list_versions(&icon_id).unwrap();
        assert_eq!(versions.len(), 3);
        // 最新在前：version_no 倒序
        assert_eq!(versions[0].version_no, 3);
        assert_eq!(versions[2].version_no, 1);
    }

    #[test]
    fn test_latest_version_bytes() {
        let (_dir, storage) = open_test_storage();
        storage.save_icon(&fake_png(), "test", "", "").unwrap();
        let icon_id = storage.list_icons(10, 0).unwrap()[0].id.clone();

        // 无版本时返回 None
        assert!(storage.latest_version_bytes(&icon_id).unwrap().is_none());

        // 存一个版本，内容为特定字节
        let payload = vec![1u8, 2, 3, 4];
        storage.save_version(&icon_id, &payload, "").unwrap();
        let bytes = storage.latest_version_bytes(&icon_id).unwrap().unwrap();
        assert_eq!(bytes, payload);
    }

    #[test]
    fn test_version_limit_eviction() {
        let (_dir, storage) = open_test_storage();
        storage.save_icon(&fake_png(), "test", "", "").unwrap();
        let icon_id = storage.list_icons(10, 0).unwrap()[0].id.clone();

        // 存超过上限（10）个版本
        for i in 0..(Storage::MAX_VERSIONS_PER_ICON + 3) {
            storage.save_version(&icon_id, &fake_png(), &format!("v{i}")).unwrap();
        }
        let versions = storage.list_versions(&icon_id).unwrap();
        // 不应超过上限
        assert_eq!(versions.len(), Storage::MAX_VERSIONS_PER_ICON);
        // 最早的被淘汰：version_no 应从 4 开始（1,2,3 被删），最新的仍在
        let min_no = versions.iter().map(|v| v.version_no).min().unwrap();
        let max_no = versions.iter().map(|v| v.version_no).max().unwrap();
        assert_eq!(min_no, 4, "最早的版本应被淘汰");
        assert_eq!(max_no, (Storage::MAX_VERSIONS_PER_ICON + 3) as i64);
    }

    #[test]
    fn test_delete_version() {
        let (_dir, storage) = open_test_storage();
        storage.save_icon(&fake_png(), "test", "", "").unwrap();
        let icon_id = storage.list_icons(10, 0).unwrap()[0].id.clone();
        let meta = storage.save_version(&icon_id, &fake_png(), "").unwrap();

        assert!(storage.delete_version(&meta.id).unwrap());
        // 再删返回 false
        assert!(!storage.delete_version(&meta.id).unwrap());
        assert!(storage.list_versions(&icon_id).unwrap().is_empty());
    }
}
