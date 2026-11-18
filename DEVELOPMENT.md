# 开发规范与自动化工作流

本文是本仓库的默认开发约定，也是后续 AI 助手在本项目中执行“提交—推送—PR—审查合并—构建—部署—排查”的标准流程。

## 1. 总则

- 用户说“继续/继续修复/帮我修/自动处理/按规范来”时，默认授权完成当前任务的本地实现、验证、提交、推送、开 PR、CI 自审与可合并时的合并。助手应自主选择方案并继续推进，只在遇到明确阻断或需要用户做真机操作时停下。
- 只做能直接支持当前用户目标的变更；不为“顺手改进”扩大范围，也不重排无关代码。纯文档、诊断或本地小任务不会被自动升级为代码交付或部署。
- 保留工作区中用户已有的无关改动。提交前必须核对 `git status --porcelain` 和 `git diff --name-only`，只 stage 与本任务相关的文件。
- 不直接推 `main`。用户已授权的代码变更走 feature branch → Pull Request → CI → merge；文档任务也只是推送分支并开 PR，不替代用户做需要确认的发布动作。
- 版本号变更、tag、GitHub Release、下载入口或对外发布公告必须先取得用户明确确认；修复 bug 不等于授权公开发布。
- 固定目标路径的部署属于用户已授权的“重新构建/替换 D 盘 P 目录/重启”流程，不必每次重复询问。其他删除、覆盖、批量文件操作或非本仓库目录，先说明目标路径和范围；优先备份，再做变更。
- 临时文件优先放在 `D:\T`，不要写入系统 TEMP 或仓库目录。
- 涉及本机工具链、Python、ESP-IDF、pnpm、Gradle 等环境问题时，先查 `D:\env\DEPENDENCIES.md`。
- 不得把 API Key、token、credentials、代理凭据或其他 secret 写进日志、Issue、PR、commit 或规范文档。

## 2. 分支与提交

### 分支命名

- 新功能：`feature/<short-slug>`
- 修复：`fix/<short-slug>`
- 文档：`docs/<short-slug>`
- 工具链与杂务：`chore/<short-slug>`

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

验证通过后按以下顺序执行；已有本地 commit 时跳过 `git add`/`git commit`，已有远端分支时跳过 `git push`：

```powershell
git switch -c <branch-name>
git add -- <related-files>
git commit -m "<type>: <short-summary>"
git push -u fork <branch-name>
gh pr create --repo EvanDbg/doubao-ime-win --base main --head SsuJojo:<branch-name> --title "<type>: <short-summary>" --body-file <pr-body-file>
```

`gh pr create` 没有独立的 `--from-fork` 参数；跨 fork 提 PR 时必须同时使用 `--repo EvanDbg/doubao-ime-win` 和 `--head SsuJojo:<branch-name>`。习惯单命令创建时可用 `--fill` 作为起点，但必须随即用 `gh pr edit` 补齐下列章节，不能把 `--fill` 当作完整描述：

PR 描述固定使用以下模板；没有内容时写“无”，不要留空章节：

```markdown
## 动机

## 变更范围

## 验证

- 本地验证：
- Windows 真机验证：

## 风险与未验证项
```

涉及 hotkey、悬浮球、文本插入、录音或设置窗口时，“Windows 真机验证”必须写实际观测结果；若助手无法操作 GUI，则写具体未验证项并请用户代验。

### 审查与合并

打开 PR 后，先做一次 diff 自审，确认：

- 没有 secret、调试代码、无关格式化或意外删除。
- 配置字段有默认值，旧 `config.toml` 能继续解析。
- Windows API 调用使用 unsafe 边界清晰，错误能被记录或返回。
- UI 行为不会抢走用户正在输入窗口的焦点。

确认后优先等待并检查目标仓库上的 CI（PR head SHA 必须等于本地待合并 commit）。若本仓库没有 workflow 或 PR checks 一直不产生状态，助手必须记录“无可用 CI”，不能把缺少检查误报为“CI 全绿”：

```powershell
gh pr checks <pr-number> --repo EvanDbg/doubao-ime-win --watch --fail-fast
gh pr view <pr-number> --repo EvanDbg/doubao-ime-win --json state,mergeable,mergeStateStatus,reviewDecision,statusCheckRollup,headRefOid
```

自动合并的前置条件全部满足时才可合并：PR 可合并、无冲突、无要求人工审查的 review gate、所有必需检查通过（若没有检查，则本地验证必须全部通过并在 PR 中说明“无可用 CI”）、head SHA 与本地一致。满足后自动 squash merge：

```powershell
gh pr merge <pr-number> --repo EvanDbg/doubao-ime-win --squash --delete-branch
```

合并后同步本地；若 PR head 与当前分支不是同一 commit，先保存或提交本任务改动，再切换：

```powershell
git switch main
git pull --ff-only
```

禁止使用 `--force` 合并、绕过冲突、跳过失败检查、自批准自己的 PR 或直接推 `main`。如果分支保护要求人工审查，自动流程停在等待审查状态，并明确告诉用户。

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

- 编译、clippy、test 失败时优先在当前任务分支修复，修复后重新执行失败的验证。
- 不要为了让 CI 通过而删除测试、降低断言或加无意义的 allow。
- 能稳定复现且现有测试框架能覆盖的 bug，先加最小回归测试，再修实现；纯文档缺陷直接修正文档并检查内部一致性。

### CI 失败

优先读取失败日志：

```powershell
gh run list --branch main --limit 5
gh run view <run-id> --log-failed
```

如果失败只来自噪声、网络或 runner 抖动，可以重跑一次；连续两次同样失败必须当作真实问题分析。没有 CI 可用时，以本地验证结果为准，并明确说明未运行远端检查。

### Bug Issue

合并进 `main` 后仍可复现的回归，或用户明确要求“登记 Issue/发行 bug/记录问题”的缺陷，创建 GitHub Issue；日志中出现的普通错误不必自动开 Issue：

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
5. 不确定时把相关日志片段保存到 `D:\T`，先自行继续分析；只有需要用户操作 GUI、提供凭据或做发布决定时才停下。

## 7. 自动发布边界

以下动作永远不会自动执行，必须先取得用户明确确认（即使用户说“全部自动”“一直继续”也一样）：

- 修改 `Cargo.toml` 中的 `version`。
- 创建或推送 `v*` tag。
- 创建 GitHub Release。
- 修改 Release 附件或发布公告。
- 删除 Release、tag、分支或用户数据。

CI 中 `v*` tag 会触发 Release。不要为了“部署最新版”而顺手打 tag；本地 `D:\P` 部署应从合并后的 `main` 重新构建。

## 8. 完成定义

助手必须先识别任务类型，再套用对应的完成定义。用户说“继续/按规范来/帮我修”时，由助手根据变更内容自行选择类型，不向用户追问类型。

### A. 纯文档、诊断或纯本地任务

1. 文档内部规则一致；Markdown、链接和受影响的 workflow YAML 已核对。
2. 涉及 Rust 代码的任务已通过 `cargo fmt -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test` 和 `cargo build --release`；纯文档任务无需执行 Rust 构建。
3. 已创建英文本地 commit；无关用户改动未被纳入。
4. 已推送分支并开 PR 到 `main`；或用户明确只要本地改动时保留本地 commit。
5. 已报告实际观测结果；未验证项和失败原因明确列出。
6. 不自动替换 `D:\P` 的可执行文件，不修改版本号、不打 tag、不发 Release。

### B. 默认代码任务（未明确说“只本地试试”）

1. 满足 A 中适用的验证、commit、PR 和报告要求。
2. PR 通过第 3 节的自审与合并前置条件后自动 squash merge；本地 `main` 已同步。
3. 已从合并后的 `main` 重新构建。
4. 按第 4 节备份并替换 `D:\P\doubao-voice-input\doubao-voice-input.exe`。
5. 新进程已启动，配置与凭据未被覆盖；或替换失败时已从备份恢复并说明失败原因。
6. 版本号、tag、Release 和公告仍需用户单独确认，不属于代码任务完成条件。

### C. 用户明确只要本地验证、诊断或试运行的任务

1. 完成请求的构建/运行/诊断并报告观测结果。
2. 未经用户要求不创建 commit、不推送、不开 PR、不合并、不部署、不发 Release。
