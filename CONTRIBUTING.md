# Contributing to Doubao Voice Input

Thanks for your interest in contributing! Issues and pull requests are welcome.

感谢你对本项目的关注，欢迎提交 Issue 和 Pull Request。

## Reporting issues / 提交 Issue

- Please search existing issues first to avoid duplicates.
- Include your Windows version, the app version (or commit), steps to reproduce, and any relevant log output.
- 提交前请先搜索已有 Issue，避免重复；请附上 Windows 版本、程序版本、复现步骤和相关日志。

## Development setup / 开发环境

- Rust 1.70+ (stable)
- Windows 10/11 x64
- Visual Studio Build Tools 2022
- CMake and Protobuf Compiler (`protoc`)

```powershell
git clone https://github.com/EvanDbg/doubao-ime-win.git
cd doubao-ime-win
cargo build --release
```

## Pull requests / 提交 PR

- Keep each PR focused on a single change; describe the motivation and what you tested.
- Make sure `cargo build --release` succeeds before submitting.
- For hotkey or input-behaviour changes, please describe how you verified the behaviour on a real Windows machine.
- 每个 PR 只做一件事，说明动机和测试方式；提交前确保能成功构建。涉及热键/输入行为的改动，请说明在真实 Windows 机器上的验证情况。

## License / 许可证

By contributing, you agree that your contributions will be licensed under the MIT License (see [LICENSE](LICENSE)).

提交贡献即表示你同意你的贡献以 MIT License 发布（见 [LICENSE](LICENSE)）。
