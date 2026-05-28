# InsightDB 核心架构与开发规范

## 一、架构原则

InsightDB 的架构目标是让近期桌面 MVP 和未来 Web/Agent 演进共享同一套诊断核心，避免被 Tauri、HTTP API 或某个模型供应商绑定。

核心原则：

- 核心诊断逻辑只存在于 Rust core crates。
- Bridge 层只负责协议转换，不承载业务判断。
- Frontend 只负责交互、展示和本地 UI 状态。
- AI 是解释层和建议生成辅助，不是唯一诊断来源。
- Agent/eBPF 是可选数据源，不影响桌面 MVP 主路径。

## 二、Frontend

前端定位是慢 SQL 诊断工作台，不是通用数据库管理器，也不是数据处理引擎。界面优先服务“输入/选择 SQL -> 采集上下文 -> 生成诊断报告 -> 验证建议”这条主路径。

推荐技术：

- React + TypeScript。
- Tailwind CSS + shadcn/ui。
- Zustand 管理轻量 UI 状态。
- `@tanstack/react-virtual` 或同类方案处理大列表。
- `cmdk` 提供命令面板。

约束：

- 第一屏应突出诊断入口，表浏览和数据编辑不能成为主导航中心。
- 不在前端持有完整大结果集。
- 不直接调用 Tauri `invoke`，必须走 `api` adapter。
- 不在前端拼接诊断结论，只渲染 core 返回的结构化报告。
- 查询、诊断、AI 输出都必须可取消。
- UI 文案必须区分事实、建议、风险和模型推断。
- UI 不应把 AI 聊天作为核心产品形态；AI 入口必须绑定具体 SQL、执行计划或诊断报告。

## 三、Bridge Layer

Bridge 层包括 Tauri IPC、未来 HTTP/WebSocket API、CLI 入口。

约束：

- Bridge 不直接访问数据库驱动。
- Bridge 不解析执行计划。
- Bridge 不生成诊断规则。
- Bridge 只负责鉴权、参数校验、序列化、事件流和错误映射。

IPC/HTTP 响应应保持小而稳定：

- 大结果集分页或流式传输。
- 单次 IPC payload 不应超过约 5MB。
- 长任务使用事件流：进度、日志、结果片段、取消状态。
- 错误统一为 `{ code, message, suggestion, retryable, source }`。

## 四、Core Backend

核心库建议按职责拆分：

```text
insightdb_connector
insightdb_catalog
insightdb_explain
insightdb_rules
insightdb_advisor
insightdb_ai
insightdb_storage
```

### Connector

职责：

- 管理 MySQL/PostgreSQL 连接。
- 执行 SQL。
- 支持取消查询。
- 检测权限和版本。
- 标准化数据库错误。

约束：

- 默认只读模式。
- 不允许无界读取。
- 查询结果必须分页、流式或游标化。

### Catalog

职责：

- 采集 schema、table、column、index、constraint、statistics。
- 采集数据库版本和关键配置。
- 输出稳定的元数据模型。

约束：

- 不依赖数据库特定字段泄露到上层。
- 采集失败时返回部分结果和 warning，不应导致整次诊断失败。

### Explain

职责：

- 解析 MySQL/PostgreSQL JSON 执行计划。
- 转换为统一 `PlanNode`。
- 保留原始计划用于审计和回归测试。

`PlanNode` 至少包含：

- 节点类型。
- 访问方法。
- 表名。
- 估算行数。
- 实际行数，若可用。
- 成本，若可用。
- 过滤条件。
- 排序、临时表、回表、并行等标记。

### Rules

职责：

- 基于执行计划和元数据输出确定性 findings。
- 不调用 AI。

每个 finding 必须包含：

- `id`
- `severity`
- `title`
- `evidence`
- `recommendation`
- `risk`
- `verification`
- `confidence`

### Advisor

职责：

- 聚合规则结果。
- 去重和排序建议。
- 生成最终诊断报告。
- 明确哪些内容是事实，哪些内容是推断。

### AI

职责：

- 脱敏。
- 上下文压缩。
- Prompt 构建。
- 调用模型并流式返回解释。

约束：

- 不保存原始凭据。
- 不执行 SQL。
- 不直接产生可自动执行的变更。
- 模型失败不能影响规则诊断可用性。

## 五、安全规范

必须作为核心架构的一部分实现：

- 凭据使用系统安全存储。
- 默认只读连接。
- 危险 SQL 拦截。
- AI 上下文脱敏。
- 用户可预览即将发送给模型的上下文。
- 诊断报告记录数据来源、采集时间和模型版本。
- Web 版必须补齐 RBAC、租户隔离、审计日志和连接密钥托管。

危险 SQL 至少包含：

- `DROP`
- `TRUNCATE`
- 无 `WHERE` 的 `DELETE`
- 无 `WHERE` 的 `UPDATE`
- 批量 DDL
- 显式权限变更语句

## 六、未来演进约束

### Web 版

Web 版不能只是把 Tauri API 换成 HTTP。必须额外设计：

- 用户和组织模型。
- RBAC。
- 审计日志。
- 数据库连接代理。
- 密钥托管。
- 任务队列。
- 多租户隔离。

### Agent/eBPF

Agent/eBPF 不进入 MVP。

未来接入时必须满足：

- Agent 是独立二进制。
- eBPF 只是指标数据源。
- 数据进入统一 metrics/capture model。
- 桌面核心诊断不依赖内核探针。
- 明确 Linux 内核、权限和云数据库限制。

## 七、代码规范

Rust：

- 模块、函数、变量使用 `snake_case`。
- 类型和 trait 使用 `CamelCase`。
- 公共核心类型必须有 `///` 文档。
- 数据模型优先使用 `serde` 可序列化结构。
- 错误使用统一枚举或错误类型，不泄露驱动内部错误到 UI。

TypeScript：

- 组件使用 `PascalCase.tsx`。
- API adapter 和工具函数使用 `camelCase.ts`。
- UI 状态与服务数据分离。
- 不在组件内散落 API endpoint、IPC command、错误码。

通用：

- 禁止魔法字符串，SQL 视图名、错误码、规则 ID、API route 必须常量化。
- 复杂规则必须带官方文档或样本来源注释。
- 任何模型或第三方 API 的版本、参数、限制在实现前必须查证。
