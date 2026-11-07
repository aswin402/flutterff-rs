use std::collections::VecDeque;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::config::{
    BAR_WIDTH, BUILDING_ROWS, DONE_ROWS, MAX_LOG_HISTORY, MIN_NAME_WIDTH, PKG_NAME,
    SEPARATOR_WIDTH, VERSION,
};
use crate::flutter::strip_ansi;
use crate::mcp;
use crate::theme::{
    BASE_RGB, BOLD, CYAN, CYAN_RGB, DIM, GREEN, MINT_RGB, RED, RESET, SAPPHIRE_RGB, SUB,
    SUBTEXT_RGB, SURFACE1_RGB, TEXT, TEXT_RGB, blend, fg_rgb, sample_flutterff_ramp,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Step {
    NetCheck,
    GtkShell,
    FlutterDaemon,
    CompileWeb,
    BindServer,
}

impl Step {
    pub const TOTAL: usize = 5;

    pub fn label(self) -> &'static str {
        match self {
            Self::NetCheck => "network check",
            Self::GtkShell => "gtk3 + webkit2 shell",
            Self::FlutterDaemon => "flutter web-server daemon",
            Self::CompileWeb => "compile lib/main.dart (web)",
            Self::BindServer => "bind local web-server",
        }
    }
}

pub struct PrettyProgress {
    pub flag_line: String,
    pub window_label: String,
    pub width: i32,
    pub height: i32,
    pub port: u16,
    pub mode_label: String,
    pub seen: usize,
    pub building: Vec<(Step, String, Instant)>,
    pub done: Vec<(String, f32)>,
    pub settled: Option<bool>,
    pub tui_finished: bool,
    pub server_url: Option<String>,
    pub error_msg: Option<String>,
    pub queued_logs: Vec<String>,
    pub log_history: VecDeque<String>,
}

impl PrettyProgress {
    pub fn new(
        flag_line: String,
        window_label: String,
        width: i32,
        height: i32,
        port: u16,
        mode_label: String,
    ) -> Self {
        Self {
            flag_line,
            window_label,
            width,
            height,
            port,
            mode_label,
            seen: 0,
            building: Vec::new(),
            done: Vec::new(),
            settled: None,
            tui_finished: false,
            server_url: None,
            error_msg: None,
            queued_logs: Vec::new(),
            log_history: VecDeque::with_capacity(MAX_LOG_HISTORY),
        }
    }

    pub fn start_step(&mut self, step: Step, custom_label: Option<String>) {
        if !self.building.iter().any(|(s, _, _)| *s == step) {
            let label = custom_label.unwrap_or_else(|| step.label().to_string());
            self.building.push((step, label, Instant::now()));
        }
    }

    pub fn finish_step(&mut self, step: Step, done_label: Option<String>) {
        let (default_label, secs) = match self.building.iter().position(|(s, _, _)| *s == step) {
            Some(pos) => {
                let (_, lbl, start) = self.building.remove(pos);
                (lbl, start.elapsed().as_secs_f32())
            }
            None => (step.label().to_string(), 0.01),
        };
        self.seen = (self.seen + 1).min(Step::TOTAL);
        self.done
            .push((done_label.unwrap_or(default_label), secs.max(0.01)));
    }

    fn push_history(&mut self, plain_line: String) {
        if self.log_history.len() >= MAX_LOG_HISTORY {
            self.log_history.pop_front();
        }
        self.log_history.push_back(plain_line);
    }

    pub fn log_or_print(&mut self, formatted: String) {
        self.push_history(strip_ansi(&formatted));
        if self.tui_finished {
            println!("{}", formatted);
        } else {
            self.queued_logs.push(formatted);
        }
    }
}

pub fn spawn_pretty_tui(progress: Arc<Mutex<PrettyProgress>>, started: Instant) {
    thread::spawn(move || {
        let mut drawn_lines: usize = 0;
        let sep = "─".repeat(SEPARATOR_WIDTH);
        print!("\x1b[?25l");
        let _ = std::io::stdout().flush();

        let render_frame = |s: &PrettyProgress, settled_mode: bool, drawn: &mut usize| {
            let elapsed = started.elapsed().as_secs_f32();
            let ratio = if settled_mode && s.settled == Some(true) {
                1.0
            } else {
                (s.seen as f32 / Step::TOTAL as f32).min(1.0)
            };
            let filled = (BAR_WIDTH as f32 * ratio).round() as usize;

            let mut lines: Vec<String> = Vec::with_capacity(20);
            lines.push(format!(
                "🦊 {}{}{}{} {}v{}{}",
                BOLD, CYAN, PKG_NAME, RESET, DIM, VERSION, RESET
            ));
            lines.push(format!("{}{}{}", DIM, s.flag_line, RESET));
            lines.push(format!("{}{}{}", DIM, sep, RESET));
            lines.push(format!("{}Building{}", DIM, RESET));

            let b_count = s.building.len().min(BUILDING_ROWS);
            let b_name_w = s
                .building
                .iter()
                .take(BUILDING_ROWS)
                .map(|(_, n, _)| n.len())
                .max()
                .unwrap_or(0)
                .max(MIN_NAME_WIDTH - 1)
                + 1;

            for i in 0..BUILDING_ROWS {
                match s.building.get(i) {
                    Some((_, name, st)) => {
                        let alpha = (i + 1) as f32 / b_count.max(1) as f32;
                        let dot_c = fg_rgb(blend(CYAN_RGB, BASE_RGB, alpha));
                        let name_c = fg_rgb(blend(SAPPHIRE_RGB, BASE_RGB, alpha));
                        let time_c = fg_rgb(blend(SUBTEXT_RGB, BASE_RGB, alpha));
                        let secs = st.elapsed().as_secs_f32();
                        lines.push(format!(
                            "{}● {}{}{:<width$}{}{}{:05.2}s{}",
                            dot_c,
                            RESET,
                            name_c,
                            name,
                            RESET,
                            time_c,
                            secs,
                            RESET,
                            width = b_name_w
                        ));
                    }
                    None if i == 0 && s.building.is_empty() => {
                        lines.push(format!("{}  …{}", DIM, RESET));
                    }
                    None => lines.push(String::new()),
                }
            }

            lines.push(format!("{}{}{}", DIM, sep, RESET));
            lines.push(format!(
                "{}Completed ({}/{}){}",
                DIM,
                s.seen,
                Step::TOTAL,
                RESET
            ));

            let recent: Vec<&(String, f32)> = s.done.iter().rev().take(DONE_ROWS).collect();
            let r_count = recent.len().max(1);
            lines.extend(std::iter::repeat_n(
                String::new(),
                DONE_ROWS.saturating_sub(recent.len()),
            ));
            let d_name_w = recent
                .iter()
                .map(|(n, _)| n.len())
                .max()
                .unwrap_or(0)
                .max(MIN_NAME_WIDTH - 1)
                + 1;

            for (i, (name, secs)) in recent.into_iter().rev().enumerate() {
                let alpha = (i + 1) as f32 / r_count as f32;
                let chk_c = fg_rgb(blend(MINT_RGB, BASE_RGB, alpha));
                let name_c = fg_rgb(blend(TEXT_RGB, BASE_RGB, alpha));
                let time_c = fg_rgb(blend(SUBTEXT_RGB, BASE_RGB, alpha));
                lines.push(format!(
                    "{}✓ {}{}{:<width$}{}{}{:05.2}s{}",
                    chk_c,
                    RESET,
                    name_c,
                    name,
                    RESET,
                    time_c,
                    secs,
                    RESET,
                    width = d_name_w
                ));
            }

            lines.push(String::new());

            let bar_filled = if settled_mode {
                let col = if s.settled == Some(true) { GREEN } else { RED };
                format!("{}{}{}", col, "█".repeat(filled), RESET)
            } else {
                let phase = elapsed * 0.9;
                let mut buf: String = (0..filled)
                    .map(|bi| {
                        let u = (bi as f32 / BAR_WIDTH as f32) - phase;
                        format!("{}█", fg_rgb(sample_flutterff_ramp(u)))
                    })
                    .collect();
                buf.push_str(RESET);
                buf
            };
            let bar_empty = format!(
                "{}{}{}",
                fg_rgb(SURFACE1_RGB),
                "░".repeat(BAR_WIDTH.saturating_sub(filled)),
                RESET
            );

            lines.push(format!(
                "{}{:<14}{}{}{}{}{:>5.0}%{}",
                DIM,
                "Launching",
                RESET,
                bar_filled,
                bar_empty,
                TEXT,
                ratio * 100.0,
                RESET
            ));
            lines.push(format!(
                "{}{:<14}{}{}{:.2}s{}",
                DIM, "Elapsed", RESET, TEXT, elapsed, RESET
            ));

            let mut out = String::new();
            if *drawn > 0 {
                out.push_str(&format!("\x1b[{}A", *drawn));
            }
            for l in &lines {
                out.push_str(&format!("\r\x1b[2K{}\n", l));
            }
            print!("{}", out);
            let _ = std::io::stdout().flush();
            *drawn = lines.len();
        };

        loop {
            let is_settled = {
                let guard = progress.lock().unwrap();
                render_frame(&guard, false, &mut drawn_lines);
                guard.settled.is_some()
            };
            if is_settled {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }

        for _ in 0..4 {
            {
                let guard = progress.lock().unwrap();
                render_frame(&guard, true, &mut drawn_lines);
            }
            thread::sleep(Duration::from_millis(40));
        }

        if drawn_lines > 0 {
            print!("\x1b[{}A", drawn_lines);
            for _ in 0..drawn_lines {
                print!("\r\x1b[2K\n");
            }
            print!("\x1b[{}A", drawn_lines);
        }
        print!("\x1b[?25h");
        let _ = std::io::stdout().flush();

        let total_secs = started.elapsed().as_secs_f32();
        let mut guard = progress.lock().unwrap();
        let ok = guard.settled.unwrap_or(false);
        let srv = guard
            .server_url
            .clone()
            .unwrap_or_else(|| "stopped".to_string());

        if ok {
            println!(
                "{}{}✓ {} v{} serving in {:.2}s{}",
                GREEN, BOLD, PKG_NAME, VERSION, total_secs, RESET
            );
            println!(
                "{}  {:<15}{}{}{}",
                DIM, "window", RESET, TEXT, guard.window_label
            );
            println!("{}  {:<15}{}{}{}", DIM, "server", RESET, CYAN, srv);
            println!(
                "{}  {:<15}{}{}{}",
                DIM, "mode", RESET, TEXT, guard.mode_label
            );
            println!(
                "{}  {:<15}{}{}{}",
                DIM,
                "mcp socket",
                RESET,
                SUB,
                mcp::socket_path().display()
            );
            println!("{}{}{}", DIM, sep, RESET);
        } else {
            println!(
                "{}{}✗ {} failed after {:.2}s{}",
                RED, BOLD, PKG_NAME, total_secs, RESET
            );
            if let Some(err) = &guard.error_msg {
                eprintln!("  {}{}{}", RED, err, RESET);
            }
        }

        for log in guard.queued_logs.drain(..) {
            println!("{}", log);
        }
        guard.tui_finished = true;
    });
}
