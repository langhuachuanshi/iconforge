# IconForge

AI 图标生成与编辑桌面应用。从概念生成到抠图、编辑、多格式导出的完整工作流，离线可用、数据本地存储。

作者：**Silas** · 奥哈悠工作室 · silas@890625.com

## 功能

按工作流组织，菜单顺序即使用顺序：

- **生成图标** —— AI 文生图，支持通义万相（阿里云百炼）、字节豆包 Seedream、智谱 CogView，自带 14 个图标风格模板。引导式点选 / 专家式直输两种模式，可一键复制拼装后的完整提示词
- **编辑图标** —— PS 风格左侧工具栏 + 右侧抽屉配置面板
  - 智能抠图：本地 ONNX 模型（5 款可选，离线免费）/ 阿里云云端分割
  - 去底色：色键魔棒，支持画布吸管拾色
  - 手动修补：画笔擦除/恢复透明区域
  - 自由裁剪：取景框 + 方向键微调（Shift 加速）
  - 智能裁剪：去除透明边距、按宽高比裁剪
  - 边缘净化：收缩、羽化、去色晕、内描边
  - 形状遮罩：圆角矩形（比例预览）、圆形
  - 调色：亮度、对比度、饱和度
- **导出图标** —— 统一导出页，多尺寸 PNG + 多尺寸 ICO 打包 ZIP，支持加入本地图片批量导出
- **图标提取** —— 从 .exe/.dll/.ocx 提取图标资源，支持拖入 .lnk 快捷方式（自动解析目标 exe）
- **历史记录** —— 生成结果自动保存（含完整提示词，可查看/复制），编辑版本存档（工程文件），载入时优先恢复最新编辑

## 技术栈

| 层 | 技术 |
|----|------|
| 桌面框架 | Tauri 2.x（Rust 后端 + WebView 前端） |
| 前端 | Vue 3 + TypeScript + Vite + Element Plus + Pinia |
| 图像处理 | image + imageproc（Rust） |
| 抠图 | ONNX Runtime（本地模型）/ 阿里云 VIAPI（云端） |
| 数据存储 | SQLite + 文件系统 |

## 快速开始

### 开发

```bash
# 安装依赖
pnpm install

# 开发模式（热重载）
pnpm tauri dev
```

### 构建

```bash
# 构建安装包（.msi，Windows）
pnpm tauri build
# 产物：src-tauri/target/release/bundle/msi/IconForge_<版本>_x64_zh-CN.msi
```

> 只产 MSI（`tauri.conf.json` 的 `bundle.targets` 设为 `["msi"]`）。DirectML.dll（本地抠图 ONNX 后端依赖）会被 Tauri 自动 bundle 进安装包，无需手动配置。

### 发版（CI 自动打包）

通过版本号脚本触发 GitHub Actions 自动构建并发布到 Releases：

```bash
# 自动算下一版本号 + commit + 打 tag + push（触发 CI）
pnpm run bump patch    # 0.1.1 → 0.1.2
pnpm run bump minor    # 0.1.1 → 0.2.0
pnpm run bump major    # 0.1.1 → 1.0.0
# 或精确指定
pnpm run bump 0.2.0
```

推送 `v*` tag 后，`.github/workflows/release.yml` 在 `windows-latest` 上构建 MSI 并发布到 GitHub Releases（首次构建约 20–40 分钟，ort/ONNX 编译较久）。

## 目录结构

```
iconforge/
├── src/                        # Vue 前端
│   ├── views/                  # 各功能页（生成/编辑/导出/提取/历史/设置）
│   ├── api/client.ts           # Tauri invoke 封装
│   ├── stores/workspace.ts     # 工作区状态（当前图、图标 id）
│   ├── utils/clipboard.ts      # 剪贴板工具
│   └── router/                 # 路由配置
├── src-tauri/                  # Rust 后端
│   ├── src/
│   │   ├── commands/           # Tauri 命令（按功能域分文件）
│   │   ├── services/           # 业务逻辑（图像处理/抠图/导出/存储/签名）
│   │   ├── models.rs           # 请求/响应结构体
│   │   └── lib.rs              # 命令注册入口
│   ├── icons/                  # 应用图标资源
│   ├── Cargo.toml
│   └── tauri.conf.json         # Tauri 配置（版本/打包/权限）
├── scripts/bump.mjs            # 版本号同步脚本（patch/minor/major + tag）
├── .github/workflows/release.yml  # tag 触发的发版 CI
└── package.json
```

## 配置

首次使用前，在应用内 **设置** 页配置：

- **生图服务**：至少一个 AI 服务商的 API Key（通义万相 / 豆包 / CogView）
- **抠图服务**（可选）：本地模型（应用内下载）或阿里云 AccessKey

本地抠图、图像编辑、形状遮罩、调色等功能**无需任何 Key**，开箱即用。

## 桌面快捷方式

首次运行时，应用会检测桌面是否已有快捷方式，没有则在右下角弹通知提示创建。可选择：
- **创建快捷方式** —— 一键创建桌面 .lnk（Windows，兼容 OneDrive 桌面重定向）
- **不再提示** —— 持久化配置，永不再弹

---

Copyright © 2026 奥哈悠工作室（Silas）
