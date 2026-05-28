# InsightDB 连接器测试环境说明

## 依赖

- Rust 1.75+
- Docker（必须运行）
- 测试期间需要从 Docker Hub 拉取 MySQL 8.0 和 PostgreSQL 15 镜像（自动完成）

## 运行集成测试

```bash
cd insightdb_connector
cargo test --test integration_tests
```

测试会：

1. 启动 MySQL 容器（默认镜像 `mysql:8`）
2. 启动 PostgreSQL 容器（默认镜像 `postgres:15`）
3. 执行连接、查询、取消等测试
4. 测试结束后自动销毁容器

## 配置

无需额外配置。如果需要指定不同的镜像版本，可以修改 `testcontainers-modules` 的调用参数（参见 `testcontainers-modules` 文档）。

## 注意

- 第一次运行会下载镜像，耗时较长。
- 如果 Docker 未运行，测试会报错，提示无法连接。
- 部分系统需要将当前用户加入 `docker` 组，或以 sudo 运行。
