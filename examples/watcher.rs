use futures_lite::StreamExt;
use mirajazz::{
    device::{DeviceQuery, DeviceWatcher},
    error::MirajazzError,
};

const QUERY: DeviceQuery = DeviceQuery::new(65440, 2, 0x0300, 0x1003);

#[tokio::main]
async fn main() -> Result<(), MirajazzError> {
    let mut watcher_struct = DeviceWatcher::new();
    let mut watcher = watcher_struct.watch(&[QUERY]).await?;

    loop {
        if let Some(ev) = watcher.next().await {
            println!("New device event: {:?}", ev);
        }
    }
}
