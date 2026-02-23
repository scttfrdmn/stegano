//! Structured logging for production monitoring and debugging

use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Structured logger for production environments
#[derive(Debug)]
pub struct StructuredLogger {
    config: LoggingConfig,
    writers: Vec<Arc<Mutex<dyn LogWriter + Send>>>,
}

/// Configuration for structured logging
#[derive(Debug, Clone)]
pub struct LoggingConfig {
    pub level: LogLevel,
    pub format: LogFormat,
    pub include_timestamp: bool,
    pub include_level: bool,
    pub include_target: bool,
    pub include_thread_id: bool,
    pub max_field_length: usize,
}

/// Log severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
}

/// Log output formats
#[derive(Debug, Clone, Copy)]
pub enum LogFormat {
    Json,
    Structured,
    Compact,
}

/// Log entry with structured fields
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: SystemTime,
    pub level: LogLevel,
    pub target: String,
    pub message: String,
    pub fields: HashMap<String, Value>,
    pub thread_id: String,
}

/// Trait for log output writers
pub trait LogWriter: std::fmt::Debug {
    fn write_log(&mut self, entry: &LogEntry, format: LogFormat) -> io::Result<()>;
    fn flush(&mut self) -> io::Result<()>;
}

/// Console log writer
#[derive(Debug)]
pub struct ConsoleWriter {
    use_stderr: bool,
}

/// File log writer
#[derive(Debug)]
pub struct FileWriter {
    file: std::fs::File,
    _path: std::path::PathBuf,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: LogLevel::Info,
            format: LogFormat::Structured,
            include_timestamp: true,
            include_level: true,
            include_target: true,
            include_thread_id: false,
            max_field_length: 1024,
        }
    }
}

impl StructuredLogger {
    /// Create a new structured logger with default configuration
    pub fn new() -> crate::core::Result<Self> {
        Self::with_config(LoggingConfig::default())
    }

    /// Create a structured logger with custom configuration
    pub fn with_config(config: LoggingConfig) -> crate::core::Result<Self> {
        let console_writer = Arc::new(Mutex::new(ConsoleWriter::new(false)));

        Ok(Self {
            config,
            writers: vec![console_writer],
        })
    }

    /// Add a file writer
    pub fn add_file_writer<P: AsRef<std::path::Path>>(
        &mut self,
        path: P,
    ) -> crate::core::Result<()> {
        let file_writer = Arc::new(Mutex::new(FileWriter::new(path)?));
        self.writers.push(file_writer);
        Ok(())
    }

    /// Initialize the logger as the global logger
    pub fn initialize(&self) -> crate::core::Result<()> {
        // In a real implementation, this would set up the global logger
        // For now, we'll just log that initialization occurred
        self.info(
            "Structured logger initialized",
            &[
                ("level", &self.config.level.as_str()),
                ("format", &self.config.format.as_str()),
            ],
        );

        Ok(())
    }

    /// Log at trace level
    pub fn trace(&self, message: &str, fields: &[(&str, &dyn std::fmt::Display)]) {
        if self.config.level <= LogLevel::Trace {
            self.log(LogLevel::Trace, message, fields);
        }
    }

    /// Log at debug level
    pub fn debug(&self, message: &str, fields: &[(&str, &dyn std::fmt::Display)]) {
        if self.config.level <= LogLevel::Debug {
            self.log(LogLevel::Debug, message, fields);
        }
    }

    /// Log at info level
    pub fn info(&self, message: &str, fields: &[(&str, &dyn std::fmt::Display)]) {
        if self.config.level <= LogLevel::Info {
            self.log(LogLevel::Info, message, fields);
        }
    }

    /// Log at warn level
    pub fn warn(&self, message: &str, fields: &[(&str, &dyn std::fmt::Display)]) {
        if self.config.level <= LogLevel::Warn {
            self.log(LogLevel::Warn, message, fields);
        }
    }

    /// Log at error level
    pub fn error(&self, message: &str, fields: &[(&str, &dyn std::fmt::Display)]) {
        if self.config.level <= LogLevel::Error {
            self.log(LogLevel::Error, message, fields);
        }
    }

    /// Log an operation
    pub fn log_operation(&self, operation: &str, duration: Duration, success: bool) {
        let level = if success {
            LogLevel::Info
        } else {
            LogLevel::Warn
        };
        let status = if success { "success" } else { "failure" };

        self.log(
            level,
            "Operation completed",
            &[
                ("operation", &operation),
                ("duration_ms", &duration.as_millis()),
                ("status", &status),
                ("success", &success),
            ],
        );
    }

    /// Log a security event
    pub fn log_security_event(&self, event_type: &str, details: &[(&str, &dyn std::fmt::Display)]) {
        let mut fields = vec![
            ("event_type", &event_type as &dyn std::fmt::Display),
            ("category", &"security" as &dyn std::fmt::Display),
        ];
        fields.extend_from_slice(details);

        self.log(LogLevel::Warn, "Security event", &fields);
    }

    /// Log a performance metric
    pub fn log_performance(&self, metric: &str, value: f64, unit: &str) {
        self.log(
            LogLevel::Info,
            "Performance metric",
            &[
                ("metric", &metric),
                ("value", &value),
                ("unit", &unit),
                ("category", &"performance"),
            ],
        );
    }

    /// Core logging function
    fn log(&self, level: LogLevel, message: &str, fields: &[(&str, &dyn std::fmt::Display)]) {
        let entry = LogEntry {
            timestamp: SystemTime::now(),
            level,
            target: "stegano".to_string(),
            message: message.to_string(),
            fields: self.convert_fields(fields),
            thread_id: format!("{:?}", std::thread::current().id()),
        };

        for writer in &self.writers {
            if let Ok(mut writer) = writer.lock() {
                if let Err(e) = writer.write_log(&entry, self.config.format) {
                    eprintln!("Failed to write log entry: {e}");
                }
            }
        }
    }

    /// Convert display fields to JSON values
    fn convert_fields(&self, fields: &[(&str, &dyn std::fmt::Display)]) -> HashMap<String, Value> {
        let mut result = HashMap::new();

        for (key, value) in fields {
            let value_str = format!("{value}");
            let truncated = if value_str.len() > self.config.max_field_length {
                format!("{}...", &value_str[..self.config.max_field_length])
            } else {
                value_str
            };

            result.insert(key.to_string(), Value::String(truncated));
        }

        result
    }

    /// Flush all writers
    pub fn flush(&self) -> crate::core::Result<()> {
        for writer in &self.writers {
            if let Ok(mut writer) = writer.lock() {
                writer.flush().map_err(crate::core::SteganoError::IoError)?;
            }
        }
        Ok(())
    }
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "trace" => Some(Self::Trace),
            "debug" => Some(Self::Debug),
            "info" => Some(Self::Info),
            "warn" => Some(Self::Warn),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

impl LogFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Structured => "structured",
            Self::Compact => "compact",
        }
    }
}

impl ConsoleWriter {
    pub fn new(use_stderr: bool) -> Self {
        Self { use_stderr }
    }
}

impl LogWriter for ConsoleWriter {
    fn write_log(&mut self, entry: &LogEntry, format: LogFormat) -> io::Result<()> {
        let output = match format {
            LogFormat::Json => self.format_json(entry)?,
            LogFormat::Structured => self.format_structured(entry),
            LogFormat::Compact => self.format_compact(entry),
        };

        if self.use_stderr {
            writeln!(io::stderr(), "{output}")?;
        } else {
            writeln!(io::stdout(), "{output}")?;
        }

        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.use_stderr {
            io::stderr().flush()
        } else {
            io::stdout().flush()
        }
    }
}

impl ConsoleWriter {
    fn format_json(&self, entry: &LogEntry) -> io::Result<String> {
        let timestamp = entry
            .timestamp
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();

        let mut json_entry = json!({
            "timestamp": timestamp,
            "level": entry.level.as_str(),
            "target": entry.target,
            "message": entry.message,
            "thread": entry.thread_id,
        });

        if let Value::Object(ref mut obj) = json_entry {
            for (key, value) in &entry.fields {
                obj.insert(key.clone(), value.clone());
            }
        }

        serde_json::to_string(&json_entry)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    fn format_structured(&self, entry: &LogEntry) -> String {
        let timestamp = entry
            .timestamp
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();

        let mut parts = vec![
            format!("time={}", timestamp),
            format!("level={}", entry.level.as_str()),
            format!("target={}", entry.target),
            format!("msg=\"{}\"", entry.message),
        ];

        for (key, value) in &entry.fields {
            if let Value::String(s) = value {
                parts.push(format!("{key}=\"{s}\""));
            } else {
                parts.push(format!("{key}={value}"));
            }
        }

        parts.join(" ")
    }

    fn format_compact(&self, entry: &LogEntry) -> String {
        let level_char = match entry.level {
            LogLevel::Trace => "T",
            LogLevel::Debug => "D",
            LogLevel::Info => "I",
            LogLevel::Warn => "W",
            LogLevel::Error => "E",
        };

        format!("{} {}", level_char, entry.message)
    }
}

impl FileWriter {
    pub fn new<P: AsRef<std::path::Path>>(path: P) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;

        Ok(Self { file, _path: path })
    }
}

impl LogWriter for FileWriter {
    fn write_log(&mut self, entry: &LogEntry, format: LogFormat) -> io::Result<()> {
        let console_writer = ConsoleWriter::new(false);
        let output = match format {
            LogFormat::Json => console_writer.format_json(entry)?,
            LogFormat::Structured => console_writer.format_structured(entry),
            LogFormat::Compact => console_writer.format_compact(entry),
        };

        writeln!(self.file, "{output}")?;
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[derive(Debug)]
    struct TestWriter {
        buffer: Cursor<Vec<u8>>,
    }

    impl TestWriter {
        fn new() -> Self {
            Self {
                buffer: Cursor::new(Vec::new()),
            }
        }

        fn get_output(&self) -> String {
            String::from_utf8_lossy(self.buffer.get_ref()).to_string()
        }
    }

    impl LogWriter for TestWriter {
        fn write_log(&mut self, entry: &LogEntry, format: LogFormat) -> io::Result<()> {
            let console_writer = ConsoleWriter::new(false);
            let output = match format {
                LogFormat::Json => console_writer.format_json(entry)?,
                LogFormat::Structured => console_writer.format_structured(entry),
                LogFormat::Compact => console_writer.format_compact(entry),
            };

            writeln!(self.buffer, "{output}")?;
            Ok(())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_log_level_ordering() {
        assert!(LogLevel::Trace < LogLevel::Debug);
        assert!(LogLevel::Debug < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Warn);
        assert!(LogLevel::Warn < LogLevel::Error);
    }

    #[test]
    fn test_log_level_string_conversion() {
        assert_eq!(LogLevel::Info.as_str(), "info");
        assert_eq!(LogLevel::from_str("warn"), Some(LogLevel::Warn));
        assert_eq!(LogLevel::from_str("invalid"), None);
    }

    #[test]
    fn test_structured_logger_creation() {
        let logger = StructuredLogger::new();
        assert!(logger.is_ok());

        let config = LoggingConfig {
            level: LogLevel::Debug,
            format: LogFormat::Json,
            include_timestamp: false,
            include_level: true,
            include_target: false,
            include_thread_id: true,
            max_field_length: 512,
        };

        let logger = StructuredLogger::with_config(config);
        assert!(logger.is_ok());
    }

    #[test]
    fn test_logging_levels() {
        let mut logger = StructuredLogger::with_config(LoggingConfig {
            level: LogLevel::Warn,
            ..LoggingConfig::default()
        })
        .unwrap();

        // Replace writer with test writer
        let test_writer = Arc::new(Mutex::new(TestWriter::new()));
        logger.writers = vec![test_writer.clone()];

        // These should not be logged (below threshold)
        logger.trace("trace message", &[]);
        logger.debug("debug message", &[]);
        logger.info("info message", &[]);

        // These should be logged
        logger.warn("warn message", &[]);
        logger.error("error message", &[]);

        let output = test_writer.lock().unwrap().get_output();
        assert!(output.contains("warn message"));
        assert!(output.contains("error message"));
        assert!(!output.contains("trace message"));
        assert!(!output.contains("debug message"));
        assert!(!output.contains("info message"));
    }

    #[test]
    fn test_structured_fields() {
        let mut logger = StructuredLogger::new().unwrap();
        let test_writer = Arc::new(Mutex::new(TestWriter::new()));
        logger.writers = vec![test_writer.clone()];

        logger.info(
            "test message",
            &[
                ("operation", &"embed"),
                ("duration", &100),
                ("success", &true),
            ],
        );

        let output = test_writer.lock().unwrap().get_output();
        assert!(output.contains("test message"));
        assert!(output.contains("operation"));
        assert!(output.contains("embed"));
        assert!(output.contains("duration"));
        assert!(output.contains("100"));
    }

    #[test]
    fn test_operation_logging() {
        let mut logger = StructuredLogger::new().unwrap();
        let test_writer = Arc::new(Mutex::new(TestWriter::new()));
        logger.writers = vec![test_writer.clone()];

        logger.log_operation("embed", Duration::from_millis(150), true);
        logger.log_operation("extract", Duration::from_millis(75), false);

        let output = test_writer.lock().unwrap().get_output();
        assert!(output.contains("Operation completed"));
        assert!(output.contains("embed"));
        assert!(output.contains("extract"));
        assert!(output.contains("success"));
        assert!(output.contains("failure"));
    }

    #[test]
    fn test_security_event_logging() {
        let mut logger = StructuredLogger::new().unwrap();
        let test_writer = Arc::new(Mutex::new(TestWriter::new()));
        logger.writers = vec![test_writer.clone()];

        logger.log_security_event(
            "unauthorized_access",
            &[("user", &"test_user"), ("ip", &"192.168.1.1")],
        );

        let output = test_writer.lock().unwrap().get_output();
        assert!(output.contains("Security event"));
        assert!(output.contains("unauthorized_access"));
        assert!(output.contains("security"));
        assert!(output.contains("test_user"));
    }

    #[test]
    fn test_performance_logging() {
        let mut logger = StructuredLogger::new().unwrap();
        let test_writer = Arc::new(Mutex::new(TestWriter::new()));
        logger.writers = vec![test_writer.clone()];

        logger.log_performance("throughput", 1024.5, "MB/s");

        let output = test_writer.lock().unwrap().get_output();
        assert!(output.contains("Performance metric"));
        assert!(output.contains("throughput"));
        assert!(output.contains("1024.5"));
        assert!(output.contains("MB/s"));
    }

    #[test]
    fn test_json_format() {
        let config = LoggingConfig {
            format: LogFormat::Json,
            ..LoggingConfig::default()
        };
        let mut logger = StructuredLogger::with_config(config).unwrap();
        let test_writer = Arc::new(Mutex::new(TestWriter::new()));
        logger.writers = vec![test_writer.clone()];

        logger.info("json test", &[("key", &"value")]);

        let output = test_writer.lock().unwrap().get_output();
        assert!(output.contains("\"message\":\"json test\""));
        assert!(output.contains("\"key\":\"value\""));
        assert!(output.contains("\"level\":\"info\""));
    }

    #[test]
    fn test_compact_format() {
        let config = LoggingConfig {
            format: LogFormat::Compact,
            ..LoggingConfig::default()
        };
        let mut logger = StructuredLogger::with_config(config).unwrap();
        let test_writer = Arc::new(Mutex::new(TestWriter::new()));
        logger.writers = vec![test_writer.clone()];

        logger.info("compact test", &[]);
        logger.error("error test", &[]);

        let output = test_writer.lock().unwrap().get_output();
        assert!(output.contains("I compact test"));
        assert!(output.contains("E error test"));
    }

    #[test]
    fn test_field_truncation() {
        let config = LoggingConfig {
            max_field_length: 10,
            ..LoggingConfig::default()
        };
        let mut logger = StructuredLogger::with_config(config).unwrap();
        let test_writer = Arc::new(Mutex::new(TestWriter::new()));
        logger.writers = vec![test_writer.clone()];

        let long_value = "this is a very long string that should be truncated";
        logger.info("truncation test", &[("long_field", &long_value)]);

        let output = test_writer.lock().unwrap().get_output();
        assert!(output.contains("this is a "));
        assert!(output.contains("..."));
        assert!(!output.contains("very long string"));
    }
}
