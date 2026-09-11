//! The Elm-architecture runtime.
//!
//! Reimplements the part of `charm.land/bubbletea` glow drives: a model with
//! `init`/`update`/`view`, commands that run off the main loop, the alternate
//! screen, optional cell-motion mouse tracking, suspend, and the key names
//! `KeyPressMsg.String()` produces — those names are the TUI's key bindings, so
//! they are part of the contract.

use std::io::Write;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, Event as CtEvent, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers,
};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::{cursor, queue, terminal};

/// Something that happened at the terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A key was pressed, named the way bubbletea names it.
    Key(String),
    /// The window was resized.
    Resize(u16, u16),
    /// The terminal reported its background colour.
    BackgroundColor(bool),
}

/// Work the runtime performs on the model's behalf.
pub enum Cmd<M> {
    /// Nothing to do.
    None,
    /// Stop the program.
    Quit,
    /// Stop the program and hand the terminal back to the shell.
    Suspend,
    /// Several commands, run concurrently.
    Batch(Vec<Cmd<M>>),
    /// A function run off the main loop, whose result becomes a message.
    Async(Box<dyn FnOnce() -> Option<M> + Send>),
    /// A child process run with the terminal restored, then a message.
    Exec(Vec<String>, Box<dyn FnOnce(Option<String>) -> M + Send>),
}

impl<M> Cmd<M> {
    /// Collects commands, dropping the empty ones.
    pub fn batch(cmds: Vec<Cmd<M>>) -> Cmd<M> {
        let cmds: Vec<Cmd<M>> = cmds
            .into_iter()
            .filter(|c| !matches!(c, Cmd::None))
            .collect();
        match cmds.len() {
            0 => Cmd::None,
            1 => cmds.into_iter().next().expect("one command"),
            _ => Cmd::Batch(cmds),
        }
    }

    /// A command that produces `msg` after `delay`.
    pub fn tick(delay: Duration, msg: M) -> Cmd<M>
    where
        M: Send + 'static,
    {
        Cmd::Async(Box::new(move || {
            std::thread::sleep(delay);
            Some(msg)
        }))
    }
}

/// What the model wants drawn.
pub struct View {
    /// The frame.
    pub content: String,
    /// Whether to use the alternate screen.
    pub alt_screen: bool,
    /// Whether to track the mouse.
    pub mouse: bool,
}

/// A Bubble Tea model.
pub trait Model {
    /// The message type the model consumes.
    type Msg: Send + 'static;

    /// Commands to run before the first frame.
    fn init(&mut self) -> Cmd<Self::Msg>;
    /// Folds a message into the model.
    fn update(&mut self, msg: Self::Msg) -> Cmd<Self::Msg>;
    /// Renders the current state.
    fn view(&self) -> View;
    /// Translates a terminal event into a message, or ignores it.
    fn on_event(&self, event: Event) -> Option<Self::Msg>;
}

/// Runs `model` to completion, returning it.
pub fn run<M: Model>(mut model: M) -> Result<M, String> {
    let mut term = Terminal::new()?;
    let (tx, rx): (Sender<M::Msg>, Receiver<M::Msg>) = channel();

    // Terminal events are read on their own thread and translated by the model.
    let (etx, erx) = channel::<Event>();
    let reader = std::thread::spawn(move || loop {
        match crossterm::event::read() {
            Ok(CtEvent::Key(k)) => {
                if k.kind != KeyEventKind::Release {
                    if let Some(name) = key_name(&k) {
                        if etx.send(Event::Key(name)).is_err() {
                            return;
                        }
                    }
                }
            }
            Ok(CtEvent::Resize(w, h)) => {
                if etx.send(Event::Resize(w, h)).is_err() {
                    return;
                }
            }
            Ok(_) => {}
            Err(_) => return,
        }
    });

    let mut pending = 1usize;
    spawn(&tx, &mut pending, model.init(), &mut term, &mut model)?;

    // The starting state the program would otherwise learn from the terminal.
    if let Ok((w, h)) = terminal::size() {
        if let Some(msg) = model.on_event(Event::Resize(w, h)) {
            let cmd = model.update(msg);
            spawn(&tx, &mut pending, cmd, &mut term, &mut model)?;
        }
    }
    if let Some(msg) = model.on_event(Event::BackgroundColor(
        super::lipgloss::has_dark_background(),
    )) {
        let cmd = model.update(msg);
        spawn(&tx, &mut pending, cmd, &mut term, &mut model)?;
    }

    term.render(&model.view())?;

    'outer: loop {
        // Drain whichever source has something ready.
        let mut got = false;
        while let Ok(event) = erx.try_recv() {
            got = true;
            if let Some(msg) = model.on_event(event) {
                let cmd = model.update(msg);
                if spawn(&tx, &mut pending, cmd, &mut term, &mut model)? {
                    break 'outer;
                }
            }
        }
        while let Ok(msg) = rx.try_recv() {
            got = true;
            let cmd = model.update(msg);
            if spawn(&tx, &mut pending, cmd, &mut term, &mut model)? {
                break 'outer;
            }
        }
        if got {
            term.render(&model.view())?;
            continue;
        }
        std::thread::sleep(Duration::from_millis(4));
    }

    term.restore()?;
    drop(erx);
    drop(reader);
    Ok(model)
}

/// Dispatches a command, returning whether the program should quit.
fn spawn<M: Model>(
    tx: &Sender<M::Msg>,
    pending: &mut usize,
    cmd: Cmd<M::Msg>,
    term: &mut Terminal,
    model: &mut M,
) -> Result<bool, String> {
    match cmd {
        Cmd::None => Ok(false),
        Cmd::Quit => Ok(true),
        Cmd::Suspend => {
            term.restore()?;
            #[cfg(unix)]
            // SAFETY: raising SIGTSTP only affects this process group.
            unsafe {
                libc::raise(libc::SIGTSTP);
            }
            term.setup(&model.view())?;
            Ok(false)
        }
        Cmd::Batch(cmds) => {
            for c in cmds {
                if spawn(tx, pending, c, term, model)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Cmd::Async(f) => {
            *pending += 1;
            let tx = tx.clone();
            std::thread::spawn(move || {
                if let Some(msg) = f() {
                    let _ = tx.send(msg);
                }
            });
            Ok(false)
        }
        Cmd::Exec(argv, done) => {
            term.restore()?;
            let result = std::process::Command::new(&argv[0])
                .args(&argv[1..])
                .status();
            term.setup(&model.view())?;
            let err = match result {
                Ok(s) if s.success() => None,
                Ok(s) => Some(format!("exit status {}", s.code().unwrap_or(1))),
                Err(e) => Some(super::go_exec_error(&argv[0], &e)),
            };
            let _ = tx.send(done(err));
            Ok(false)
        }
    }
}

/// The terminal's mode, restored on drop.
struct Terminal {
    raw: bool,
    alt: bool,
    mouse: bool,
    hid_cursor: bool,
    last_lines: usize,
}

impl Terminal {
    fn new() -> Result<Terminal, String> {
        Ok(Terminal {
            raw: false,
            alt: false,
            mouse: false,
            hid_cursor: false,
            last_lines: 0,
        })
    }

    fn setup(&mut self, view: &View) -> Result<(), String> {
        let mut out = std::io::stdout();
        if !self.raw {
            // The program drives the controlling terminal, not stdout, so a
            // detached process fails here rather than drawing into a pipe.
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open("/dev/tty")
                .map_err(|e| {
                    format!(
                        "bubbletea: error opening TTY: bubbletea: could not open TTY: open /dev/tty: {}",
                        super::go_errno(&e)
                    )
                })?;
            enable_raw_mode().map_err(|e| e.to_string())?;
            self.raw = true;
            self.hid_cursor = true;
        }
        if view.alt_screen && !self.alt {
            queue!(out, EnterAlternateScreen).map_err(|e| e.to_string())?;
            self.alt = true;
        }
        if view.mouse && !self.mouse {
            queue!(out, EnableMouseCapture).map_err(|e| e.to_string())?;
            self.mouse = true;
        }
        queue!(out, cursor::Hide).map_err(|e| e.to_string())?;
        out.flush().map_err(|e| e.to_string())?;
        self.last_lines = 0;
        Ok(())
    }

    fn render(&mut self, view: &View) -> Result<(), String> {
        if !self.raw || (view.alt_screen && !self.alt) || (view.mouse && !self.mouse) {
            self.setup(view)?;
        }
        let mut out = std::io::stdout();
        queue!(out, cursor::MoveTo(0, 0)).map_err(|e| e.to_string())?;
        let lines: Vec<&str> = view.content.split('\n').collect();
        for (i, line) in lines.iter().enumerate() {
            if i > 0 {
                out.write_all(b"\r\n").map_err(|e| e.to_string())?;
            }
            out.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
            queue!(out, terminal::Clear(terminal::ClearType::UntilNewLine))
                .map_err(|e| e.to_string())?;
        }
        if lines.len() < self.last_lines {
            queue!(out, terminal::Clear(terminal::ClearType::FromCursorDown))
                .map_err(|e| e.to_string())?;
        }
        self.last_lines = lines.len();
        out.flush().map_err(|e| e.to_string())?;
        Ok(())
    }

    fn restore(&mut self) -> Result<(), String> {
        let mut out = std::io::stdout();
        if self.mouse {
            queue!(out, DisableMouseCapture).map_err(|e| e.to_string())?;
            self.mouse = false;
        }
        if self.alt {
            queue!(out, LeaveAlternateScreen).map_err(|e| e.to_string())?;
            self.alt = false;
        }
        if self.hid_cursor {
            queue!(out, cursor::Show).map_err(|e| e.to_string())?;
            self.hid_cursor = false;
        }
        out.flush().map_err(|e| e.to_string())?;
        if self.raw {
            disable_raw_mode().map_err(|e| e.to_string())?;
            self.raw = false;
        }
        Ok(())
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

/// The name bubbletea gives a key press.
pub fn key_name(k: &KeyEvent) -> Option<String> {
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    let alt = k.modifiers.contains(KeyModifiers::ALT);
    let shift = k.modifiers.contains(KeyModifiers::SHIFT);

    let base = match k.code {
        KeyCode::Char(' ') => "space".to_string(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "enter".into(),
        KeyCode::Esc => "esc".into(),
        KeyCode::Tab => "tab".into(),
        KeyCode::BackTab => return Some("shift+tab".into()),
        KeyCode::Backspace => "backspace".into(),
        KeyCode::Delete => "delete".into(),
        KeyCode::Insert => "insert".into(),
        KeyCode::Left => "left".into(),
        KeyCode::Right => "right".into(),
        KeyCode::Up => "up".into(),
        KeyCode::Down => "down".into(),
        KeyCode::Home => "home".into(),
        KeyCode::End => "end".into(),
        KeyCode::PageUp => "pgup".into(),
        KeyCode::PageDown => "pgdown".into(),
        KeyCode::F(n) => format!("f{n}"),
        _ => return None,
    };

    let mut name = String::new();
    if ctrl {
        name.push_str("ctrl+");
    }
    if alt {
        name.push_str("alt+");
    }
    // A shifted letter arrives already upper-cased, so the modifier is only
    // spelled out for the keys that have no shifted form of their own.
    if shift && !matches!(k.code, KeyCode::Char(_)) {
        name.push_str("shift+");
    }
    name.push_str(&base);
    Some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, mods: KeyModifiers) -> String {
        key_name(&KeyEvent::new(code, mods)).expect("a name")
    }

    #[test]
    fn plain_keys_are_named_by_their_character() {
        assert_eq!(key(KeyCode::Char('j'), KeyModifiers::NONE), "j");
        assert_eq!(key(KeyCode::Char('G'), KeyModifiers::SHIFT), "G");
        assert_eq!(key(KeyCode::Char(' '), KeyModifiers::NONE), "space");
    }

    #[test]
    fn control_and_alt_are_spelled_out() {
        assert_eq!(key(KeyCode::Char('c'), KeyModifiers::CONTROL), "ctrl+c");
        assert_eq!(key(KeyCode::Char('z'), KeyModifiers::CONTROL), "ctrl+z");
        assert_eq!(key(KeyCode::Char('k'), KeyModifiers::ALT), "alt+k");
    }

    #[test]
    fn navigation_keys_use_bubbleteas_names() {
        assert_eq!(key(KeyCode::PageUp, KeyModifiers::NONE), "pgup");
        assert_eq!(key(KeyCode::PageDown, KeyModifiers::NONE), "pgdown");
        assert_eq!(key(KeyCode::BackTab, KeyModifiers::SHIFT), "shift+tab");
        assert_eq!(key(KeyCode::Esc, KeyModifiers::NONE), "esc");
        assert_eq!(key(KeyCode::Enter, KeyModifiers::NONE), "enter");
    }

    #[test]
    fn batch_drops_empty_commands() {
        let cmd: Cmd<()> = Cmd::batch(vec![Cmd::None, Cmd::None]);
        assert!(matches!(cmd, Cmd::None));
        let cmd: Cmd<()> = Cmd::batch(vec![Cmd::None, Cmd::Quit]);
        assert!(matches!(cmd, Cmd::Quit));
        let cmd: Cmd<()> = Cmd::batch(vec![Cmd::Quit, Cmd::Quit]);
        assert!(matches!(cmd, Cmd::Batch(v) if v.len() == 2));
    }
}
