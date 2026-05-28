use url::Url;
use crate::error::ConnectorError;

/// 数据库类型
#[derive(Debug, Clone, PartialEq)]
pub enum DatabaseKind {
    MySQL,
    PostgreSQL,
}

/// 连接配置，可从数据库 URL 解析
#[derive(Clone)]
pub struct ConnectorConfig {
    pub kind: DatabaseKind,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub database: String,
    /// 额外参数，如 `ssl_mode`、`application_name` 等
    pub extra_params: Vec<(String, String)>,
}

impl std::fmt::Debug for ConnectorConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectorConfig")
            .field("kind", &self.kind)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("user", &self.user)
            .field("password", &"********")
            .field("database", &self.database)
            .field("extra_params", &self.extra_params)
            .finish()
    }
}

impl ConnectorConfig {
    /// 从形如 `mysql://user:pass@host:3306/db` 或 `postgres://user:pass@host:5432/db` 的 URL 解析配置。
    pub fn from_url(url_str: &str) -> Result<Self, ConnectorError> {
        let url = Url::parse(url_str)
            .map_err(|e| ConnectorError::invalid_config(
                format!("无法解析连接 URL: {e}"),
                Some("请使用标准格式：mysql://user:pass@host:port/db 或 postgres://...".to_string()),
            ))?;

        let kind = match url.scheme() {
            "mysql" => DatabaseKind::MySQL,
            "postgres" | "postgresql" => DatabaseKind::PostgreSQL,
            other => return Err(ConnectorError::invalid_config(
                format!("不支持的数据库协议: {other}，仅支持 mysql/postgresql"),
                Some("请使用 mysql:// 或 postgres:// 开头".to_string()),
            )),
        };

        let host = url.host_str()
            .ok_or_else(|| ConnectorError::invalid_config(
                "URL 缺少主机名".to_string(),
                Some("示例：mysql://user:pass@localhost:3306/mydb".to_string()),
            ))?
            .to_string();

        let port = url.port().unwrap_or(match kind {
            DatabaseKind::MySQL => 3306,
            DatabaseKind::PostgreSQL => 5432,
        });

        let user = url.username().to_string();
        let password = url.password().unwrap_or("").to_string();

        // 数据库名称：路径的第一个段
        let database = url.path().trim_start_matches('/').to_string();
        if database.is_empty() {
            return Err(ConnectorError::invalid_config(
                "URL 缺少数据库名称".to_string(),
                Some("请在路径中指定数据库名称，如 mysql://user@host/mydb".to_string()),
            ));
        }

        // 解析额外参数（?sslmode=require 等）
        let extra_params: Vec<(String, String)> = url.query_pairs()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        Ok(ConnectorConfig {
            kind,
            host,
            port,
            user,
            password,
            database,
            extra_params,
        })
    }

    /// 构造 sqlx 所需的连接字符串（不通过 URL 解析，直接拼写）
    pub fn to_connection_string(&self) -> String {
        // sqlx 支持 mysql://... 和 postgres://... 格式
        let scheme = match self.kind {
            DatabaseKind::MySQL => "mysql",
            DatabaseKind::PostgreSQL => "postgres",
        };
        let mut s = format!(
            "{}://{}:{}@{}:{}/{}",
            scheme,
            percent_encoding::utf8_percent_encode(&self.user, percent_encoding::NON_ALPHANUMERIC),
            percent_encoding::utf8_percent_encode(&self.password, percent_encoding::NON_ALPHANUMERIC),
            self.host,
            self.port,
            self.database,
        );
        // 附加参数
        if !self.extra_params.is_empty() {
            let params: Vec<String> = self.extra_params.iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect();
            s.push('?');
            s.push_str(&params.join("&"));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mysql_url() {
        let cfg = ConnectorConfig::from_url("mysql://root:secret@localhost:3306/testdb").unwrap();
        assert_eq!(cfg.kind, DatabaseKind::MySQL);
        assert_eq!(cfg.host, "localhost");
        assert_eq!(cfg.port, 3306);
        assert_eq!(cfg.user, "root");
        assert_eq!(cfg.password, "secret");
        assert_eq!(cfg.database, "testdb");
    }

    #[test]
    fn parse_postgres_url() {
        let cfg = ConnectorConfig::from_url("postgres://admin:pass@pg-host:5432/mydb").unwrap();
        assert_eq!(cfg.kind, DatabaseKind::PostgreSQL);
        assert_eq!(cfg.host, "pg-host");
        assert_eq!(cfg.port, 5432);
        assert_eq!(cfg.user, "admin");
        assert_eq!(cfg.password, "pass");
        assert_eq!(cfg.database, "mydb");
    }

    #[test]
    fn parse_with_extra_params() {
        let cfg = ConnectorConfig::from_url("mysql://user@host/db?sslmode=required").unwrap();
        assert_eq!(cfg.extra_params.len(), 1);
        assert_eq!(cfg.extra_params[0], ("sslmode".to_string(), "required".to_string()));
    }

    #[test]
    fn invalid_scheme() {
        let err = ConnectorConfig::from_url("sqlite:///tmp/db").unwrap_err();
        assert!(err.to_string().contains("不支持的数据库协议"));
    }
}
