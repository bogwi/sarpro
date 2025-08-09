use std::fmt;
use std::sync::{Arc, Mutex};
use tracing::{Event, Subscriber, field::Visit};
use tracing_subscriber::layer::{Context, Layer};

#[derive(Clone)]
pub struct LogEntry {
    pub level: tracing::Level,
    pub timestamp: String,
    pub message: String,
    pub target: String,
}

impl LogEntry {
    pub fn new(level: tracing::Level, message: String, target: String) -> Self {
        let timestamp = chrono::Utc::now().format("%H:%M:%S").to_string();
        Self {
            level,
            timestamp,
            message,
            target,
        }
    }
}

static LOG_BUFFER: once_cell::sync::Lazy<Arc<Mutex<Vec<LogEntry>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(Vec::new())));

pub fn get_log_buffer() -> Arc<Mutex<Vec<LogEntry>>> {
    LOG_BUFFER.clone()
}

pub struct GuiLogLayer;

impl GuiLogLayer {
    pub fn new() -> Self {
        Self
    }
}

struct MessageVisitor {
    message: String,
}

impl MessageVisitor {
    fn new() -> Self {
        Self {
            message: String::new(),
        }
    }
}

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        }
    }
}

impl<S> Layer<S> for GuiLogLayer
where
    S: Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        // Get event metadata
        let metadata = event.metadata();
        let level = metadata.level();

        // Extract the message using a visitor
        let mut visitor = MessageVisitor::new();
        event.record(&mut visitor);

        // Create log entry
        let message = if !visitor.message.is_empty() {
            visitor.message
        } else {
            metadata.target().to_string()
        };

        let log_entry = LogEntry::new(level.clone(), message, metadata.target().to_string());

        // Write to global buffer
        if let Ok(mut buf) = LOG_BUFFER.lock() {
            buf.push(log_entry);
            if buf.len() > 1000 {
                buf.remove(0);
            }
        }
    }
}
