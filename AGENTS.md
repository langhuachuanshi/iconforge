# AGENTS.md

## 项目概览

IconForge — Tauri 2.x 桌面应用（Windows）。AI 生图 → 编辑裁剪 → 多格式导出（PNG/ICO/ZIP）。前端 Vue 3 + Element Plus + Pinia，后端 Rust（tauri commands）。

## 目录结构

- `src/views/` — 各功能页（生成/编辑/导出/提取/历史/设置）
- `src/api/client.ts` — 全部 Tauri invoke 封装，前后端契约的唯一入口
- `src-tauri/src/commands/` — Tauri 命令，按功能域分文件
- `src-tauri/src/services/` — 业务逻辑（图像处理/抠图/导出/存储/签名）
- `backend/`、`docker/` — 旧代码，忽略

## 常用命令

```bash
pnpm tauri dev       # 开发启动（端口 1420，前端热重载 + Rust 增量编译）
pnpm tauri build     # 生产构建 .msi
cargo check          # Rust 类型检查（src-tauri/）
cargo test           # Rust 测试（src-tauri/）
pnpm run bump patch  # 发版：版本号同步 + commit + tag + push（触发 CI）
```

## 项目约定

- 图片以 base64 走 IPC；导出用文件路径（前端 dialog → Rust 写文件）。
- 数据落 `%APPDATA%/com.iconforge.app/`（SQLite + 文件系统）。
- 前端路由用 hash 模式。
- 生成页的提示词预览（`GenerateView` 的 `finalPrompt`）是后端 `assemble_guided_prompt` 拼接规则的镜像实现，改任何一侧必须同步另一侧。
- Rust 有单元测试（image/export），前端暂无测试。
- **凭据红线**（通用，必留）：密钥 / cookie / token 只走环境变量或本地配置文件，绝不写进代码、示例默认值和文档。API Key 只存本地 SQLite，不经过网络。
- **完成标准**（通用，必留）：改动以 `pnpm build` + `cargo check`（/ `cargo test`）全过为完成，不含糊。

## UI 规范

- 必须用 Element Plus 组件，禁止自建自定义组件（除非明确要求）。
- 用 EP 自带样式，禁止硬编码样式/颜色，禁止大量自定义 CSS；微调仅通过 EP CSS 变量。
- 默认 dark 主题，使用 Element Plus 官方 dark 方案。

## 注释

- **只写代码表达不了的信息**：为什么这么做、约束、坑、实测结论；不复述代码行为。
- **默认一行**：超过三行的注释，先考虑改代码（换名字、拆函数），而不是继续写注释。
- Rust 导出函数用 `///` 文档注释（以用途开头）。
- **同步红线**：改代码必须同步改注释；过时注释直接删，不留历史。
- 注释用中文，标识符与专有名词保持原文。

## 文档与计划

- `README.md`：只放项目定位、安装、快速开始、文档导航；深入内容一律进 `docs/`。
- `docs/plans/`：开发 / 重构计划。**计划先行**——多步任务先落计划再动手，计划不落盘不动工；每项任务带验收标准与 checkbox，执行中更新勾选，完成后文首标注完成日期。
- `docs/reviews/`：代码审查报告，文件名 `YYYY-MM-DD-<范围>.md`。发现按 P0（正确性·安全）/ P1（应尽快）/ P2（建议）分级，每条 = 位置 + 依据 + 处置。
- 需求文档、调研等深入内容放 `docs/<主题>/`，不放根目录。

## TODO

`TODO.md` 是根目录任务索引。**想到就记**：开发中冒出的想法、还没做成计划的待办，直接按格式追加，不要求先写计划书。

- 想法 → 待办：一句话即可，零门槛入账。
- 待办 → 开发：先把计划落到 `docs/plans/`，条目补上计划链接，移入"进行中"。
- 完成：打勾标日期移入"已完成"；有产出的（报告 / 计划）把链接写进条目。

## 更新日志

- 根目录 `CHANGELOG.md`，格式统一中文：

  ```markdown
  # 更新日志

  ## [0.2.1] - 2026-09-14

  ### 新增
  - 条目

  ### 修复
  - 条目

  ### 变更 / 移除
  - 条目
  ```

- 版本条目从上到下按时间倒序，最新在最上。
- 条目与 tag 一一对应：**先补条目，后打 tag**。

## Tag 规范

- 语义化版本 `vX.Y.Z`（semver），由 `pnpm run bump <patch|minor|major|x.y.z>` 统一管理——脚本同步 `package.json` / `src-tauri/Cargo.toml` / `src-tauri/tauri.conf.json` 三处版本号，并自动 commit + tag + push。
- 推送 `v*` tag 触发 `.github/workflows/release.yml` 在 `windows-latest` 构建 MSI 并发布到 Releases。
- 发 tag 前置：build / test 全过；CHANGELOG 已有本次条目（先补条目，后打 tag）。

## 提交与发版

- **主动提交**：每完成一个可独立验证的任务就主动 commit，不等用户提醒；阶段性收尾主动 push（用户明确说"先不推"的除外）。
- **提交信息**：`<类型>(<范围>): 中文描述`，类型取 feat / fix / docs / refactor / test / chore；一次提交只说一件事。
- **提交质量线**：提交前 build / test 全过，不提交半成品；收尾时工作区不留未提交的改动。
- **tag / 发版必须等明确指令**：只有用户说出"发布新版本""打 tag"之类的明确指示才进入发版流程，**绝不主动打 tag**。
- **发版流程**（用户下达指令后按序执行）：
  1. 文档同步：依据自上次 tag 以来的全部提交写 CHANGELOG 条目，同步 README、docs/ 下文档。
  2. 验证：build / test 全过。
  3. `docs:` 提交文档同步。
  4. `pnpm run bump <版本>`（自动 commit + tag + push，触发 release CI）。

## 行为准则

以下准则偏向谨慎而非速度，目的是减少常见编码失误。

1. **先想清楚再动手**：动手前说出假设，不确定就问；有多种理解列出来让用户选；有更简单的方案直说；不清楚就停下来指出哪里不明白。
2. **简单优先**：用最少的代码解决问题；不加没要求的功能与"灵活性"；不为不可能的场景做错误处理；能用 50 行就不写 200 行。
3. **精准修改**：只动该动的——不顺手改进旁边的代码，不重构没坏的东西，匹配已有风格；只清理自己改动产生的孤立代码；每一行改动都应能追溯到需求。
4. **目标驱动**：把任务转成可验证目标（加校验 → 先写测试让它通过；修 bug → 先写复现测试；重构 → 前后测试都过）；多步任务先列"步骤 → 验证检查点"。
