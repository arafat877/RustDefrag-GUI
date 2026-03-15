pub mod messages;
pub mod worker;

use std::sync::{Arc, atomic::AtomicBool, mpsc};
use messages::{EngineCommand, EngineEvent};

pub struct EngineHandle {
    pub cmd_tx:    mpsc::SyncSender<EngineCommand>,
    pub evt_rx:    mpsc::Receiver<EngineEvent>,
    pub stop_flag: Arc<AtomicBool>,
}

impl EngineHandle {
    pub fn launch() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::sync_channel::<EngineCommand>(8);
        let (evt_tx, evt_rx) = mpsc::channel::<EngineEvent>();
        let stop_flag        = Arc::new(AtomicBool::new(false));
        worker::spawn_worker(cmd_rx, evt_tx, stop_flag.clone());
        Self { cmd_tx, evt_rx, stop_flag }
    }

    pub fn send(&self, cmd: EngineCommand) {
        let _ = self.cmd_tx.try_send(cmd);
    }

    pub fn stop(&self) {
        use std::sync::atomic::Ordering;
        self.stop_flag.store(true, Ordering::SeqCst);
        let _ = self.cmd_tx.try_send(EngineCommand::Stop);
    }
}
