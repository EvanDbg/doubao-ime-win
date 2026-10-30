# Doubao Voice Input (豆包语音输入)

Windows 语音输入工具，基于豆包 ASR 实现实时语音识别。

> **English summary:** A lightweight Windows voice-input tool written in Rust. It uses the Doubao ASR service for real-time speech recognition: double-tap `Ctrl` (the default tap gesture), configure single-tap or hold-to-talk (release to stop), or use the floating button to start dictation. The recognized text is typed directly into the currently focused window. Portable single executable, no installation required. Licensed under the MIT License.

## 功能特性

- 🎤 **实时语音识别** - 基于豆包 ASR 的高精度语音识别
- ⌨️ **可配置快捷键触发** - 单键模式默认双击 Ctrl 开始/停止，支持单击切换或长按说话、松开停止
- 📍 **悬浮按钮** - 现代风格可拖动悬浮按钮，左键切换录音，右键退出
- 🔄 **流式识别** - 实时显示识别结果，支持文本修正
- 🖥️ **系统托盘** - 托盘图标菜单控制，右键访问设置和退出
- 📦 **绿色便携** - 单文件可执行，无需安装

## 快速开始

### 下载使用

1. 从 [Releases](https://github.com/EvanDbg/doubao-ime-win/releases) 下载最新版本
2. 解压到任意目录
3. 运行 `doubao-voice-input.exe`
4. 首次运行会自动注册设备

### 使用方法

1. **快捷键** (单键模式默认双击 Ctrl):
   - 快速双击 `Ctrl` 键开始语音输入
   - 再次双击停止录音，文本自动插入到当前焦点窗口
   - 可配置 `trigger = "single"` 单击切换开始/停止，或 `trigger = "hold"` 按下开始、松开停止
   - 长按说话支持 `Ctrl`、`Shift`、`Alt`；其他键配置长按时回退为双击

2. **悬浮按钮**:
   - 🟣 紫色 = 待机状态
   - 🔴 红色 = 正在录音
   - 🟠 橙色 = 处理中
   - **左键点击** = 开始/停止录音
   - **右键点击** = 退出程序（有确认提示）
   - **拖动** = 调整位置

3. **系统托盘**:
   - 右键托盘图标打开菜单
   - 菜单项：开始/停止语音输入、设置、退出

## 配置文件

配置文件 `config.toml` 与程序同目录：

```toml
[general]
auto_start = false
language = "zh-CN"

[hotkey]
mode = "double_tap"
combo_key = "Ctrl+Shift+V"
double_tap_key = "Ctrl"  # 单击/双击/长按共用的触发键
double_tap_interval = 300  # 毫秒
trigger = "double"  # "single" 单击 / "double" 双击（默认）/ "hold" 长按说话，松开停止

[floating_button]
enabled = true
position_x = 100
position_y = 100

[asr]
vad_enabled = true
```

`trigger` 在 `mode = "double_tap"` 时生效；旧配置未写 `trigger` 时默认使用 `"double"`，继续双击 Ctrl 开始/停止。组合键模式 `mode = "combo"` 的行为不变。

Issue #1 的 Typeless 风格单击 Alt 设置：

```toml
[hotkey]
mode = "double_tap"
trigger = "single"
double_tap_key = "Alt"
```

## 从源码构建

### 环境要求

- Rust 1.70+ (stable)
- Windows 10/11 x64
- Visual Studio Build Tools 2022
- CMake
- Protobuf Compiler (protoc)

### 构建步骤

```powershell
# 克隆项目
git clone https://github.com/EvanDbg/doubao-ime-win.git
cd doubao-ime-win

# 构建 Release 版本
cargo build --release

# 可执行文件位置
# target/release/doubao-voice-input.exe
```

### GitHub Actions

项目已配置 GitHub Actions 自动构建：
- 推送到 `main` 分支时自动构建
- 创建 `v*` 标签时自动发布 Release

## 技术架构

| 模块 | 技术 |
|------|------|
| 语言 | Rust |
| 语音识别 | 豆包 ASR (doubaoime-asr 协议) |
| 音频采集 | cpal |
| 音频编码 | Opus |
| 热键监听 | global-hotkey / Win32 键盘钩子（单击 / 双击 / 长按） |
| 系统托盘 | tray-icon |
| 悬浮按钮 | Win32 API (Layered Window) |
| 文本输入 | Windows SendInput API |

## 免责声明

> ⚠️ **注意**
> 
> 本项目基于豆包输入法客户端协议分析实现，非官方 API。
> - 仅供学习研究使用
> - 协议可能随时变更导致功能失效
> - 请遵守相关法律法规

## 贡献

欢迎提交 Issue 和 Pull Request，详见 [CONTRIBUTING.md](CONTRIBUTING.md)。

## 许可证

本项目以 MIT License 发布，详见 [LICENSE](LICENSE) 文件。

## 致谢

- [doubaoime-asr](https://github.com/starccy/doubaoime-asr) - 豆包 ASR 协议参考实现
