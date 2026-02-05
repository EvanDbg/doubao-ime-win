# PRD 文档说明

本目录包含 Doubao IME Windows 项目的完整产品需求文档（Product Requirement Document）。

## 📚 文档列表

### 1. [windows-ime-requirements.md](./windows-ime-requirements.md)
**产品需求文档（v2.0 简化版）**

包含内容：
- 项目背景与目标（简化为语音输入工具）
- 功能需求详细说明
- 非功能需求（性能、兼容性、安全性）
- 技术方案（Rust + Windows API）
- 开发计划与里程碑（7 周）
- Doubao ASR 协议要点
- Windows SendInput API 说明
- 用户交互流程图
- 配置文件结构
- 测试策略
- 交付物清单

### 2. [technical-architecture.md](./technical-architecture.md)
**技术架构详细设计（v2.0 简化版）**

包含内容：
- 最终技术选型（Rust）
- 完整依赖列表
- 系统架构分层设计
- 核心模块详细设计（带代码示例）
  - 语音输入控制器（实时插入 + 退格修正）
  - 文本插入服务（SendInput + 退格）
  - 热键管理（支持双击）
  - ASR 客户端模块
  - 音频服务模块
  - 凭据存储模块
- 关键流程实现（语音输入完整流程）
- 打包与部署方案
- 性能优化策略
- 测试策略与示例

### 3. [task-list.md](./task-list.md)
**开发任务清单（简化版）**

包含内容：
- 6 个开发阶段的详细任务列表（7 周）
- 每个任务的检查项（Checkbox）
- 技术风险评估与缓解措施
- 当前开发进度跟踪

---

## 📖 阅读建议

### 对于产品经理/项目管理者
1. 先阅读 `windows-ime-requirements.md` 了解整体需求
2. 查看 `task-list.md` 了解开发计划和进度

### 对于开发人员
1. 阅读 `windows-ime-requirements.md` 理解业务需求
2. 深入阅读 `technical-architecture.md` 了解技术实现细节
3. 参考 `task-list.md` 执行开发任务

### 对于测试人员
1. 阅读 `windows-ime-requirements.md` 第八节"测试策略"
2. 查看 `technical-architecture.md` 第七节"测试策略"中的代码示例

---

## 🔄 文档更新记录

| 日期 | 版本 | 更新内容 |
|------|------|---------|
| 2026-02-05 | v1.0 | 初始版本发布 |

---

## 📝 文档维护

- **负责人**: 项目组
- **更新频率**: 根据项目进展随时更新
- **审阅流程**: 所有重大变更需经过团队评审

---

## 🔗 相关链接

- [项目 README](../README.md)
- [doubaoime-asr 参考项目](https://github.com/starccy/doubaoime-asr)
- [Windows TSF 官方文档](https://learn.microsoft.com/en-us/windows/win32/tsf/)
