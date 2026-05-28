# InsightDB AI 与本地知识库设计规范

## 一、定位

AI 知识库不是替代诊断引擎的聊天素材库，而是规则诊断和 AI 解释之间的增强层。

InsightDB 的 AI 能力不以 Text-to-SQL 或数据库聊天为主。市场上已有大量工具覆盖自然语言生成 SQL，InsightDB 的差异点是让 AI 基于执行计划、元数据、规则命中和本地知识库解释慢 SQL，并输出可验证建议。

优先级：

1. 数据库事实：执行计划、DDL、索引、统计信息、版本信息。
2. 确定性规则：本地 rules engine 命中的 findings。
3. 本地知识库：版本坑点、云厂商限制、历史经验。
4. AI 推断：基于上述上下文生成解释和建议。

AI 输出必须明确标注哪些内容来自事实证据，哪些内容是模型推断。

## 二、知识条目 Schema

知识库条目必须结构化，便于规则引擎和 Prompt Builder 精确使用。

```yaml
id: "MYSQL-ORDER-001"
title: "MySQL filesort 导致排序开销过高"
tags: ["mysql", "sort", "filesort", "index"]
scope:
  db_type: "mysql"
  version_range: ">= 8.0.0"
conditions:
  plan_flags: ["filesort"]
  sql_keywords: ["ORDER BY"]
  involved_objects: []
evidence_template: |
  执行计划显示查询触发 filesort，说明排序无法完全利用索引顺序。
recommendation_template: |
  检查 ORDER BY 字段与过滤条件是否能组成联合索引，并验证优化后执行计划是否消除 filesort。
risk: "新增索引会增加写入成本和存储占用。"
verification: |
  对比优化前后的 EXPLAIN、扫描行数和实际执行耗时。
references:
  - "https://dev.mysql.com/doc/"
priority: 50
confidence: 0.8
```

字段要求：

- `id` 必须稳定，不能因文案修改变化。
- `conditions` 必须能被程序判断，避免只写自然语言。
- `evidence_template` 必须引用可验证事实。
- `recommendation_template` 不得包含破坏性自动执行语句。
- `references` 应优先使用官方文档或可信技术文档。

## 三、检索流程

知识库检索不直接基于用户自然语言，而基于结构化诊断上下文：

```text
SQL
  -> SQL parser / lightweight extraction
  -> Catalog metadata
  -> Explain Plan
  -> Rules Findings
  -> Knowledge Retrieval
  -> Advisor Report
  -> AI Explanation
```

检索方式：

- 精确匹配：数据库类型、版本范围、计划节点、规则 ID。
- 关键词匹配：SQL keyword、plan flag、index/table metadata。
- 上下文匹配：云厂商、参数配置、表规模、索引数量。
- 语义向量匹配作为未来扩展，不进入 MVP 主路径。

性能目标：

- MVP 内置知识库检索应在 100ms 内完成。
- 知识库增大后使用索引或嵌入式搜索引擎。
- 检索失败不能阻塞规则诊断。

## 四、脱敏策略

发送给 AI 前必须脱敏：

- 数据库名、schema 名、表名、列名可按策略保留或映射为别名。
- 字符串、数字、日期字面量默认替换。
- 样例数据默认不发送。
- 凭据、主机、IP、用户名永不发送。
- 用户必须能预览最终 Prompt 上下文。

示例：

```text
orders.user_phone = '13800000000'
```

应转换为：

```text
table_1.column_3 = '<string_literal>'
```

## 五、Prompt 结构

Prompt Builder 必须使用结构化输入，不允许从 UI 文本拼接。

```markdown
# Role
你是 InsightDB 的数据库性能诊断助手。你只能基于提供的上下文进行分析。

# Rules
- 区分事实证据和推断。
- 不生成 DROP、TRUNCATE、批量 DELETE/UPDATE 等危险操作。
- 不声称已经验证未被提供的数据。
- 如果证据不足，明确说明缺失信息。

# System Context
- DB Type: {{db_type}}
- Version: {{version}}
- Engine/Extension: {{engine}}

# Sanitized SQL
{{sanitized_sql}}

# Catalog Summary
{{catalog_summary}}

# Explain Summary
{{explain_summary}}

# Rule Findings
{{rule_findings}}

# Local Knowledge
{{matched_knowledge}}

# Output Format
请输出：
1. 问题摘要
2. 证据
3. 优化建议
4. 风险
5. 验证方式
6. 置信度
```

## 六、AI 输出约束

AI 结果不能直接作为最终事实：

- UI 必须显示“规则诊断”和“AI 解释”的来源区别。
- AI 建议不能自动执行。
- AI 输出如果与规则结果冲突，应提示用户存在冲突。
- 当本地知识库与模型通用建议冲突时，优先展示本地知识库，并说明原因。
- 模型调用失败、超时或限流时，诊断报告仍应可用。

## 七、冷启动知识库

MVP 内置知识库应少而精，优先覆盖高频、可验证问题：

- MySQL 全表扫描。
- MySQL filesort / temporary table。
- MySQL 联合索引顺序问题。
- MySQL 低选择性索引。
- PostgreSQL Seq Scan 与统计信息过期。
- PostgreSQL Nested Loop 放大。
- PostgreSQL missing index。
- PostgreSQL work_mem 相关排序/哈希风险。
- 云数据库参数和权限受限提示。

不建议一开始堆 50 条以上泛化规则。早期规则应追求高准确率和可解释性。
