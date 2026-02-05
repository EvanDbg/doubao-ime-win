# Doubao Voice Input - 项目目录结构（简化版）

## 📁 推荐的项目目录结构

```
doubao-voice-input/
├── .github/
│   └── workflows/
│       └── ci.yml                 # GitHub Actions CI
│
├── assets/
│   ├── icon.ico                   # 应用图标
│   └── tray_icon.png              # 托盘图标
│
├── docs/
│   └── user-guide.md              # 用户使用指南
│
├── PRD/
│   ├── README.md                  # PRD 文档导航
│   ├── windows-ime-requirements.md # 产品需求文档 v2.0
│   ├── technical-architecture.md  # 技术架构设计 v2.0
│   ├── task-list.md               # 开发任务清单
│   └── project-structure.md       # 本文档
│
├── scripts/
│   └── build-portable.ps1         # Windows 便携版打包脚本
│
├── src/
│   ├── main.rs                    # 程序入口
│   │
│   ├── ui/
│   │   ├── mod.rs
│   │   ├── floating_button.rs     # 悬浮按钮
│   │   ├── system_tray.rs         # 系统托盘
│   │   └── settings_window.rs     # 设置窗口
│   │
│   ├── business/
│   │   ├── mod.rs
│   │   ├── voice_controller.rs    # 语音输入控制器
│   │   ├── text_inserter.rs       # 文本插入服务
│   │   └── hotkey_manager.rs      # 热键管理
│   │
│   ├── asr/
│   │   ├── mod.rs
│   │   ├── client.rs              # ASR 客户端
│   │   ├── protocol.rs            # 协议定义
│   │   └── device_reg.rs          # 设备注册
│   │
│   ├── audio/
│   │   ├── mod.rs
│   │   ├── capture.rs             # 音频采集
│   │   └── processor.rs           # PCM 处理
│   │
│   ├── data/
│   │   ├── mod.rs
│   │   ├── config.rs              # 配置管理
│   │   └── credential.rs          # 凭据存储
│   │
│   └── utils/
│       ├── mod.rs
│       └── logger.rs              # 日志
│
├── tests/
│   ├── integration_test.rs        # 集成测试
│   └── unit/
│       └── text_inserter_test.rs
│
├── .gitignore
├── Cargo.toml                     # Rust 项目配置
├── Cargo.lock
├── config.toml.example            # 配置文件示例
├── LICENSE
├── README.md
└── CHANGELOG.md                   # 版本变更日志
```

---

## 📦 便携版运行时目录结构

```
doubao-voice-portable/
├── doubao-voice-input.exe         # 主程序（单文件）
├── config.toml                    # 配置文件
├── credentials.json               # 凭据文件（加密，自动生成）
├── logs/                          # 日志目录（可选）
│   └── app.log
├── README.md                      # 使用说明
└── LICENSE
```

**目标体积**: < 15MB（所有文件）

---

## 🔧 开发环境配置

### 必需工具
- Rust 1.70+ (stable)
- Windows SDK 10.0.19041.0+
- Visual Studio 2022 Build Tools（可选，用于某些依赖）

### 安装 Rust（中国镜像）
```powershell
# 设置镜像（加速下载）
$env:RUSTUP_DIST_SERVER="https://mirrors.tuna.tsinghua.edu.cn/rustup"
$env:RUSTUP_UPDATE_ROOT="https://mirrors.tuna.tsinghua.edu.cn/rustup/rustup"

# 安装 Rust
iwr https://win.rustup.rs -outfile rustup-init.exe
.\rustup-init.exe
```

### Cargo 配置（加速编译）
创建 `~/.cargo/config.toml`:
```toml
[source.crates-io]
replace-with = 'tuna'

[source.tuna]
registry = "https://mirrors.tuna.tsinghua.edu.cn/git/crates.io-index.git"

[build]
jobs = 4  # 并行编译作业数
```

---

## 📝 文件命名规范

### Rust 源文件
- 模块: `snake_case.rs`
- 示例: `voice_controller.rs`, `text_inserter.rs`

### 配置文件
- TOML: `*.toml`
- JSON: `*.json`

### 文档文件
- Markdown: `kebab-case.md`
- 示例: `user-guide.md`, `api-reference.md`

---

## 🔄 Git 工作流

### 分支策略（简化版）
- `main` - 主分支（稳定版本）
- `develop` - 开发分支
- `feature/*` - 功能分支

### Commit 规范
```
<type>: <subject>

<body>
```

**Type**:
- `feat`: 新功能
- `fix`: 修复 bug
- `docs`: 文档更新
- `refactor`: 代码重构
- `test`: 测试

**示例**:
```
feat: add floating button UI

- Implement draggable circular button
- Add recording animation effect
- Support position persistence
```

---

## 🚀 快速开始（开发）

### 1. 克隆项目
```bash
git clone https://github.com/yourusername/doubao-voice-input.git
cd doubao-voice-input
```

### 2. 安装依赖
```bash
cargo build
```

### 3. 运行开发版
```bash
cargo run
```

### 4. 构建 Release 版
```bash
cargo build --release
```

### 5. 打包便携版
```powershell
.\scripts\build-portable.ps1
```

---

## 📊 与原项目结构对比

### 移除目录
- ❌ `src/tsf_service/` - Windows TSF 框架（不再需要）
- ❌ `src/candidate_engine/` - 候选词引擎
- ❌ `src/dictionary/` - 用户词库

### 简化结果
| 指标 | 原结构 | 简化版 |
|------|--------|--------|
| 源码模块 | 15+ | 8 |
| 代码文件数 | 30+ | 15 |
| 预计代码行数 | 5000+ | 2000-3000 |

---

## 🔍 代码组织原则

### 模块职责
- **ui/**: 纯 UI 逻辑，不包含业务逻辑
- **business/**: 业务逻辑，协调各服务
- **asr/**: ASR 协议实现，独立模块
- **audio/**: 音频采集与处理
- **data/**: 配置和凭据管理

### 依赖关系
```
main.rs
  ↓
ui/ ←→ business/
         ↓
    asr/ + audio/
         ↓
       data/
```

---

## 🧪 测试目录

### 单元测试
```
src/
  business/
    text_inserter.rs
    #[cfg(test)]
    mod tests { ... }
```

### 集成测试
```
tests/
  integration_test.rs  # 端到端测试
```

---

**最后更新**: 2026-02-05（简化版）
