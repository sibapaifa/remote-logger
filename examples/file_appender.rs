use remote_logger::{FileAppender, RemoteLogger};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    println!("Running File appender example");
    let handle = RemoteLogger::new()
        .with_appender(
            FileAppender::new("output.log")
                .unwrap()
                .with_target("LOCAL_FILE")
                .with_module_level((module_path!().to_owned(), log::LevelFilter::Info))
                .with_max_level(log::LevelFilter::Info),
        )
        .init()
        .unwrap();

    for i in 0..10 {
        log::info!(target: "CONSOLE", "CONSOLE: Line no: {}", i);
        log::info!(target: "LOCAL_FILE", "LOCAL_FILE: Line no: {}", i);
        log::info!("WITHOUT_TARGET: Line no: {}", i);
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    handle.close_all().await.unwrap();
}
