# InsightDB 项目设计与开发计划

## 一、项目定位

InsightDB 的长期愿景是成为一个本地优先、证据驱动、AI 辅助的数据库性能诊断与可观测性工作台。它不是另一个通用数据库客户端，也不是早期就对标 Datadog/DBdoctor 的完整监控平台。

近期产品切入点必须收敛：

> 面向 MySQL/PostgreSQL 的慢 SQL 诊断工作台：自动采集 SQL、执行计划、表结构、索引和统计信息，通过确定性规则引擎给出证据链，再由 AI 负责解释、排序和生成可验证建议。

长期愿景保留：

- 桌面端：Tauri + React，提供本地优先、低延迟、隐私可控的诊断体验。
- Web 端：在核心库稳定后，通过 HTTP/WebSocket 适配为团队协作版。
- Agent/eBPF：作为高级监控能力预留，不进入早期关键路径。
- AI DBA：AI 不替代规则引擎，只基于真实证据生成解释和建议。

## 二、竞争力目标

InsightDB 的竞争力不能来自泛化对标或技术栈堆叠，而应来自可验证能力：

- 每条诊断建议必须包含证据、风险、收益预估和验证方式。
- AI 输出必须可追溯到采集到的元数据、执行计划或规则命中。
- 默认只读、脱敏、本地优先，避免把用户数据库结构和数据无控制地发送给模型。
- 先把慢 SQL 诊断做到可信，再扩展到团队协作和内核监控。

对标方向应聚焦具体能力：

- TablePlus：连接体验和基础查询速度。
- DataGrip：SQL/执行计划严谨性。
- pganalyze / Percona Toolkit：诊断证据链。
- Cursor：上下文组织和解释体验。

## 三、市场与开源竞争格局

开源数据库管理工具市场已经非常拥挤。InsightDB 不应以“通用数据库 GUI”作为主战场。

现有开源或开放生态工具大致分为四类：

- 通用数据库客户端：DBeaver、DbGate、Beekeeper Studio、Outerbase Studio。
- AI SQL 客户端：Chat2DB、SQLChat、DBChat、各类 Text-to-SQL 工具。
- 专项数据库工具：pgAdmin、phpMyAdmin、DB Browser for SQLite。
- 性能诊断/运维工具：Percona Toolkit、pganalyze、云厂商诊断控制台。

这些工具的优势：

- DBeaver 覆盖数据库类型广，功能完整，社区成熟。
- DbGate 和 Beekeeper Studio 更轻量，现代 UI 和日常编辑体验较好。
- Outerbase Studio 更偏浏览器化、数据编辑和团队数据体验。
- Chat2DB / SQLChat / DBChat 已经覆盖“自然语言生成 SQL”和“数据库聊天”心智。

InsightDB 不适合正面竞争的方向：

- 大而全数据库管理。
- 完整 CRUD 数据编辑器。
- AI 生成 SQL 聊天工具。
- 多数据库数量竞赛。
- 通用 BI、Dashboard 或 Notebook 平台。

InsightDB 应聚焦的差异化：

- 第一屏是“诊断一个 SQL”，不是表浏览器。
- 核心对象是 `DiagnosisReport`，不是 query result。
- 诊断结果必须有证据链：执行计划、DDL、索引、统计信息、规则命中。
- AI 是 DBA 解释器，不是自由聊天框。
- 默认本地优先、只读、脱敏，适合生产库问题排查。
- 开源版优先把 MySQL/PostgreSQL 慢 SQL 诊断做到高可信，而不是堆功能面。

竞争力判断：

- 作为通用数据库 GUI：竞争力弱。
- 作为现代轻量 SQL 客户端：竞争力中低。
- 作为 AI 生成 SQL 工具：竞争力中低。
- 作为本地优先、AI 辅助、证据驱动的慢 SQL 诊断工具：具备中高竞争力。

因此，InsightDB 的产品战略应是“诊断型替代/补充工具”，而不是 DBeaver、Beekeeper Studio 或 DbGate 的同质化替代品。

## 四、核心架构

采用 `Core Rust Library + Bridge Adapter + Frontend` 的分层架构。所有未来迁移能力依赖于核心库与宿主环境解耦。

```text
[Frontend: React + TypeScript + Tailwind]
  ├── SQL Workspace
  ├── Explain Viewer
  ├── Diagnosis Report
  ├── AI Assistant Drawer
  └── API Adapter
       ├── Tauri IPC Adapter
       └── Future HTTP/WebSocket Adapter

[Bridge Layer]
  ├── Desktop: Tauri Commands / Events
  └── Future Web: Axum API / WebSocket / Auth Gateway

[Core Rust Crates]
  ├── insightdb_connector   // MySQL/PostgreSQL 连接、权限检测、查询取消
  ├── insightdb_catalog     // 表、列、索引、统计信息、版本信息采集
  ├── insightdb_explain     // EXPLAIN 解析和统一执行计划模型
  ├── insightdb_rules       // 确定性诊断规则
  ├── insightdb_advisor     // 建议生成、风险评级、证据链
  ├── insightdb_ai          // 脱敏、上下文压缩、流式模型调用
  └── insightdb_storage     // 本地历史、诊断报告、连接配置

[Future Extensions]
  ├── insightdb_server      // Web 团队协作版
  └── insightdb_agent       // 远程 Agent / eBPF 探针
```

关键原则：

- 前端不得直接依赖 Tauri API，所有业务调用必须走 API Adapter。
- Rust 核心库不得依赖 Tauri、Web 框架或 UI 类型。
- 诊断数据模型必须稳定，桌面端、Web 端、CLI 和 Agent 共享同一套核心结构。
- AI 模块只能消费脱敏后的上下文，不拥有数据库连接和执行权限。

## 五、MVP 功能范围

MVP 只做慢 SQL 诊断闭环，不做完整数据库客户端。

必须包含：

- MySQL 8.0 / PostgreSQL 15+ 连接。
- SQL 输入、执行、取消查询。
- 表结构、索引、行数、版本信息采集。
- MySQL `EXPLAIN FORMAT=JSON` 和 PostgreSQL `EXPLAIN (FORMAT JSON)` 解析。
- 统一执行计划模型。
- 规则引擎诊断：全表扫描、索引缺失、低选择性索引、排序/临时表、Nested Loop 风险、扫描行数异常。
- 诊断报告：证据、严重级别、建议、风险、验证 SQL。
- AI 解释：基于诊断报告生成自然语言说明，不自动执行任何变更。
- 本地历史报告。

暂不包含：

- eBPF。
- Web 团队版。
- 完整 CRUD 管理器。
- 多数据库大而全支持。
- 自动执行索引变更。

## 六、安全与隐私

安全能力是早期核心能力，不是后期补丁：

- 默认只读连接，写操作需要显式开启。
- 危险 SQL 拦截：`DROP`、`TRUNCATE`、无条件 `DELETE/UPDATE`、批量 DDL。
- 凭据使用系统安全存储，不明文落盘。
- AI 上下文必须脱敏：库名、表名、列名、字面量、样例数据按策略替换。
- 发送给模型的内容需要在 UI 中可预览。
- AI 建议只能生成草稿或复制文本，不允许直接执行。
- 所有诊断报告保存数据来源和生成时间，便于复盘。

## 七、开发路线

### Phase 0: 核心模型与连接层

目标：建立与 UI 无关的 Rust 核心。

验收标准：

- `insightdb_core` 可被 CLI 测试调用。
- MySQL/PostgreSQL 能执行 `SELECT 1`。
- 连接失败、权限不足、网络中断返回统一错误。
- 支持查询取消。
- 不把完整结果集一次性加载进内存。

### Phase 1: 元数据与执行计划

目标：形成诊断所需的事实基础。

验收标准：

- 采集数据库版本、表结构、索引、估算行数。
- 解析 MySQL/PostgreSQL JSON 执行计划。
- 转换为统一 `PlanNode`。
- 输出计划节点的扫描方式、估算行数、过滤条件、排序/临时表等属性。

### Phase 2: 规则引擎与诊断报告

目标：先用确定性逻辑建立可信建议。

验收标准：

- 规则命中输出 `finding_id`、严重级别、证据、建议、风险。
- 每条建议都有验证方式。
- 规则结果不依赖 AI。
- 报告可序列化保存到本地。

### Phase 3: AI 解释层

目标：让 AI 增强可读性，而不是替代判断。

验收标准：

- Prompt 只基于脱敏后的诊断上下文生成。
- 支持流式输出。
- AI 输出明确区分“证据事实”和“模型推断”。
- 模型失败时，规则诊断仍可完整工作。

### Phase 4: 桌面体验完善

目标：形成可日常使用的桌面工作台。

验收标准：

- SQL Workspace、执行计划视图、诊断报告、历史报告可用。
- 大结果集分页/虚拟滚动。
- 查询和诊断过程可取消。
- UI 不因长查询或 AI 流式输出阻塞。

### Phase 5: Web 与团队版预备

目标：验证核心库复用，不急于商业化扩张。

验收标准：

- 用 Axum 包一层 API，不修改核心诊断逻辑。
- 补齐鉴权、租户隔离、审计日志、连接密钥托管设计。
- 明确哪些能力只能在桌面本地运行，哪些能进入团队版。

### Phase 6: Agent/eBPF 可行性验证

目标：只在用户场景明确时验证高级监控。

验收标准：

- Agent 是独立二进制，不嵌入桌面核心路径。
- eBPF 仅作为可选数据源接入统一指标模型。
- 明确内核版本、权限、云数据库不可用场景。

## 八、测试策略

测试重点从“视觉性能口号”调整为关键风险：

- 连接层：成功、失败、超时、取消、权限不足。
- 元数据采集：不同版本 MySQL/PostgreSQL 的字段差异。
- 执行计划解析：真实 JSON 样本回归测试。
- 规则引擎：固定输入生成稳定 finding。
- 脱敏：确保 Prompt 不包含原始敏感标识和字面量。
- AI：Mock 流式响应、超时、限流、模型错误。
- UI：长查询不阻塞、诊断可取消、大结果集不掉帧。

## 九、下一步

建议先完成 Phase 0 到 Phase 2，再进入 AI 和 UI 体验打磨。只有当规则诊断本身可信时，AI 才能成为产品优势，而不是不可控的包装层。
