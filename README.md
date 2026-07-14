# ICON 工作台

为开发者提供一站式 AI 图标生成解决方案：从概念到多格式图标导出的完整工作流。

## 功能

- **AI 图标生成** —— 多服务商支持，用户自行配置 API Key
  - 通义万相（阿里云）
  - 字节豆包 Seedream（火山方舟）
  - 智谱 CogView
- **图像编辑** —— 居中裁剪为正方形
- **背景移除** —— 本地 rembg（U2Net 模型），免费、离线、无需 API Key
- **多格式导出** —— 一键导出 PNG 多尺寸 + ICO 多尺寸（打包 zip）
- **提示词模板** —— 14 个内置图标风格模板（扁平化、iOS、3D、渐变等）
- **历史记录** —— 生成的图标自动保存，支持查看/重用/删除

> API Key 仅保存在浏览器本地（localStorage），不会上传存储，调用时由后端透传给服务商。

## 端口说明

为避免与其他项目冲突，本项目使用非标准端口：

| 服务 | 端口 |
|------|------|
| 前端 | **22080** |
| 后端 | **22443** |

## 技术栈

| 层 | 技术 |
|----|------|
| 前端 | Vue 3 + TypeScript + Vite + Element Plus + Pinia |
| 后端 | Python 3.11 + FastAPI |
| 图像处理 | Pillow |
| 抠图 | rembg (U2Net，本地运行) |
| 数据存储 | SQLite + 文件系统 |
| 部署 | Docker Compose |

## 目录结构

```
icon-workbench/
├── backend/                # FastAPI 后端
│   ├── app/
│   │   ├── providers/      # AI 服务商抽象（可插拔）
│   │   ├── services/       # 图像处理 / 抠图 / 导出 / 存储
│   │   ├── api/            # 路由（generate/edit/export/history）
│   │   └── data/           # 提示词模板
│   ├── models/             # rembg 模型文件（不入 git，构建时 COPY）
│   └── Dockerfile
├── frontend/               # Vue 3 前端
│   ├── src/
│   │   ├── views/          # 生成 / 编辑 / 历史 / 设置 页
│   │   ├── stores/         # Pinia（settings / workspace）
│   │   └── api/            # 后端 API 封装
│   └── Dockerfile
├── docker/
│   ├── docker-compose.yml       # 开发/本地部署（构建镜像）
│   └── docker-compose.prod.yml  # 生产部署（拉取远程镜像）
├── scripts/
│   ├── kill-ports.sh       # 按端口停止服务
│   └── docker-push.sh      # 构建并推送到腾讯云镜像仓库
└── Makefile                # 常用命令封装
```

---

## 快速开始

### 方式一：容器开发（推荐，日常用这个）

全用容器开发，**本机不需要装 Python / Node**。容器内带热重载，改代码即时生效。

**前置要求**：Docker + Docker Compose

```bash
# 1. 构建开发镜像（仅首次，约 5-10 分钟）
make install

# 2. 启动开发环境（容器内热重载）
make dev

# 3. 访问
#    前端:    http://localhost:22080
#    后端:    http://localhost:22443
#    API 文档: http://localhost:22443/docs

# 4. 查看日志
make logs

# 5. 停止
make down
```

> 💡 首次启动后端需 30-60 秒加载 rembg 模型，之后重启很快。
> 改代码后：后端 uvicorn 自动重启，前端 vite HMR 自动刷新，无需重新 build。

### 方式二：本地开发（不用容器，备选）

如果不想用容器，需要本机有 Python 3.11+ 和 Node 20+：

```bash
# 安装依赖（仅首次）
cd backend && python -m venv .venv && .venv/bin/pip install -r requirements.txt
cd frontend && corepack pnpm install

# 启动（本地热重载）
make dev-local

# 停止用 Ctrl+C
```

---

## Makefile 命令一览

```bash
make help              # 显示所有命令
make install           # 构建开发镜像（首次，约 5-10 分钟）
make dev               # 启动开发环境（容器内热重载）
make dev-local         # 启动本地开发（不用容器）
make down              # 停止开发环境
make logs              # 实时查看日志（Ctrl+C 退出）
make build             # 构建前端生产产物（验证编译）
make up                # 生产部署启动（拉取远程镜像）
make clean             # 停止并清理
```

---

## 配置 API Key

启动应用后，进入 **设置** 页，填入至少一个 AI 服务商的 API Key：

| 服务商 | 获取地址 | 说明 |
|--------|---------|------|
| 通义万相 | https://bailian.console.aliyun.com | 阿里云百炼平台 |
| 字节豆包 | https://console.volcengine.com/ark | 火山方舟 |
| 智谱 CogView | https://bigmodel.cn | 智谱开放平台（有免费额度） |

> 抠图功能使用本地 rembg，**无需任何 Key**。

---

## 部署到服务器

当你确定要发布版本时，推送镜像到腾讯云镜像仓库：

```bash
# 1. 登录（首次）
docker login ccr.ccs.tencentyun.com --username=<你的用户名>

# 2. 构建并推送（替换 <命名空间> 为你的实际值）
./scripts/docker-push.sh <命名空间> latest
# 例如: ./scripts/docker-push.sh icon-workbench latest
```

在服务器上部署（只需要 `docker-compose.prod.yml` 这一个文件）：

```bash
# 1. 登录
docker login ccr.ccs.tencentyun.com --username=<你的用户名>

# 2. 拉取镜像并启动
docker compose -f docker-compose.prod.yml up -d
```

---

## API 文档

后端启动后访问 http://localhost:22443/docs 查看 Swagger 文档。

主要接口：

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/health` | 健康检查 |
| GET | `/api/providers` | 列出所有 AI 服务商 |
| GET | `/api/templates` | 获取提示词模板 |
| POST | `/api/generate` | 生成图标（自动保存到历史） |
| POST | `/api/crop` | 裁剪图片 |
| POST | `/api/remove-bg` | 移除背景（本地 rembg） |
| POST | `/api/export` | 导出多格式（ZIP） |
| GET | `/api/icons` | 历史记录列表 |
| GET | `/api/icons/{id}` | 获取单张图片 |
| DELETE | `/api/icons/{id}` | 删除图标 |

---

## 新增 AI 服务商

Provider 采用可插拔设计，新增一个服务商只需：

1. 在 `backend/app/providers/` 下新建文件，实现 `ImageProvider` 抽象基类
2. 用 `@register` 装饰器注册
3. 在 `backend/app/providers/__init__.py` 中 import 它

无需改动其他代码，前端会自动从 `/api/providers` 拿到新服务商并展示配置项。

---

## 后续规划（V2+）

- 矢量化转换（位图转 SVG）
- 用户系统对接
- 复杂画布编辑（Fabric.js）
- 批量生成
- 异步任务队列（Celery + Redis）
- 桌面应用封装
- 自部署 Stable Diffusion 模型支持
