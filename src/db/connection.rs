use anyhow::{anyhow, Result};

use crate::config::ConnectionSettings;

pub fn build_config(settings: &ConnectionSettings) -> Result<tiberius::Config> {
    let mut config = tiberius::Config::new();
    config.host(&settings.server);
    config.port(settings.port);
    config.database(&settings.database);

    match (&settings.user, &settings.password) {
        (Some(user), Some(pass)) => {
            config.authentication(tiberius::AuthMethod::sql_server(user, pass));
        }
        (Some(user), None) => {
            return Err(anyhow!(
                "Password is required for SQL authentication (user: {})",
                user
            ));
        }
        _ => {}
    }

    if settings.encrypt {
        config.encryption(tiberius::EncryptionLevel::Required);
    } else {
        config.encryption(tiberius::EncryptionLevel::NotSupported);
    }

    if settings.trust_cert {
        config.trust_cert();
    }

    Ok(config)
}

pub fn build_ado_string(settings: &ConnectionSettings) -> String {
    let mut parts = vec![
        format!("Server={},{}", settings.server, settings.port),
        format!("Database={}", settings.database),
    ];

    if let Some(user) = &settings.user {
        parts.push(format!("User ID={}", user));
    }
    if let Some(password) = &settings.password {
        parts.push(format!("Password={}", password));
    }
    if settings.encrypt {
        parts.push("Encrypt=true".to_string());
    } else {
        parts.push("Encrypt=false".to_string());
    }
    if settings.trust_cert {
        parts.push("TrustServerCertificate=true".to_string());
    }
    parts.push(format!("Connection Timeout={}", settings.timeout_ms / 1000));
    parts.join(";")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_ado_string() {
        let mut settings = ConnectionSettings::default();
        settings.server = "db.example".to_string();
        settings.port = 1444;
        settings.database = "main".to_string();
        settings.user = Some("sa".to_string());
        settings.password = Some("secret".to_string());

        let ado = build_ado_string(&settings);
        assert!(ado.contains("Server=db.example,1444"));
        assert!(ado.contains("Database=main"));
        assert!(ado.contains("User ID=sa"));
        assert!(ado.contains("Password=secret"));
        assert!(ado.contains("Encrypt=true"));
        assert!(ado.contains("TrustServerCertificate=true"));
    }
}
