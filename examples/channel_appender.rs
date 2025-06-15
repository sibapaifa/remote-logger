use remote_logger::{ChannelAppender, LogMessage, RemoteLogger};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    println!("Running Channel appender example");
    let (sender, receiver) = crossbeam_channel::unbounded::<LogMessage>();
    let handle = tokio::runtime::Handle::current();
    std::thread::spawn(move || {
        handle.block_on(async move {
            while let Ok(msg) = receiver.recv() {
                println!("Received msg: {}", msg.message());
            }
        });
    });

    let handle = RemoteLogger::new()
        .with_appender(
            ChannelAppender::new(sender)
                .with_target("CHANNEL")
                .with_module_level((module_path!().to_owned(), log::LevelFilter::Info))
                .with_max_level(log::LevelFilter::Info),
        )
        .init()
        .unwrap();

    for i in 0..10 {
        log::info!(target: "CHANNEL", "Line no: {}", i);
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    handle.close_all().await.unwrap();
}
