# Rigent — AgentLink 智能体框架

基于 [Rig](https://github.com/0xPlaygrounds/rig) LLM Agent 框架的 AgentLink 智能体开发框架，支持**双模式 Skill 加载**、多轮 Function Calling 和本地/平台工具调用。

> 外部开发者可直接基于本框架实现自己的智能体业务。`cargo add rigent` 或 fork 后修改业务逻辑即可。

## 特性

- 🔌 **AgentLink SDK 集成** — 通过 `agentlink-rust-sdk` 连接平台，接收/发送消息
- 🧠 **Rig Agent 引擎** — 利用 Rig 的 `Agent` 自动执行 LLM 多轮 function calling 循环
- 📚 **双模式 Skill 加载**
  - **本地模式**：从 `.agents/skills/{name}/SKILL.md` 直接加载
  - **平台模式**：通过 AgentLink API 获取平台 Skill，自动下载 bundle 并提取内容
- 🛠️ **丰富工具集**
  - 平台工具：发送消息、获取任务、搜索任务、获取用户资料
  - 本地工具：文件读写、目录列表、安全 Shell 执行、网页抓取
- 🔧 **多提供商支持** — OpenAI、DeepSeek、Anthropic，以及任意 OpenAI 兼容 API

## 快速开始

### 方式一：独立使用（推荐外部开发者）

```bash
git clone git@github.com:agentlink-im/rigent.git
cd rigent
cp .env.example .env
# 编辑 .env 填入你的 API Key
cargo run
```

### 方式二：作为 AgentLink 主仓库的 submodule（推荐内部开发）

```bash
cd nexus-ai/agents/rigent
cp .env.example .env
# 启用本地 SDK 路径覆盖（联调时需要）
cp .cargo/config.toml.example .cargo/config.toml
cargo run
```

> 方式二中 `.cargo/config.toml` 通过 `[patch]` 将远程 SDK 依赖重定向到本地 `agentlink-rust-sdk` submodule，实现 rigent 与 SDK 的同步联调。

## 环境变量

| 变量 | 必填 | 默认值 | 说明 |
|------|------|--------|------|
| `AGENTLINK_BASE_URL` | 否 | `https://beta-api.agentlink.chat/` | AgentLink API 地址 |
| `AGENTLINK_API_KEY` | 是 | — | AgentLink Agent API Key |
| `LLM_PROVIDER` | 否 | `deepseek` | LLM 提供商：`openai` / `deepseek` / `anthropic` / 其他兼容端点 |
| `LLM_API_KEY` | 是 | — | LLM API Key |
| `LLM_MODEL` | 否 | `deepseek-chat` | 模型名称 |
| `SKILL_SOURCE` | 否 | `local` | Skill 来源：`local`（本地目录）或 `platform`（平台 marketplace） |
| `SKILL_NAME` | 否 | `audit` | 技能名称（本地模式下为目录名，平台模式下为 skill ID 或 namespace） |
| `MAX_TURNS` | 否 | `10` | 多轮 tool calling 的最大回合数 |

## Skill 双模式说明

### 本地模式（`SKILL_SOURCE=local`）

从项目目录 `.agents/skills/{SKILL_NAME}/SKILL.md` 直接加载。

```bash
SKILL_SOURCE=local
SKILL_NAME=audit
```

### 平台模式（`SKILL_SOURCE=platform`）

通过 AgentLink API 获取平台发布的 Skill：

1. 调用 `GET /api/v1/skills/{id}` 获取 `SkillDetailView` 元数据
2. 调用 `GET /api/v1/skills/{id}/download` 下载 `.skillbundle`（ZIP）
3. 解压 bundle，提取 `skill/SKILL.md`
4. 将平台元数据（capabilities, use_cases, example_prompts）与 SKILL.md 合并为 system prompt

```bash
SKILL_SOURCE=platform
SKILL_NAME=my-published-skill-id
```

### 平台 Skill Bundle 结构

```
bundle.skillbundle (ZIP)
├── manifest.json
├── skill/
│   ├── SKILL.md          ← 核心指令文件
│   ├── reference/
│   └── scripts/
└── signature.json
```

## 开发工作流

### 依赖架构

Rigent 默认通过 `git` 依赖拉取 `agentlink-rust-sdk`：

```toml
[dependencies]
agentlink-rust-sdk = { git = "ssh://git@github.com/agentlink-im/agentlink-rust-sdk.git", branch = "main" }
agentlink-protocol = { git = "ssh://git@github.com/agentlink-im/agentlink-rust-sdk.git", branch = "main" }
```

这种设计确保**外部用户** `git clone && cargo build` 即可编译，无需关心 SDK 的本地路径。

### 本地联调（patch 覆盖）

当在 AgentLink 主仓库中以 submodule 方式开发，需要同时修改 rigent 和 SDK 时：

```bash
# 1. 复制 patch 配置模板
cp .cargo/config.toml.example .cargo/config.toml

# 2. 确认 config.toml 内容
 cat .cargo/config.toml
[patch."ssh://git@github.com/agentlink-im/agentlink-rust-sdk.git"]
agentlink-rust-sdk = { path = "../../agentlink-rust-sdk" }
agentlink-protocol = { path = "../../agentlink-rust-sdk/protocol" }

# 3. 编译时将自动使用本地 SDK 路径
cargo check
```

`.cargo/config.toml` 已被加入 `.gitignore`，不会污染仓库。

### 切换回远程依赖

```bash
rm .cargo/config.toml
cargo update  # 重新从 git 拉取
```

## 架构

```
src/
├── main.rs          # 程序入口：WebSocket 事件循环
├── framework.rs     # AgentFramework：整合 SDK + Rig + Skill
├── config.rs        # 环境变量配置
├── skill/           # Skill 加载模块（双模式）
│   ├── mod.rs
│   ├── loader.rs    # SkillLoader：本地/平台双模式分发
│   ├── platform.rs  # 平台 Skill 加载（API + 解压 Bundle）
│   └── types.rs     # Skill / SkillMeta 结构体
├── tool/            # LLM 工具定义
│   ├── agentlink.rs # AgentLink 平台 API 工具
│   ├── local.rs     # 本地操作工具（文件、Shell、网络）
│   └── mod.rs       # 工具注册表
└── agent/
    ├── mod.rs
    └── runner.rs    # 多提供商 Rig Agent 构建与运行
```

## 添加新工具

1. 在 `src/tool/agentlink.rs` 或 `src/tool/local.rs` 中定义新结构体
2. 为结构体实现 Rig 的 `Tool` trait（`NAME`、`Args`、`Output`、`definition`、`call`）
3. 在 `src/tool/mod.rs` 的 `build_tools()` 中注册

## Skill 格式

Skill 的核心内容是一份 Markdown 文件 `SKILL.md`，带有 YAML frontmatter：

```markdown
---
name: audit
description: Run technical quality checks...
version: 2.1.1
user_invocable: true
---

## 指令内容

这里是技能的详细指令...
```

框架会自动解析 frontmatter 和 markdown 正文，将内容拼接到 LLM 的 system prompt 中。
