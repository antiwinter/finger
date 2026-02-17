use crate::types::*;
use crate::logger;
use super::{Platform, WindowHandle};

pub struct StubPlatform;

impl Platform for StubPlatform {
    fn get_instances(&self, pattern: &str) -> Vec<(WindowId, String)> {
        logger::info_p("stub", &format!("get_instances(\"{}\")", pattern));
        let pat = pattern.to_lowercase();
        if pat.contains("warcraft") || pat.contains("wow") {
            vec![
                (10001, "World of Warcraft".into()),
                (10002, "World of Warcraft".into()),
            ]
        } else if pat.contains("僵尸") || pat.contains("zombie") {
            vec![(20001, "向僵尸开炮".into())]
        } else {
            vec![(30001, format!("Window<{}>", pattern))]
        }
    }

    fn create_window(&self, pattern: &str, window_id: WindowId) -> Box<dyn WindowHandle> {
        logger::info_p("stub", &format!("create_window(\"{}\", {})", pattern, window_id));
        Box::new(StubWindow {
            window_id,
            title: format!("Stub-{}", window_id),
            region: Region {
                l: 0, t: 0, r: 1920, b: 1080,
                w: 1920, h: 1080, cx: 960, cy: 540,
            },
        })
    }
}

struct StubWindow {
    window_id: WindowId,
    title: String,
    region: Region,
}

impl WindowHandle for StubWindow {
    fn id(&self) -> WindowId { self.window_id }
    fn title(&self) -> &str { &self.title }
    fn region(&self) -> Option<Region> { Some(self.region) }

    fn update(&mut self) {
        logger::info_p("stub", &format!("win({}).update()", self.window_id));
    }

    fn activate(&mut self) {
        logger::info_p("stub", &format!("win({}).activate()", self.window_id));
    }

    fn click_relative(&mut self, x_ratio: f64, y_ratio: f64) {
        logger::info_p("stub", &format!("win({}).click_relative({:.2}, {:.2})", self.window_id, x_ratio, y_ratio));
    }

    fn tap(&mut self, key: &str) {
        logger::info_p("stub", &format!("win({}).tap(\"{}\")", self.window_id, key));
    }

    fn type_text(&mut self, text: &str) {
        logger::info_p("stub", &format!("win({}).type_text(\"{}\")", self.window_id, text));
    }

    fn capture(&mut self, rect: Option<CaptureRect>) -> Option<Capture> {
        logger::info_p("stub", &format!("win({}).capture({:?})", self.window_id, rect));
        None
    }
}
