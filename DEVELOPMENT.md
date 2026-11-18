# 开发规范与自动化工作流

本文是本仓库的默认开发约定，也是后续 AI 助手在本项目中执行“提交—推送—PR—审查合并—构建—部署—排查”的标准流程。

## 1. 总则

- 只做用户要求的变更；不修复无关问题，不重排无关代码。
- 保留工作区中用户已有的无关改动。提交前必须核对 `git status --porcelain` 和 `git diff --name-only`，只 stage 与本任务相关的文件。
- 不直接推 `main`。所有代码变更都走 feature branch → Pull Request → CI → merge。
- 版本号变更、tag、GitHub Release、对外发布公告必须先取得用户明确确认；修复 bug 不等于授权发布。
- 涉及删除、覆盖、批量文件操作或非本仓库目录时，先说明目标路径和范围；优先备份，再做变更。
- 临时文件优先放在 `D:\T`，不要写入系统 TEMP 或仓库目录。
- 涉及本机工具链、Python、ESP-IDF、pnpm、Gradle 等环境问题时，先查 `D:\env\DEPENDENCIES.md`。
- 不得把 API Key、token、credentials、代理凭据或其他 secret 写进日志、Issue、PR、commit 或规范文档。

## 2. 分支与提交

### 分支命名

- 新功能：`feature/<short-slug>`
- 修复：`fix/<short-slug>`
- 文档或工具链：`chore/<short-slug>`

示例：`feature/settings-window`、`fix/floating-button-focus`。

### 提交规则

- 代码任务完成并通过验证后，创建本地英文 commit。
- Commit message 使用 Conventional Commits：
  - `feat: add native settings window`
  - `fix: prevent floating button from stealing focus`
  - `docs: add development workflow`
  - `chore: bump version to v1.3.0`
- 不要创建空提交；没有本任务相关变更时不提交。
- 不要把 `Cargo.lock` 的无关变化或用户本机配置纳入提交。
- 提交前运行：

```powershell
git status --porcelain
git diff --check
```

### 提交前验证

Rust 变更至少通过以下命令：

```powershell
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

如果只改 Markdown、CI 或纯文档，可以跳过 Rust 构建，但必须核对 Markdown 和受影响的 workflow YAML。涉及热键、悬浮球、文本插入、录音、设置窗口等真实 Windows 交互时，必须在真实 Windows 会话里验证并报告观测结果。

## 3. Pull Request 与自动合并

### 自动提交、推送和开 PR

验证通过后按以下顺序执行：

```powershell
git switch -c <branch-name>
git add -- <related-files>
git commit -m "<type>: <short-summary>"
git push -u origin <branch-name>
gh pr create --base main --head <branch-name> --fill
```

PR 描述至少包含：

- 动机
- 变更范围
- 已执行的本地验证
- Windows 真机验证结果（如适用）
- 已知风险或未验证项

### 审查与合并

打开 PR 后，先做一次 diff 自审，确认：

- 没有 secret、调试代码、无关格式化或意外删除。
- 配置字段有默认值，旧 `config.toml` 能继续解析。
- Windows API 调用使用 unsafe 边界清晰，错误能被记录或返回。
- UI 行为不会抢走用户正在输入窗口的焦点。

确认后等待并检查 CI：

```powershell
gh pr checks <branch-name> --watch
gh pr view <branch-name> --json state,mergeable,reviewDecision,statusCheckRollup
```

CI 全部通过、PR 可合并且无冲突时，可以自动 squash merge：

```powershell
gh pr merge <pr-number> --squash --delete-branch
```

合并后同步本地：

```powershell
git switch main
git pull --ff-only
```

禁止使用 `--force` 合并、绕过冲突、跳过失败检查或直接推 `main`。如果分支保护要求人工审查，自动流程停在等待审查状态，并明确告诉用户。

## 4. 构建与 `D:\P` 部署

### 标准构建

```powershell
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

Release 可执行文件位置：

```text
target\release\doubao-voice-input.exe
```

如需便携包，使用：

```powershell
.\scripts\build-portable.ps1 -Version <version>
```

便携包版本号的变更属于发布动作，必须先得到用户确认。

### `D:\P` 部署目标

默认部署目录固定为：

```text
D:\P\doubao-voice-input
```

目标可执行文件固定为：

```text
D:\P\doubao-voice-input\doubao-voice-input.exe
```

部署时必须保留用户本机状态：

- `D:\P\doubao-voice-input\config.toml`
- `D:\P\doubao-voice-input\credentials.json`

不要把仓库里的 `config.toml` 覆盖到 `D:\P`；只有用户明确要求重置配置时才允许。

### 自动替换并重启

部署前必须确认正在运行的进程路径属于目标 exe。若进程路径不一致，停止部署并报告差异。

参考流程：

```powershell
$deployRoot = 'D:\P\doubao-voice-input'
$targetExe = Join-Path $deployRoot 'doubao-voice-input.exe'
$buildExe = Resolve-Path 'target\release\doubao-voice-input.exe'
$backupDir = Join-Path 'D:\T' ("doubao-voice-input-backup-" + (Get-Date -Format 'yyyyMMdd-HHmmss'))

$processes = Get-Process -Name doubao-voice-input -ErrorAction SilentlyContinue
foreach ($process in $processes) {
    if ($process.Path -ne $targetExe) {
        throw "Refusing to replace: running process path is $($process.Path), not $targetExe"
    }
}

New-Item -ItemType Directory -Force -Path $backupDir | Out-Null
Copy-Item -LiteralPath $targetExe -Destination $backupDir

foreach ($process in $processes) {
    Stop-Process -Id $process.Id -Force
}
Wait-Process -Id $processes.Id -ErrorAction SilentlyContinue

Copy-Item -LiteralPath $buildExe -Destination $targetExe -Force
Start-Process -FilePath $targetExe -WorkingDirectory $deployRoot -WindowStyle Hidden
```

多进程时不要把单进程变量用于 `Wait-Process`；应遍历全部 PID。部署完成后验证：

1. 新进程存在，且 `Path` 是目标 exe。
2. `config.toml` 和 `credentials.json` 的修改时间早于或等于部署时间，未被覆盖。
3. 如已启用文件日志，读取最新日志确认启动成功。
4. 涉及 UI、热键或录音行为时，由用户在真实焦点窗口验证。

替换失败时从 `D:\T` 的备份恢复旧 exe，再次启动，并保留失败现场。

## 5. 缺陷发现、Issue 与修复

### 本地缺陷

- 编译、clippy、test 失败时优先在本分支修复。
- 不要为了让 CI 通过而删除测试、降低断言或加无意义的 allow。
- 能稳定复现的 bug 先加最小回归测试，再修实现。

### CI 失败

优先读取失败日志：

```powershell
gh run list --branch main --limit 5
gh run view <run-id> --log-failed
```

如果失败只来自噪声、网络或 runner 抖动，可以重跑一次；连续两次同样失败必须当作真实问题分析。

### Bug Issue

合并进 `main` 后仍可复现的回归，或用户明确要求登记的缺陷，创建 GitHub Issue：

```powershell
gh issue create --title "bug: <short-summary>" --body "<repro, expected, actual, commit, logs>"
```

Issue 必须包含：

- 复现步骤
- 预期行为
- 实际行为
- commit 或版本
- 最小日志片段

日志片段不得包含 token、API key、credentials、设备完整 ID 或用户敏感文本。默认标签使用 `bug`；无法稳定复现时说明证据边界。

## 6. 日志与诊断

### 当前状态

程序目前使用 `tracing_subscriber::fmt` 输出日志；CLI 模式便于控制台观察。Release UI 模式隐藏控制台后，当前实现不会稳定留下持久化文件日志。

### 开发验证

优先用 CLI 模式验证核心链路：

```powershell
cargo run -- --cli
```

需要更详细日志时：

```powershell
$env:RUST_LOG = 'doubao_voice_input=debug'
cargo run -- --cli
```

### 打包版日志目标

后续应为 Release UI 增加滚动文件日志，建议位置：

```text
D:\P\doubao-voice-input\logs\doubao-voice-input.log
```

要求：

- 按天或按大小滚动。
- 默认保留 7 天，避免无限增长。
- 记录启动、配置加载、录音开始/结束、ASR 连接失败、AI 后处理失败、文本插入失败。
- 不记录请求头、Authorization、完整 token、credentials 或用户语音原文。

日志未落地前，不得声称“读取了打包版运行日志”；只能报告进程、退出码、CI 日志或 CLI 控制台输出。

### 读取日志

文件日志可用后使用：

```powershell
Get-Content -LiteralPath 'D:\P\doubao-voice-input\logs\doubao-voice-input.log' -Tail 200
```

跟踪新输出：

```powershell
Get-Content -LiteralPath 'D:\P\doubao-voice-input\logs\doubao-voice-input.log' -Wait -Tail 50
```

诊断顺序：

1. 先看最新启动标记。
2. 再看配置加载和凭据初始化。
3. 再看 ASR/AI 网络错误。
4. 最后看 Win32 输入或 UI 错误。
5. 不确定时保存完整日志到 `D:\T`，再让用户确认是否继续。

## 7. 自动发布边界

以下动作永远不会自动执行，必须先取得用户明确确认：

- 修改 `Cargo.toml` 中的 `version`。
- 创建或推送 `v*` tag。
- 创建 GitHub Release。
- 修改 Release 附件或发布公告。
- 删除 Release、tag、分支或用户数据。

CI 中 `v*` tag 会触发 Release。不要为了“部署最新版”而顺手打 tag；本地 `D:\P` 部署应从合并后的 `main` 重新构建。

## 8. 完成定义

一个任务只有满足以下条件才算完成：

1. 相关文件已通过格式、clippy、测试和 release 构建。
2. 已创建英文本地 commit；无关用户改动未被纳入。
3. 已推送 feature branch 并开 PR 到 `main`。
4. CI 通过并完成 squash merge；本地 `main` 已同步。
5. 已从合并后的 `main` 重新构建。
6. 已按规则备份并替换 `D:\P\doubao-voice-input\doubao-voice-input.exe`。
7. 新进程已启动，且配置与凭据未被覆盖。
8. 已报告实际观测结果；未验证项和失败原因明确列出。

