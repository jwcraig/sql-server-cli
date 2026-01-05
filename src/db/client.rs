use std::time::Duration;

use anyhow::Result;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_util::compat::TokioAsyncWriteCompatExt;

use crate::config::ConnectionSettings;
use crate::db::connection::build_config;
use crate::error::{AppError, ErrorKind};

/// Connect to SQL Server using resolved settings.
///
/// # Arguments
///
/// * `settings` - Resolved connection settings (host, auth, TLS flags, timeout).
///
/// # Returns
///
/// A connected Tiberius client wrapped for Tokio compatibility.
///
/// # Errors
///
/// Returns an error if DNS resolution, TCP connect, or login fails, or if the
/// configured timeout elapses during connect or login.
pub async fn connect(
    settings: &ConnectionSettings,
) -> Result<tiberius::Client<tokio_util::compat::Compat<TcpStream>>> {
    let config =
        build_config(settings).map_err(|err| AppError::new(ErrorKind::Config, err.to_string()))?;
    let timeout_duration = if settings.timeout_ms == 0 {
        None
    } else {
        Some(Duration::from_millis(settings.timeout_ms))
    };

    // `timeout` wraps the future in an outer Result for Elapsed vs IO errors.
    let tcp = if let Some(duration) = timeout_duration {
        timeout(duration, TcpStream::connect(config.get_addr()))
            .await
            .map_err(|_| {
                AppError::new(
                    ErrorKind::Connection,
                    format!("Connection timed out after {} ms", settings.timeout_ms),
                )
            })?
            .map_err(|err| AppError::new(ErrorKind::Connection, err.to_string()))?
    } else {
        TcpStream::connect(config.get_addr())
            .await
            .map_err(|err| AppError::new(ErrorKind::Connection, err.to_string()))?
    };
    tcp.set_nodelay(true)?;
    let client = if let Some(duration) = timeout_duration {
        timeout(
            duration,
            tiberius::Client::connect(config, tcp.compat_write()),
        )
        .await
        .map_err(|_| {
            AppError::new(
                ErrorKind::Connection,
                format!("Login timed out after {} ms", settings.timeout_ms),
            )
        })?
        .map_err(|err| AppError::new(ErrorKind::Connection, err.to_string()))?
    } else {
        tiberius::Client::connect(config, tcp.compat_write())
            .await
            .map_err(|err| AppError::new(ErrorKind::Connection, err.to_string()))?
    };
    Ok(client)
}
