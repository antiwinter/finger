use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use finger_core::types::{BotEntry, Command, OrchestratorState};
use finger_core::settings::Settings;

pub struct App {
    pub state: Arc<Mutex<Vec<BotEntry>>>,
    pub orch_state: Arc<Mutex<OrchestratorState>>,
    pub selected: usize,
    pub log_visible: bool,
    pub log_messages: Vec<String>,
    pub log_scroll: usize, // scroll offset from bottom (0 = latest)
    pub log_rx: mpsc::Receiver<String>,
    pub cmd_tx: mpsc::Sender<Command>,
    pub settings_path: PathBuf,
    pub should_quit: bool,
}

impl App {
    pub fn new(
        state: Arc<Mutex<Vec<BotEntry>>>,
        orch_state: Arc<Mutex<OrchestratorState>>,
        log_rx: mpsc::Receiver<String>,
        cmd_tx: mpsc::Sender<Command>,
        settings_path: PathBuf,
    ) -> Self {
        Self {
            state,
            orch_state,
            selected: 0,
            log_visible: true,
            log_messages: Vec::new(),
            log_scroll: 0,
            log_rx,
            cmd_tx,
            settings_path,
            should_quit: false,
        }
    }

    pub fn drain_logs(&mut self) {
        let mut new_msgs = false;
        while let Ok(msg) = self.log_rx.try_recv() {
            self.log_messages.push(msg);
            new_msgs = true;
        }
        // Auto-scroll to bottom if user was already at bottom
        if new_msgs && self.log_scroll == 0 {
            // stay at bottom
        }
    }

    pub fn scroll_log_up(&mut self, n: usize) {
        self.log_scroll = self.log_scroll.saturating_add(n);
    }

    pub fn scroll_log_down(&mut self, n: usize) {
        self.log_scroll = self.log_scroll.saturating_sub(n);
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
        {
            let mut entries = self.state.lock().unwrap();
            if let Some(entry) = entries.get_mut(self.selected) {
                entry.enabled = !entry.enabled;
            }
            let enabled_bots: Vec<String> = entries.iter()
                .filter(|e| e.enabled)
                .map(|e| e.name.clone())
                .collect();
            Settings { enabled_bots }.save(&self.settings_path);
        }
        self.cmd_tx.send(Command::Toggle(self.selected)).ok();
    }

    pub fn start_stop(&mut self) {
        {
            let mut os = self.orch_state.lock().unwrap();
            match *os {
                OrchestratorState::Running => *os = OrchestratorState::Stopping,
                OrchestratorState::Stopped => *os = OrchestratorState::Running,
                OrchestratorState::Stopping => return,
            }
        }
        self.cmd_tx.send(Command::StartStop).ok();
    }

    pub fn restart_selected(&mut self) {
        self.cmd_tx.send(Command::Restart(self.selected)).ok();
    }

    pub fn toggle_log(&mut self) {
        self.log_visible = !self.log_visible;
    }

    pub fn quit(&mut self) {
        self.cmd_tx.send(Command::Quit).ok();
        self.should_quit = true;
    }
}
