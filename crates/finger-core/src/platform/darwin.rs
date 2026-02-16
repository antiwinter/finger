use std::process::Command as ProcessCommand;

use core_foundation::array::CFArray;
use core_foundation::base::TCFType;
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use core_graphics::event::*;
use core_graphics::event_source::*;
use core_graphics::geometry::*;
use core_graphics::window::*;

use crate::logger;
use crate::types::*;
use super::{Platform, WindowHandle};

// AppleScript key codes for special keys
fn applescript_key_code(key: &str) -> Option<u16> {
    match key {
        "enter" | "return" => Some(36),
        "escape" | "esc" => Some(53),
        "delete" | "backspace" => Some(51),
        "tab" => Some(48),
        "space" => Some(49),
        "up" => Some(126),
        "down" => Some(125),
        "left" => Some(123),
        "right" => Some(124),
        _ => None,
    }
}

pub struct DarwinPlatform;

impl DarwinPlatform {
    pub fn new() -> Self {
        DarwinPlatform
    }
}

impl Platform for DarwinPlatform {
    fn get_instances(&self, pattern: &str) -> Vec<(WindowId, String)> {
        let mut windows = Vec::new();
        let re = match regex::Regex::new(&format!("(?i){}", pattern)) {
            Ok(r) => r,
            Err(e) => {
                logger::error(&format!("invalid pattern '{}': {}", pattern, e));
                return windows;
            }
        };

        unsafe {
            let option = kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements;
            let window_list_ref = CGWindowListCopyWindowInfo(option, kCGNullWindowID);
            if window_list_ref.is_null() {
                logger::warn("failed to get window list");
                return windows;
            }

            let list: CFArray = CFArray::wrap_under_create_rule(window_list_ref as _);
            let values = list.get_all_values();

            for dict_ptr in &values {
                let dict: CFDictionary<CFString, *const std::ffi::c_void> =
                    CFDictionary::wrap_under_get_rule(*dict_ptr as _);

                let name = get_cf_string(&dict, "kCGWindowName").unwrap_or_default();
                let owner = get_cf_string(&dict, "kCGWindowOwnerName").unwrap_or_default();
                let window_id = get_cf_number(&dict, "kCGWindowNumber");
                let layer = get_cf_number(&dict, "kCGWindowLayer");

                let title = if !name.is_empty() { &name } else { &owner };

                // Match against title, name, and owner individually (like JS version)
                let is_match = !title.is_empty()
                    && (re.is_match(title)
                        || (!name.is_empty() && re.is_match(&name))
                        || (!owner.is_empty() && re.is_match(&owner)));

                if is_match
                    && layer == Some(0)
                    && window_id.is_some()
                {
                    let wid = window_id.unwrap() as WindowId;
                    logger::info(&format!("[darwin] found window: \"{}\" (id: {})", title, wid));
                    windows.push((wid, title.to_string()));
                }
            }
        }

        windows
    }

    fn create_window(&self, pattern: &str, window_id: WindowId) -> Box<dyn WindowHandle> {
        let mut win = DarwinWindow {
            _pattern: pattern.to_string(),
            window_id: window_id as CGWindowID,
            title: String::new(),
            pid: None,
            region: None,
        };
        win.do_update();
        Box::new(win)
    }
}

struct DarwinWindow {
    _pattern: String,
    window_id: CGWindowID,
    title: String,
    pid: Option<i32>,
    region: Option<Region>,
}

impl DarwinWindow {
    fn do_update(&mut self) {
        unsafe {
            let option = kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements;
            let window_list_ref = CGWindowListCopyWindowInfo(option, kCGNullWindowID);
            if window_list_ref.is_null() {
                self.region = None;
                return;
            }

            let list: CFArray = CFArray::wrap_under_create_rule(window_list_ref as _);
            let values = list.get_all_values();

            for dict_ptr in &values {
                let dict: CFDictionary<CFString, *const std::ffi::c_void> =
                    CFDictionary::wrap_under_get_rule(*dict_ptr as _);

                let wid = get_cf_number(&dict, "kCGWindowNumber");
                if wid != Some(self.window_id as i64) {
                    continue;
                }

                // Found our window
                let name = get_cf_string(&dict, "kCGWindowName").unwrap_or_default();
                let owner = get_cf_string(&dict, "kCGWindowOwnerName").unwrap_or_default();
                self.title = if !name.is_empty() { name } else { owner };
                self.pid = get_cf_number(&dict, "kCGWindowOwnerPID").map(|v| v as i32);

                // Get bounds
                if let Some(bounds) = get_cf_dict(&dict, "kCGWindowBounds") {
                    let x = get_cf_number(&bounds, "X").unwrap_or(0) as i32;
                    let y = get_cf_number(&bounds, "Y").unwrap_or(0) as i32;
                    let w = get_cf_number(&bounds, "Width").unwrap_or(0) as i32;
                    let h = get_cf_number(&bounds, "Height").unwrap_or(0) as i32;

                    self.region = Some(Region {
                        l: x, t: y, r: x + w, b: y + h,
                        w, h, cx: x + w / 2, cy: y + h / 2,
                    });
                }

                return;
            }

            // Window not found
            self.region = None;
        }
    }
}

impl WindowHandle for DarwinWindow {
    fn id(&self) -> WindowId {
        self.window_id as WindowId
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn region(&self) -> Option<Region> {
        self.region
    }

    fn update(&mut self) {
        self.do_update();
    }

    fn activate(&mut self) {
        if self.pid.is_none() {
            self.do_update();
        }
        if let Some(pid) = self.pid {
            let script = format!(
                "tell application \"System Events\" to set frontmost of first process whose unix id is {} to true",
                pid
            );
            ProcessCommand::new("osascript")
                .arg("-e")
                .arg(&script)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .ok();
        }
    }

    fn click_relative(&mut self, x_ratio: f64, y_ratio: f64) {
        self.do_update();
        let region = match self.region {
            Some(r) => r,
            None => return,
        };
        let pid = match self.pid {
            Some(p) => p,
            None => return,
        };

        let x = region.l as f64 + x_ratio * region.w as f64;
        let y = region.t as f64 + y_ratio * region.h as f64;
        let point = CGPoint::new(x, y);

        let source = match CGEventSource::new(CGEventSourceStateID::HIDSystemState) {
            Ok(s) => s,
            Err(_) => return,
        };

        if let Ok(mouse_down) = CGEvent::new_mouse_event(
            source.clone(),
            CGEventType::LeftMouseDown,
            point,
            CGMouseButton::Left,
        ) {
            mouse_down.post_to_pid(pid);
        }

        std::thread::sleep(std::time::Duration::from_millis(15));

        if let Ok(mouse_up) = CGEvent::new_mouse_event(
            source,
            CGEventType::LeftMouseUp,
            point,
            CGMouseButton::Left,
        ) {
            mouse_up.post_to_pid(pid);
        }

        std::thread::sleep(std::time::Duration::from_millis(15));
    }

    fn tap(&mut self, key: &str) {
        let pid = match self.pid {
            Some(p) => p,
            None => {
                self.do_update();
                match self.pid {
                    Some(p) => p,
                    None => return,
                }
            }
        };

        // Parse modifiers (cmd+a, shift+up, etc)
        let parts: Vec<&str> = key.split('+').collect();
        let main_key = *parts.last().unwrap_or(&key);
        let mut modifiers = Vec::new();

        for part in &parts[..parts.len().saturating_sub(1)] {
            match part.to_lowercase().as_str() {
                "cmd" | "command" => modifiers.push("command down"),
                "shift" => modifiers.push("shift down"),
                "ctrl" | "control" => modifiers.push("control down"),
                "alt" | "option" => modifiers.push("option down"),
                _ => {}
            }
        }

        // Auto-detect uppercase
        let main_key_lower;
        if main_key.len() == 1 {
            let ch = main_key.chars().next().unwrap();
            if ch.is_ascii_uppercase() && !modifiers.contains(&"shift down") {
                modifiers.push("shift down");
            }
            main_key_lower = ch.to_lowercase().to_string();
        } else {
            main_key_lower = main_key.to_lowercase();
        }

        // Build AppleScript command
        let key_part = if let Some(code) = applescript_key_code(&main_key_lower) {
            format!("key code {}", code)
        } else if main_key_lower.len() == 1 {
            let escaped = main_key_lower.replace('"', "\\\"");
            format!("keystroke \"{}\"", escaped)
        } else {
            logger::warn(&format!("[darwin] unknown key: {}", main_key));
            return;
        };

        let modifier_str = if modifiers.is_empty() {
            String::new()
        } else {
            format!(" using {{{}}}", modifiers.join(", "))
        };

        let script = format!(
            "tell application \"System Events\" to tell process id {} to {}{}",
            pid, key_part, modifier_str
        );

        ProcessCommand::new("osascript")
            .arg("-e")
            .arg(&script)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .ok();

        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    fn type_text(&mut self, text: &str) {
        for ch in text.chars() {
            self.tap(&ch.to_string());
        }
    }

    fn capture(&mut self, rect: Option<CaptureRect>) -> Option<Capture> {
        self.do_update();
        let region = self.region?;

        let cg_rect = match rect {
            Some(r) => CGRect::new(
                &CGPoint::new(
                    (region.l + r.l) as f64,
                    (region.t + r.t) as f64,
                ),
                &CGSize::new(r.w as f64, r.h as f64),
            ),
            None => CGRect::new(
                &CGPoint::new(0.0, 0.0),
                &CGSize::new(0.0, 0.0),
            ),
        };

        let image_option = kCGWindowImageBoundsIgnoreFraming | kCGWindowImageNominalResolution;
        let image = create_image(
            cg_rect,
            kCGWindowListOptionIncludingWindow,
            self.window_id,
            image_option,
        )?;

        let bpr = image.bytes_per_row() as u32;
        let width = bpr / 4; // real width from bytes per row
        let height = image.height() as u32;

        // Get raw pixel data
        let cf_data = image.data();
        let bytes = cf_data.bytes();

        Some(Capture {
            data: bytes.to_vec(),
            width,
            height,
            bytes_per_row: bpr,
        })
    }
}

// --- CF Dictionary helpers ---

unsafe fn get_cf_string(
    dict: &CFDictionary<CFString, *const std::ffi::c_void>,
    key: &str,
) -> Option<String> {
    let cf_key = CFString::new(key);
    let value = dict.find(&cf_key)?;
    let cf_str: CFString = CFString::wrap_under_get_rule(*value as _);
    Some(cf_str.to_string())
}

unsafe fn get_cf_number(
    dict: &CFDictionary<CFString, *const std::ffi::c_void>,
    key: &str,
) -> Option<i64> {
    let cf_key = CFString::new(key);
    let value = dict.find(&cf_key)?;
    let cf_num: CFNumber = CFNumber::wrap_under_get_rule(*value as _);
    cf_num.to_i64()
}

unsafe fn get_cf_dict(
    dict: &CFDictionary<CFString, *const std::ffi::c_void>,
    key: &str,
) -> Option<CFDictionary<CFString, *const std::ffi::c_void>> {
    let cf_key = CFString::new(key);
    let value = dict.find(&cf_key)?;
    Some(CFDictionary::wrap_under_get_rule(*value as _))
}
