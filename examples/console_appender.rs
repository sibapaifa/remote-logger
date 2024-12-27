use remote_logger::ConsoleAppender;
use remote_logger::RemoteLogger;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    println!("Running console appender example");
    let handle = RemoteLogger::new()
        .with_appender(
            ConsoleAppender::new()
                .with_target("CONSOLE")
                .with_max_level(log::LevelFilter::Info),
        )
        .init()
        .unwrap();

    for i in 0..100 {
        log::info!(target: "CONSOLE", "CONSOLE: Line no: {}", i);
        log::info!(target: "SOMETHING_ELSE", "SOMETHING_ELSE: Line no: {}", i);
        log::info!("WITHOUT_TARGET: Line no: {}", i);
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }

    handle.close_all().await.unwrap();
}
