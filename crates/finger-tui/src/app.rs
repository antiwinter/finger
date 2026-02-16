use std::sync::{Arc, Mutex, mpsc};
use finger_core::types::{BotEntry, Command};

pub struct App {
    pub state: Arc<Mutex<Vec<BotEntry>>>,
    pub selected: usize,
    pub log_visible: bool,
    pub log_messages: Vec<String>,
    pub log_rx: mpsc::Receiver<String>,
    pub cmd_tx: mpsc::Sender<Command>,
    pub should_quit: bool,
}

impl App {
    pub fn new(
        state: Arc<Mutex<Vec<BotEntry>>>,
        log_rx: mpsc::Receiver<String>,
        cmd_tx: mpsc::Sender<Command>,
    ) -> Self {
        Self {
            state,
            selected: 0,
            log_visible: true,
            log_messages: Vec::new(),
            log_rx,
            cmd_tx,
            should_quit: false,
        }
    }

    pub fn drain_logs(&mut self) {
        while let Ok(msg) = self.log_rx.try_recv() {
            self.log_messages.push(msg);
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        let len = self.state.lock().unwrap().len();
        if self.selected + 1 < len {
            self.selected += 1;
        }
    }

    pub fn toggle_selected(&mut self) {
        self.cmd_tx.send(Command::Toggle(self.selected)).ok();
    }

    pub fn toggle_log(&mut self) {
        self.log_visible = !self.log_visible;
    }

    pub fn quit(&mut self) {
        self.cmd_tx.send(Command::Quit).ok();
        self.should_quit = true;
    }
}
