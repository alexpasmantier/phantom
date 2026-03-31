use std::io;
use std::os::unix::io::AsRawFd;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use phantom_core::protocol::ResponseData;
use phantom_core::types::ScreenFormat;
use phantom_daemon::engine::EngineCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Padding, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};
use ratatui::Terminal;
use throbber_widgets_tui::{Throbber, ThrobberState, BRAILLE_SIX};

use crate::phantom::PhantomInner;
use crate::runner::{RunnerEvent, TestResult, format_duration, run_tests_on_thread};

type TestFn = Box<dyn FnOnce(&crate::Phantom) -> crate::Result<()> + Send>;

// ── Palette ─────────────────────────────────────────────────

const PASS: Color = Color::Rgb(102, 204, 102);
const FAIL: Color = Color::Rgb(235, 80, 80);
const RUNNING: Color = Color::Rgb(255, 200, 60);
const DIM: Color = Color::Rgb(100, 100, 110);
const DIMMER: Color = Color::Rgb(60, 60, 68);
const TEXT: Color = Color::Rgb(210, 210, 220);
const BG: Color = Color::Rgb(22, 22, 30);
const SURFACE: Color = Color::Rgb(30, 30, 40);
const BORDER: Color = Color::Rgb(55, 55, 70);
const ACCENT: Color = Color::Rgb(130, 140, 255);

// ── State ───────────────────────────────────────────────────

struct TuiState {
    test_names: Vec<String>,
    results: Vec<Option<TestResult>>,
    running_idx: Option<usize>,
    running_since: Option<Instant>,
    active_inner: Option<Arc<PhantomInner>>,
    active_session_name: Option<String>,
    screen_lines: Vec<String>,
    done: bool,
    quit: bool,
    scroll_offset: u16,
    start_time: Instant,
    throbber_state: ThrobberState,
    frame_count: u64,
}

impl TuiState {
    fn new(names: Vec<String>) -> Self {
        let n = names.len();
        Self {
            test_names: names,
            results: vec![None; n],
            running_idx: None,
            running_since: None,
            active_inner: None,
            active_session_name: None,
            screen_lines: Vec::new(),
            done: false,
            quit: false,
            scroll_offset: 0,
            start_time: Instant::now(),
            throbber_state: ThrobberState::default(),
            frame_count: 0,
        }
    }

    fn passed(&self) -> usize {
        self.results
            .iter()
            .filter(|r| matches!(r, Some(TestResult::Passed(_))))
            .count()
    }

    fn failed(&self) -> usize {
        self.results
            .iter()
            .filter(|r| matches!(r, Some(TestResult::Failed(_, _))))
            .count()
    }

    fn finished(&self) -> usize {
        self.results.iter().filter(|r| r.is_some()).count()
    }

    fn total_duration(&self) -> Duration {
        self.start_time.elapsed()
    }

    fn tick(&mut self) {
        self.frame_count += 1;
        if self.frame_count % 4 == 0 {
            self.throbber_state.calc_next();
        }
    }

    fn poll_screenshot(&mut self) {
        let Some(ref inner) = self.active_inner else {
            return;
        };
        let Some(ref name) = self.active_session_name else {
            return;
        };

        let name = name.clone();
        let Ok(resp) = inner.send_command(|reply| EngineCommand::Screenshot {
            session: name,
            format: ScreenFormat::Text,
            region: None,
            reply,
        }) else {
            return;
        };

        if let phantom_core::protocol::Response::Ok {
            data: Some(ResponseData::Screen(screen)),
        } = resp
        {
            self.screen_lines = screen.screen.iter().map(|r| r.text.clone()).collect();
        }
    }

    fn process_event(&mut self, ev: RunnerEvent) {
        match ev {
            RunnerEvent::TestStarted(idx) => {
                self.running_idx = Some(idx);
                self.running_since = Some(Instant::now());
                self.screen_lines.clear();
                self.active_inner = None;
                self.active_session_name = None;
                self.scroll_offset = idx.saturating_sub(2) as u16;
            }
            RunnerEvent::TestFinished(idx, result) => {
                self.results[idx] = Some(result);
                self.running_idx = None;
                self.running_since = None;
            }
            RunnerEvent::SessionCreated(inner, name) => {
                self.active_inner = Some(inner);
                self.active_session_name = Some(name);
            }
            RunnerEvent::SessionEnded => {
                self.active_inner = None;
                self.active_session_name = None;
            }
            RunnerEvent::Done => {
                self.done = true;
            }
        }
    }
}

// ── Main loop ───────────────────────────────────────────────

pub(crate) fn run_with_tui(tests: Vec<(String, TestFn)>) -> ! {
    let names: Vec<String> = tests.iter().map(|(n, _)| n.clone()).collect();
    let (event_tx, event_rx) = crossbeam_channel::unbounded();

    let saved_stderr = unsafe { nix::libc::dup(2) };
    if let Ok(devnull) = std::fs::OpenOptions::new().write(true).open("/dev/null") {
        unsafe { nix::libc::dup2(devnull.as_raw_fd(), 2) };
    }

    let test_handle = run_tests_on_thread(tests, event_tx);

    enable_raw_mode().expect("failed to enable raw mode");
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).expect("failed to enter alternate screen");
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).expect("failed to create terminal");

    let mut state = TuiState::new(names);

    let exit_code = loop {
        while let Ok(ev) = event_rx.try_recv() {
            state.process_event(ev);
        }

        state.poll_screenshot();
        state.tick();

        terminal
            .draw(|frame| draw(frame, &mut state))
            .expect("failed to draw");

        if event::poll(Duration::from_millis(33)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            if state.done {
                                break if state.failed() == 0 { 0 } else { 1 };
                            }
                            state.quit = true;
                        }
                        _ if state.done => {
                            break if state.failed() == 0 { 0 } else { 1 };
                        }
                        _ => {}
                    }
                }
            }
        }

        if state.quit {
            break if state.failed() == 0 { 0 } else { 1 };
        }
    };

    disable_raw_mode().expect("failed to disable raw mode");
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .expect("failed to leave alternate screen");

    if saved_stderr >= 0 {
        unsafe {
            nix::libc::dup2(saved_stderr, 2);
            nix::libc::close(saved_stderr);
        }
    }

    if !state.quit {
        let _ = test_handle.join();
    }

    print_summary(&state);
    std::process::exit(exit_code);
}

// ── Drawing ─────────────────────────────────────────────────

fn draw(frame: &mut ratatui::Frame, state: &mut TuiState) {
    let size = frame.area();

    frame.render_widget(Block::default().style(Style::default().bg(BG)), size);

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // header
            Constraint::Min(6),   // content
            Constraint::Length(3), // progress block
        ])
        .split(size);

    draw_header(frame, outer[0], state);

    let content = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(outer[1]);

    draw_session(frame, content[0], state);
    draw_tests(frame, content[1], state);
    draw_footer(frame, outer[2], state);
}

fn draw_header(frame: &mut ratatui::Frame, area: Rect, state: &TuiState) {
    let elapsed = format_duration(state.total_duration());

    let right_spans = if let Some(idx) = state.running_idx {
        let name = &state.test_names[idx];
        let elapsed = state
            .running_since
            .map(|s| format_duration(s.elapsed()))
            .unwrap_or_default();
        vec![
            Span::styled("running ", Style::default().fg(DIM)),
            Span::styled(name.as_str(), Style::default().fg(RUNNING).bold()),
            Span::styled(format!(" {elapsed} "), Style::default().fg(DIMMER)),
        ]
    } else if state.done {
        vec![Span::styled(
            if state.failed() == 0 {
                "done "
            } else {
                "failed "
            },
            Style::default()
                .fg(if state.failed() == 0 { PASS } else { FAIL })
                .bold(),
        )]
    } else {
        vec![]
    };

    let left = vec![
        Span::styled(" phantom ", Style::default().fg(ACCENT).bold()),
        Span::styled(
            format!("{} tests", state.test_names.len()),
            Style::default().fg(DIM),
        ),
        Span::styled(format!("  {elapsed}"), Style::default().fg(DIMMER)),
    ];

    let left_len: usize = left.iter().map(|s| s.content.len()).sum();
    let right_len: usize = right_spans.iter().map(|s| s.content.len()).sum();
    let pad = (area.width as usize).saturating_sub(left_len + right_len);

    let mut spans = left;
    spans.push(Span::raw(" ".repeat(pad)));
    spans.extend(right_spans);

    let header = Paragraph::new(Line::from(spans)).style(Style::default().bg(SURFACE));
    frame.render_widget(header, area);
}

fn draw_session(frame: &mut ratatui::Frame, area: Rect, state: &TuiState) {
    let title_text = match &state.active_session_name {
        Some(name) => format!(" {name} "),
        None => " session ".to_string(),
    };

    let block = Block::default()
        .title(Line::from(title_text).style(Style::default().fg(ACCENT).bold()))
        .borders(Borders::ALL)
        .border_set(border::ROUNDED)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(BG))
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);

    let lines: Vec<Line> = if state.screen_lines.is_empty() {
        let msg = if state.running_idx.is_some() {
            "waiting for session..."
        } else {
            ""
        };
        vec![
            Line::from(""),
            Line::from(Span::styled(msg, Style::default().fg(DIMMER).italic())),
        ]
    } else {
        state
            .screen_lines
            .iter()
            .take(inner.height as usize)
            .map(|l| {
                let truncated: String = l.chars().take(inner.width as usize).collect();
                Line::from(Span::styled(truncated, Style::default().fg(TEXT)))
            })
            .collect()
    };

    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_tests(frame: &mut ratatui::Frame, area: Rect, state: &mut TuiState) {
    let block = Block::default()
        .title(Line::from(" tests ").style(Style::default().fg(ACCENT).bold()))
        .borders(Borders::ALL)
        .border_set(border::ROUNDED)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(BG))
        .padding(Padding::new(1, 1, 0, 0));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let max_name_w = (inner.width as usize).saturating_sub(14);
    let visible_height = inner.height as usize;

    let throbber = Throbber::default()
        .throbber_set(BRAILLE_SIX)
        .throbber_style(Style::default().fg(RUNNING));
    let spinner_span = throbber.to_symbol_span(&state.throbber_state);

    let mut lines: Vec<Line> = Vec::new();

    for (i, name) in state.test_names.iter().enumerate() {
        let is_running = state.running_idx == Some(i);
        let truncated: String = name.chars().take(max_name_w).collect();

        let elapsed_str = match &state.results[i] {
            Some(TestResult::Passed(d)) | Some(TestResult::Failed(d, _)) => format_duration(*d),
            None if is_running => state
                .running_since
                .map(|s| format_duration(s.elapsed()))
                .unwrap_or_default(),
            _ => String::new(),
        };

        let name_len = truncated.chars().count();
        let dur_len = elapsed_str.chars().count();
        let gap = max_name_w.saturating_sub(name_len) + 2;
        let pad = " ".repeat(gap.saturating_sub(dur_len));

        let line = match &state.results[i] {
            Some(TestResult::Passed(_)) => Line::from(vec![
                Span::styled(" ✓ ", Style::default().fg(PASS)),
                Span::styled(truncated, Style::default().fg(TEXT)),
                Span::raw(pad),
                Span::styled(elapsed_str, Style::default().fg(DIMMER)),
            ]),
            Some(TestResult::Failed(_, _)) => Line::from(vec![
                Span::styled(" ✗ ", Style::default().fg(FAIL).bold()),
                Span::styled(truncated, Style::default().fg(FAIL)),
                Span::raw(pad),
                Span::styled(elapsed_str, Style::default().fg(DIMMER)),
            ]),
            None if is_running => Line::from(vec![
                Span::raw(" "),
                spinner_span.clone(),
                Span::raw(" "),
                Span::styled(truncated, Style::default().fg(RUNNING).bold()),
                Span::raw(pad),
                Span::styled(elapsed_str, Style::default().fg(DIMMER)),
            ]),
            None => Line::from(vec![
                Span::styled("   ", Style::default()),
                Span::styled(truncated, Style::default().fg(DIMMER)),
            ]),
        };

        lines.push(line);

        if let Some(TestResult::Failed(_, msg)) = &state.results[i] {
            let err_w = (inner.width as usize).saturating_sub(5);
            let err_truncated: String = msg.chars().take(err_w).collect();
            lines.push(Line::from(vec![
                Span::raw("     "),
                Span::styled(err_truncated, Style::default().fg(FAIL).italic()),
            ]));
        }
    }

    let total_lines = lines.len();
    let offset = state.scroll_offset as usize;
    let visible: Vec<Line> = lines
        .into_iter()
        .skip(offset)
        .take(visible_height)
        .collect();

    frame.render_widget(Paragraph::new(visible), inner);

    if total_lines > visible_height {
        let mut scrollbar_state = ScrollbarState::new(total_lines)
            .position(offset)
            .viewport_content_length(visible_height);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(DIMMER)),
            area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut scrollbar_state,
        );
    }
}

fn draw_footer(frame: &mut ratatui::Frame, area: Rect, state: &TuiState) {
    let total = state.test_names.len();
    let passed = state.passed();
    let failed = state.failed();
    let finished = state.finished();
    let ratio = if total == 0 {
        0.0
    } else {
        finished as f64 / total as f64
    };

    let block = Block::default()
        .borders(Borders::TOP)
        .border_set(border::Set {
            top_left: "─",
            top_right: "─",
            ..border::ROUNDED
        })
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(BG))
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(inner);

    // Row 1: gradient progress bar
    let bar_line = gradient_bar(ratio, rows[0].width as usize, state.failed() > 0, state.done);
    frame.render_widget(Paragraph::new(bar_line), rows[0]);

    // Row 2: status text
    let left_spans = if state.done {
        if failed == 0 {
            vec![
                Span::styled("✓ ", Style::default().fg(PASS).bold()),
                Span::styled(
                    format!("all {total} tests passed"),
                    Style::default().fg(PASS).bold(),
                ),
            ]
        } else {
            vec![
                Span::styled("✗ ", Style::default().fg(FAIL).bold()),
                Span::styled(format!("{passed} passed"), Style::default().fg(PASS)),
                Span::styled(" · ", Style::default().fg(DIMMER)),
                Span::styled(
                    format!("{failed} failed"),
                    Style::default().fg(FAIL).bold(),
                ),
                Span::styled(format!(" / {total}"), Style::default().fg(DIM)),
            ]
        }
    } else {
        let pct = (ratio * 100.0).round() as u16;
        vec![
            Span::styled(format!("{pct}% "), Style::default().fg(ACCENT).bold()),
            Span::styled(format!("{finished}/{total}"), Style::default().fg(DIM)),
            Span::styled("  ", Style::default()),
            Span::styled(
                format!("{passed} passed"),
                Style::default().fg(if passed > 0 { PASS } else { DIM }),
            ),
            Span::styled(" · ", Style::default().fg(DIMMER)),
            Span::styled(
                format!("{failed} failed"),
                Style::default().fg(if failed > 0 { FAIL } else { DIM }),
            ),
        ]
    };

    let right = if state.done {
        "press any key"
    } else {
        "q: quit"
    };

    let left_len: usize = left_spans.iter().map(|s| s.content.len()).sum();
    let pad = (rows[1].width as usize).saturating_sub(left_len + right.len());

    let mut spans = left_spans;
    spans.push(Span::raw(" ".repeat(pad)));
    spans.push(Span::styled(right, Style::default().fg(DIMMER)));

    frame.render_widget(Paragraph::new(Line::from(spans)), rows[1]);
}

// ── Gradient progress bar ────────────────────────────────────

fn lerp_color(a: (u8, u8, u8), b: (u8, u8, u8), t: f64) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color::Rgb(
        (a.0 as f64 + (b.0 as f64 - a.0 as f64) * t) as u8,
        (a.1 as f64 + (b.1 as f64 - a.1 as f64) * t) as u8,
        (a.2 as f64 + (b.2 as f64 - a.2 as f64) * t) as u8,
    )
}

fn gradient_bar(ratio: f64, width: usize, has_failures: bool, done: bool) -> Line<'static> {
    if width == 0 {
        return Line::from("");
    }

    // Color endpoints for the gradient
    let (start, end) = if has_failures {
        ((235, 80, 80), (180, 60, 60)) // red gradient
    } else if done {
        ((80, 180, 120), (102, 204, 102)) // green gradient
    } else {
        ((90, 80, 200), (130, 180, 255)) // blue-purple → light blue
    };

    let track_color = SURFACE;

    // Sub-cell precision: multiply by 2 for half-block resolution
    let fill_units = (ratio * width as f64 * 2.0).round() as usize;
    let full_cells = fill_units / 2;
    let has_half = fill_units % 2 == 1;

    let mut spans: Vec<Span<'static>> = Vec::with_capacity(width);

    for i in 0..width {
        if i < full_cells {
            let t = if full_cells <= 1 {
                0.0
            } else {
                i as f64 / (full_cells - 1) as f64
            };
            let color = lerp_color(start, end, t);
            spans.push(Span::styled("█", Style::default().fg(color)));
        } else if i == full_cells && has_half {
            let t = if full_cells == 0 {
                0.0
            } else {
                full_cells as f64 / full_cells as f64
            };
            let color = lerp_color(start, end, t.min(1.0));
            // Half-block: foreground is the bar color, background is the track
            spans.push(Span::styled(
                "▌",
                Style::default().fg(color).bg(track_color),
            ));
        } else {
            spans.push(Span::styled(
                "░",
                Style::default().fg(track_color),
            ));
        }
    }

    Line::from(spans)
}

// ── Summary (printed after TUI exits) ───────────────────────

fn print_summary(state: &TuiState) {
    let total = state.test_names.len();
    let passed = state.passed();
    let failed = state.failed();
    let elapsed = format_duration(state.total_duration());

    println!();
    println!("  phantom integration tests ({elapsed})");
    println!("  ────────────────────────────\n");

    for (i, name) in state.test_names.iter().enumerate() {
        match &state.results[i] {
            Some(TestResult::Passed(d)) => {
                println!(
                    "  \x1b[32m✓\x1b[0m {name} \x1b[2m{}\x1b[0m",
                    format_duration(*d)
                );
            }
            Some(TestResult::Failed(d, msg)) => {
                println!(
                    "  \x1b[31m✗\x1b[0m {name} \x1b[2m{}\x1b[0m",
                    format_duration(*d)
                );
                println!("    \x1b[31m{msg}\x1b[0m");
            }
            None => {
                println!("  \x1b[2m- {name} (not run)\x1b[0m");
            }
        }
    }

    println!("\n  ────────────────────────────");
    if failed == 0 {
        println!("  \x1b[32m\x1b[1m✓ {passed}/{total} passed\x1b[0m\n");
    } else {
        println!(
            "  \x1b[32m{passed} passed\x1b[0m, \x1b[31m\x1b[1m{failed} failed\x1b[0m / {total}\n"
        );
    }
}
