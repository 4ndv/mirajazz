use futures_lite::StreamExt;
use mirajazz::{device::DeviceWatcher, error::MirajazzError};

const VID: u16 = 0x0300;

#[tokio::main]
async fn main() -> Result<(), MirajazzError> {
    let mut watcher_struct = DeviceWatcher::new();
    let mut watcher = watcher_struct.watch(&[VID]).await?;

    loop {
        if let Some(ev) = watcher.next().await {
            println!("New device event: {:?}", ev);
        }
    }
}
