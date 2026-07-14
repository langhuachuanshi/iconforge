# CLAUDE.md

## 项目

IconForge — Tauri 2.x 桌面应用。AI 生图 → 编辑裁剪 → 多格式导出（PNG/ICO/ZIP）。

## 命令

```bash
pnpm tauri dev       # 开发启动（端口 1420）
pnpm tauri build     # 生产构建 .msi
cargo check          # Rust 类型检查（src-tauri/）
cargo test           # Rust 测试（src-tauri/）
```

## 技术栈

- **前端**：Vue 3 + Vite + Element Plus + Pinia，`invoke()` IPC 通信，hash 路由
- **后端**：Rust (Tauri commands)，`src-tauri/src/`，reqwest + SQLite + image crate
- **数据流**：图片以 base64 传 IPC，导出用文件路径（前端 dialog → Rust 写文件）
- **存储**：SQLite + 文件系统 → `%APPDATA%/com.iconforge.app/`

## UI 规范

- **必须用 Element Plus 组件**，禁止自建自定义组件（除非明确要求）
- **用 EP 自带样式**，禁止硬编码样式/颜色，禁止大量自定义 CSS；微调仅通过 EP CSS 变量
- **默认 dark 主题**，使用 Element Plus 官方 dark 方案

## 约束

- Rust 有单元测试（image/export），前端暂无测试
- 抠图（remove_background）未实现，返回占位错误
- API Key 存本地 SQLite，不经过网络
- `backend/` 和 `docker/` 是旧代码，忽略
