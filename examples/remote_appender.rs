use std::time::Duration;

use async_trait::async_trait;
use remote_logger::{FileAppender, LogMessage, RemoteAppender, RemoteLogger, SendLog};

struct BulkLogSender;

#[async_trait]
impl SendLog for BulkLogSender {
    const BATCH_SIZE: usize = 10;
    const WAIT_TIMEOUT: Duration = Duration::from_secs(5);

    async fn send_log(&self, messages: &[LogMessage]) {
        println!("sending log payload count: {}", messages.len());
        for message in messages {
            let key_values = message.key_values();
            let message = message.message();
            println!(
                "message is: {}, key_values length: {}",
                message,
                key_values.len()
            );
        }
        // Prepare the bulk upload payload and call remote api from here
        // For now we just call sleep
        tokio::time::sleep(Duration::from_secs(3)).await;
        println!("Log sending successful");
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    println!("Running Remote appender example");

    let handle = RemoteLogger::new()
        .with_appender(RemoteAppender::new(BulkLogSender))
        .with_appender(FileAppender::new("output.log").unwrap())
        .init()
        .unwrap();

    for i in 0..100 {
        log::info!("Line no: {}", i);
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    handle.close_all().await.unwrap();
}
