# Doubao Voice Input (豆包语音输入)

Windows 语音输入工具，基于豆包 ASR 实现实时语音识别。

## 功能特性

- 🎤 **实时语音识别** - 基于豆包 ASR 的高精度语音识别
- ⌨️ **双击Ctrl触发** - 快速双击 Ctrl 键开始/停止语音输入
- 🛠️ **非标准按键绑定** - 支持小爱同学按钮、媒体键和厂商键
- 📍 **悬浮按钮** - 现代风格可拖动悬浮按钮，左键切换录音，右键退出
- 🔄 **流式识别** - 实时显示识别结果，支持文本修正
- 🎛️ **音频预处理** - 支持本地 VAD 状态、800 ms 尾音平滑、固定增益与可选 AEC3 回声消除
- 🖥️ **系统托盘** - 托盘图标菜单控制，右键访问设置和退出
- ⚙️ **用户设置界面** - 配置 Windows 登录自启、悬浮按钮、VAD、标准快捷键和非标准按键
- ☁️ **云端增强可控** - 可分别关闭异步实体识别与会话结束后的 10 秒流式语音校正
- ⚠️ **启动失败提示** - Windows 启动初始化失败时弹出错误提示，并自动将错误信息复制到剪贴板，便于反馈问题
- 📦 **绿色便携** - 单文件可执行，无需安装

## 快速开始

### 下载使用

1. 从 [Releases](https://github.com/EvanDbg/doubao-ime-win/releases) 下载最新版本
2. 解压到任意目录
3. 运行 `doubao-voice-input.exe`
4. 首次运行会自动注册设备

### 使用方法

1. **快捷键** (双击 Ctrl):
   - 快速双击 `Ctrl` 键开始语音输入
   - 再次双击停止录音，文本自动插入到当前焦点窗口

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

### 绑定小爱同学按钮等非标准按键

打开托盘菜单的“设置...”，点击“录入非标准按键”，然后按下目标按键。录入成功后选择“使用非标准按键”并保存。

- **按下切换**：每次按键在开始和停止之间切换。
- 非标准按键监听只适用于 Windows；程序会保留该按键原有动作，因此小爱同学可能仍会同时响应。
- 也可以直接在 `config.toml` 的 `[hotkey]` 中填写 `binding = "raw"` 及 `raw_vk_code`、`raw_scan_code`、`raw_extended`。

## 发布说明

### v1.2.16（当前版本）

- LLM 润色改为默认关闭，并允许在设置中彻底停用初始化、上下文读取和网络请求。
- 关闭润色时立即作废后台结果，始终保留 ASR 原文；实体识别仍可独立启用。
- 修复干净环境中的 Release 配置文件打包流程。

### v1.2.13

- 完善设置窗口的最小化、关闭、拖动等窗口控制，并补齐快捷键与原始按键录入设置。
- 将 WebView2 用户数据迁移至本地应用数据目录，并改进录音会话切换与云端校正配置。
- 优化 Windows Release 构建流程，统一 CI 与本地产物，同时清理未使用的依赖和旧版界面资源。

### v1.2.12

- 设置窗口与录音悬浮窗迁移至 Tao + Wry，使用离线嵌入式 Vite 前端资源。
- 新增深色 Mica 设置界面、透明录音 HUD 与实时音频电平波形。

### v1.2.11

- LLM 从纯口水词删除扩展为保守语音校正：允许结合前后文纠正高置信度同音/近音误识别和明显语序问题。
- 无把握时保持 ASR 原文，仍禁止扩写、回答问题、翻译以及擅自修改专名、英文、数字和代码。

### v1.2.10

- 按 Streaming、two-pass/three-pass、VAD 分段终态建模：修订结果整体替换当前未固化分段，只有 `is_vad_finished` 固化并异步触发 NER。
- `stream_asr_finish` 与 `SessionFinished` 仅作为阶段/生命周期事件，不再误当成额外一遍文本识别；补齐 `use_twopass_retry`。
- 新增本地 VAD 活动观测、默认 800 ms 尾音平滑和可配置 post-ratio 固定增益，音频内容与帧节奏不被 VAD 截断。
- 新增可选 WebRTC AEC3：通过 WASAPI 获取默认扬声器 loopback 参考流，48 kHz/10 ms 对齐消除回声，初始化失败时自动无损降级。

### v1.2.9

- 运行日志统一写入 EXE 同目录的 `logs` 文件夹，并按本地日期滚动为 `doubao-voice-input-YYYY-MM-DD.log`。

### v1.2.8

- ASR 停止后持续等待 WSS `SessionFinished`，收到后立即结束；异常情况下的兜底超时从 2 秒延长到 30 秒。

### v1.2.7

- 停止录音后继续接收 ASR 尾包，直到服务端 `SessionFinished` 才认定文本完全稳定。
- 保留携带最终文本的 `SessionFinished` 终态身份，避免在普通 final 后约 200ms 过早结束接收。

### v1.2.6

- LLM 清理改为流式接收，最长等待 10 秒；收到完整 `scene.completed` 后立即结束等待。
- SSE delta 仅在内存中聚合，不逐片写入目标应用；完整结果到达后才自动一次性替换。
- 超时、失败或没有有效完整结果时保留 ASR 原文。

### v1.2.5

- 修复旧配置升级后默认切到实验性 24 kHz，导致 ASR 识别率下降的回归；默认恢复为稳定的 16 kHz。
- LLM 改为专门清理口头语、语气词、重复和无意义停顿，不扩写、不改写有效内容。
- 语音结束后最多等待后台清理 3 秒；成功时自动一次性替换，超时或失败保留 ASR 原文。
- 移除完整结果确认窗口，并阻止旧会话结果覆盖新录音。

### v1.2.4

- ASR final 保持立即提交，NER 改为最长 2 秒的异步旁路，不修改当前已提交文本。
- 接入豆包输入法 1.3.7 兼容 NER 与 `scene=5` 润色接口，并加入 SAMI token 缓存。
- 语音会话结束后后台清理口头语、语气词、重复和无意义停顿；不扩写、不改变原意。
- 清理任务总预算为 3 秒；按时返回完整结果则自动一次性替换，超时或失败保留 ASR 原文。
- 使用 UI Automation 限量读取光标前后文，并在自动替换前恢复和校验原目标窗口。
- 新增 NER 与 3 秒口水词清理开关。

### v1.2.3

- 修复托盘菜单无法打开设置窗口，以及关闭设置时应用意外退出的问题。
- 修复按住说话松开后过早停止接收 ASR 响应，导致短句卡顿或最终文字丢失的问题。
- 使用真实 Opus 尾帧结束语音会话，并兼容携带最终文本的 `SessionFinished` 响应。
- 改进 Windows TLS 初始化和设备注册网络兼容性。

## 配置文件

配置文件 `config.toml` 与程序同目录：

```toml
[general]
auto_start = false
language = "zh-CN"

[hotkey]
action = "voice_input"  # "voice_input"、"official_hold" 或 "official_hands_free"
binding = "standard"  # "standard" 或 "raw"
mode = "single_tap"  # "single_tap" (单击)、"double_tap" (双击) 或 "hold" (长按)
standard_key = "Ctrl+Shift+V"  # 单键名或组合键
double_tap_interval_ms = 300  # 双击间隔，100-1000 毫秒

# 非标准按键由设置窗口“录入非标准按键”自动填写，仅 binding = "raw" 时存在：
# [hotkey.raw_key]
# vk_code = 255
# scan_code = 114  # 省略表示匹配任意扫描码（如鼠标侧键）
# extended = false

# official_hold: 将录入的自定义按键按下/松开转发为官方右 Alt 长按模式
# official_hands_free: 点按自定义按键时转发左 Ctrl+Win，之后由官方输入法接管
# 两种官方模式均不启动本程序录音，也不显示本程序悬浮窗口
# 旧版字段 (combo_key / double_tap_key / double_tap_interval / raw_vk_code 等) 首次启动自动迁移

[floating_button]
enabled = true
position_x = 100
position_y = 100

[asr]
vad_enabled = true
aec_enabled = false  # 可选 AEC3，需要默认扬声器 loopback
end_smooth_window_ms = 800  # 本地 VAD 尾音平滑，并透传云端 ASR
post_ratio_gain = 1.0  # 固定麦克风增益，范围 0.25-4.0
audio_quality = "standard"  # 推荐 "standard" (16kHz)；"high_quality" (24kHz) 为实验选项
punctuation_mode = "smart"  # "smart", "spaces", "no_sentence_final", "preserve"

[cloud]
ner_enabled = false  # 异步上传 ASR final，用于后续上下文和候选优化
llm_context_enabled = false  # 读取光标前后的文本作为校正上下文
auto_polish_enabled = false  # 默认关闭；开启后会话结束时流式润色并自动一次性替换
llm_custom_api_enabled = false  # false 使用内置豆包 Scene 5；true 使用自定义 OpenAI 兼容 API
```

LLM 润色默认关闭；关闭时不会初始化或校验润色接口、读取光标上下文或发送润色请求，ASR 原文会直接保留。启用云端增强时，NER 响应不会修改或展示在当前输入框中；LLM 会删除口水词，并在语义高度明确时结合前后文纠正同音/近音误识别和明显语序问题。润色默认使用项目已有的豆包输入法 `scene=5` 接口和本机注册产生的设备凭据，不需要用户 API Key；开启“使用自定义 API”后才会改用用户填写的 OpenAI 兼容 URL、API Key 和模型。响应以 SSE 流式接收，delta 仅在内存中聚合；10 秒内收到完整结果时自动一次性替换 ASR 原文，超时或失败时保留原文。

## 日志导出

日志位于程序同目录的 `logs` 文件夹，按本地日期保存为 `doubao-voice-input-YYYY-MM-DD.log`。反馈问题时退出程序后，将对应日期的日志文件压缩发送即可。日志按天自动切换，不需要修改启动命令或手动重定向输出。

## 从源码构建

### 环境要求

- Rust 1.97.0（由 `rust-toolchain.toml` 固定）
- Windows 10/11 x64
- Visual Studio Build Tools 2022
- CMake
- Protobuf Compiler (protoc)
- Node.js 22+（用于构建嵌入式 Tao/Wry 前端）

### 构建步骤

```powershell
# 克隆项目
git clone https://github.com/EvanDbg/doubao-ime-win.git
cd doubao-ime-win

# 首次构建前安装设置窗口和录音 HUD 的前端依赖
cd frontend
npm ci
cd ..

# 构建锁定依赖、静态 CRT 的 Windows x64 Release 版本
./scripts/build-release.ps1

# 如需仅清理并重建该 Release 目标
./scripts/build-release.ps1 -Clean

# 可执行文件位置
# target/x86_64-pc-windows-msvc/release/doubao-voice-input.exe
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
| 热键监听 | global-hotkey + Windows 低级键盘钩子 |
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

## 许可证

MIT License

## 致谢

- [doubaoime-asr](https://github.com/starccy/doubaoime-asr) - 豆包 ASR 协议参考实现
