//! The terminal user interface.

pub mod config;
pub mod editor;
pub mod ignore;
pub mod keys;
pub mod markdown;
pub mod pager;
pub mod sort;
pub mod stash;
pub mod stashhelp;
pub mod stashitem;
pub mod styles;

use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub use config::Config;

use crate::deps::bubbletea::{self, Cmd, Event, View};
use crate::deps::gitcha::{self, SearchResult};
use crate::log;
use crate::utils;

use markdown::Markdown;
use pager::PagerModel;
use stash::{StashModel, StashViewState};
use styles::Styles;

/// How long a status message like "Copied contents" stays up.
pub const STATUS_MESSAGE_TIMEOUT: Duration = Duration::from_secs(3);
/// The character that marks truncated text.
pub const ELLIPSIS: &str = "…";

/// The file patterns the document search looks for.
pub const MARKDOWN_EXTENSIONS: [&str; 5] = ["*.md", "*.mdown", "*.mkdn", "*.mkd", "*.markdown"];

/// The part of the application a message applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppContext {
    /// The file listing.
    Stash,
    /// The pager.
    Pager,
}

/// The top-level application state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Showing the file listing.
    ShowStash,
    /// Showing a document.
    ShowDocument,
}

/// Everything both sub-models need.
pub struct CommonModel {
    /// Resolved configuration.
    pub cfg: Config,
    /// The directory the search started from.
    pub cwd: String,
    /// Terminal width.
    pub width: usize,
    /// Terminal height.
    pub height: usize,
    /// Resolved styles.
    pub styles: Styles,
}

/// A message the model folds in.
pub enum Msg {
    /// A key press, named the way bubbletea names it.
    Key(String),
    /// The terminal was resized.
    WindowSize {
        /// New width.
        width: usize,
        /// New height.
        height: usize,
    },
    /// The terminal reported its background colour.
    BackgroundColor(bool),
    /// Something failed.
    Err(String),
    /// The document search started.
    InitLocalFileSearch {
        /// The directory being searched.
        cwd: String,
        /// The stream of results.
        rx: Arc<Mutex<Receiver<SearchResult>>>,
    },
    /// One document was found.
    FoundLocalFile(SearchResult),
    /// The document search ended.
    LocalFileSearchFinished,
    /// A status message's time is up.
    StatusMessageTimeout(AppContext),
    /// A document's contents were read.
    FetchedMarkdown(Box<Markdown>),
    /// A document was rendered.
    ContentRendered(String),
    /// The filter produced a new result set.
    FilteredMarkdown(Vec<usize>),
    /// The document changed on disk.
    Reload,
    /// Start watching the current document's directory.
    WatchFile,
    /// The editor exited.
    EditorFinished(Option<String>),
    /// The spinner should advance.
    SpinnerTick,
    /// The filter cursor should blink.
    Blink,
}

/// The application model.
pub struct Model {
    common: CommonModel,
    state: State,
    fatal_err: Option<String>,
    stash: StashModel,
    pager: PagerModel,
    /// The stream of documents the search is producing.
    local_file_finder: Option<Arc<Mutex<Receiver<SearchResult>>>>,
    /// The directory watcher behind the pager's live reload.
    watcher: Arc<Mutex<Option<FileWatcher>>>,
}

/// The pager's filesystem watch.
pub struct FileWatcher {
    /// Held only to keep the watch alive; dropping it stops the events.
    #[allow(dead_code)]
    watcher: notify::RecommendedWatcher,
    events: Receiver<notify::Result<notify::Event>>,
    dir: String,
}

impl Model {
    /// Builds the model for a configuration and, optionally, content that has
    /// already been rendered on the CLI side.
    pub fn new(cfg: Config, content: &str) -> Model {
        let common = CommonModel {
            cfg,
            cwd: String::new(),
            width: 0,
            height: 0,
            styles: Styles::new(true),
        };
        let stash = StashModel::new(&common.styles);

        let mut m = Model {
            state: State::ShowStash,
            fatal_err: None,
            pager: PagerModel::new(),
            stash,
            common,
            local_file_finder: None,
            watcher: Arc::new(Mutex::new(None)),
        };

        let mut path = m.common.cfg.path.clone();
        if path.is_empty() && !content.is_empty() {
            m.state = State::ShowDocument;
            m.pager.current_document = Markdown {
                body: content.to_string(),
                ..Markdown::default()
            };
            return m;
        }

        if path.is_empty() {
            path = ".".to_string();
        }
        let info = match std::fs::metadata(&path) {
            Ok(i) => i,
            Err(e) => {
                let e = format!("open {path}: {}", crate::deps::go_errno(&e));
                log::error(
                    "unable to stat file",
                    &[("file", path), ("error", e.clone())],
                );
                m.fatal_err = Some(e);
                return m;
            }
        };
        if info.is_dir() {
            m.state = State::ShowStash;
        } else {
            let cwd = std::env::current_dir()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            m.state = State::ShowDocument;
            m.pager.current_document = Markdown {
                local_path: path.clone(),
                note: strip_absolute_path(&path, &cwd),
                modtime: info.modified().ok(),
                ..Markdown::default()
            };
        }

        m
    }

    /// Drops the current document and returns to the listing.
    fn unload_document(&mut self) -> Vec<Cmd<Msg>> {
        self.state = State::ShowStash;
        self.stash.view_state = StashViewState::Ready;
        self.pager.unload(&self.common);
        self.pager.show_help = false;
        self.unwatch_file();

        let mut batch = Vec::new();
        if !self.stash.should_spin() {
            batch.push(Cmd::tick(self.stash.spinner.fps(), Msg::SpinnerTick));
        }
        batch
    }

    fn watch_file(&self) -> Cmd<Msg> {
        let dir = self.pager.local_dir();
        let path = self.pager.current_document.local_path.clone();
        let shared = Arc::clone(&self.watcher);
        Cmd::Async(Box::new(move || {
            {
                let mut guard = shared.lock().expect("watcher mutex");
                let needs_new = match guard.as_ref() {
                    Some(w) => w.dir != dir,
                    None => true,
                };
                if needs_new {
                    match new_watcher(&dir) {
                        Ok(w) => *guard = Some(w),
                        Err(e) => {
                            log::error("error adding dir to fsnotify watcher", &[("error", e)]);
                            return None;
                        }
                    }
                }
            }
            log::info("fsnotify watching dir", &[("dir", dir.clone())]);
            loop {
                let event = {
                    let guard = shared.lock().expect("watcher mutex");
                    match guard.as_ref() {
                        Some(w) => w.events.recv_timeout(Duration::from_millis(200)),
                        None => return None,
                    }
                };
                match event {
                    Ok(Ok(e)) => {
                        let relevant = matches!(
                            e.kind,
                            notify::EventKind::Modify(_) | notify::EventKind::Create(_)
                        ) && e.paths.iter().any(|p| p.to_string_lossy() == path);
                        if relevant {
                            return Some(Msg::Reload);
                        }
                    }
                    Ok(Err(_)) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(_) => return None,
                }
            }
        }))
    }

    fn unwatch_file(&self) {
        *self.watcher.lock().expect("watcher mutex") = None;
    }
}

fn new_watcher(dir: &str) -> Result<FileWatcher, String> {
    use notify::Watcher;
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |res| {
        let _ = tx.send(res);
    })
    .map_err(|e| e.to_string())?;
    watcher
        .watch(
            std::path::Path::new(dir),
            notify::RecursiveMode::NonRecursive,
        )
        .map_err(|e| e.to_string())?;
    Ok(FileWatcher {
        watcher,
        events: rx,
        dir: dir.to_string(),
    })
}

impl bubbletea::Model for Model {
    type Msg = Msg;

    fn init(&mut self) -> Cmd<Msg> {
        let mut cmds = vec![Cmd::tick(self.stash.spinner.fps(), Msg::SpinnerTick)];

        match self.state {
            State::ShowStash => cmds.push(find_local_files(&self.common.cfg)),
            State::ShowDocument => {
                let path = self.common.cfg.path.clone();
                match std::fs::read(&path) {
                    Ok(content) => {
                        let body = String::from_utf8_lossy(utils::remove_frontmatter(&content))
                            .into_owned();
                        cmds.push(pager::render_with_glamour(&self.pager, &self.common, &body));
                    }
                    Err(e) => {
                        let e = format!("open {path}: {}", crate::deps::go_errno(&e));
                        log::error(
                            "unable to read file",
                            &[("file", path), ("error", e.clone())],
                        );
                        return Cmd::Async(Box::new(move || Some(Msg::Err(e))));
                    }
                }
            }
        }

        Cmd::batch(cmds)
    }

    fn update(&mut self, msg: Msg) -> Cmd<Msg> {
        // With a fatal error showing, any key exits.
        if self.fatal_err.is_some() {
            if let Msg::Key(_) = msg {
                return Cmd::Quit;
            }
        }

        let mut cmds: Vec<Cmd<Msg>> = Vec::new();

        match &msg {
            Msg::BackgroundColor(is_dark) => {
                self.common.styles = Styles::new(*is_dark);
                let styles = self.common.styles.clone();
                self.stash.style_paginators(&styles);
            }
            Msg::Key(key) => match key.as_str() {
                "esc" => {
                    if self.state == State::ShowDocument
                        || self.stash.view_state == StashViewState::LoadingDocument
                    {
                        let batch = self.unload_document();
                        return Cmd::batch(batch);
                    }
                }
                "r" => {
                    if self.state == State::ShowStash {
                        // Everything is passed through while the filter is
                        // being typed.
                        if self.stash.filter_state == stash::FilterState::Filtering {
                            return self.stash.update(&mut self.common, &msg);
                        }
                        self.stash.markdowns.clear();
                        self.stash.filtered_markdowns.clear();
                        return bubbletea::Model::init(self);
                    }
                }
                "q" => {
                    if self.state == State::ShowStash
                        && self.stash.filter_state == stash::FilterState::Filtering
                    {
                        return self.stash.update(&mut self.common, &msg);
                    }
                    return Cmd::Quit;
                }
                "left" | "h" | "delete" => {
                    if self.state == State::ShowDocument {
                        cmds.extend(self.unload_document());
                        return Cmd::batch(cmds);
                    }
                }
                "ctrl+z" => return Cmd::Suspend,
                // Ctrl+C always quits, wherever you are.
                "ctrl+c" => return Cmd::Quit,
                _ => {}
            },

            // The window size arrives at start-up and on every resize.
            Msg::WindowSize { width, height } => {
                self.common.width = *width;
                self.common.height = *height;
                let (w, h) = (*width, *height);
                self.stash.set_size(&mut self.common, w, h);
                self.pager.set_size(&self.common, w, h);
            }

            Msg::InitLocalFileSearch { cwd, rx } => {
                self.local_file_finder = Some(Arc::clone(rx));
                self.common.cwd = cwd.clone();
                cmds.push(find_next_local_file(self.local_file_finder.clone()));
            }

            Msg::FetchedMarkdown(md) => {
                self.pager.current_document = (**md).clone();
                let body = String::from_utf8_lossy(utils::remove_frontmatter(md.body.as_bytes()))
                    .into_owned();
                cmds.push(pager::render_with_glamour(&self.pager, &self.common, &body));
            }

            Msg::ContentRendered(_) => self.state = State::ShowDocument,

            Msg::LocalFileSearchFinished => {
                // The listing is kept up to date even when it is not on screen.
                return self.stash.update(&mut self.common, &msg);
            }

            Msg::FoundLocalFile(res) => {
                let mut new_md = local_file_to_markdown(&self.common.cwd, res);
                if self.stash.filter_applied() {
                    new_md.build_filter_value();
                }
                self.stash.add_markdowns(&self.common, vec![new_md]);
                if self.stash.should_update_filter() {
                    cmds.push(stash::filter_markdowns(&self.stash));
                }
                cmds.push(find_next_local_file(self.local_file_finder.clone()));
            }

            Msg::FilteredMarkdown(_) => {
                if self.state == State::ShowDocument {
                    cmds.push(self.stash.update(&mut self.common, &msg));
                }
            }

            Msg::WatchFile => return self.watch_file(),

            _ => {}
        }

        // Process children.
        match self.state {
            State::ShowStash => cmds.push(self.stash.update(&mut self.common, &msg)),
            State::ShowDocument => {
                let cmd = self.pager.update(&self.common, &msg);
                cmds.push(cmd);
            }
        }

        Cmd::batch(cmds)
    }

    fn view(&self) -> View {
        let content = if let Some(err) = &self.fatal_err {
            error_view(&self.common.styles, err, true)
        } else if self.state == State::ShowDocument {
            self.pager.view(&self.common)
        } else {
            self.stash.view(&self.common)
        };

        View {
            content,
            alt_screen: true,
            mouse: self.common.cfg.enable_mouse,
        }
    }

    fn on_event(&self, event: Event) -> Option<Msg> {
        Some(match event {
            Event::Key(name) => Msg::Key(name),
            Event::Resize(w, h) => Msg::WindowSize {
                width: w as usize,
                height: h as usize,
            },
            Event::BackgroundColor(is_dark) => Msg::BackgroundColor(is_dark),
        })
    }
}

/// The error screen.
pub fn error_view(styles: &Styles, err: &str, fatal: bool) -> String {
    let exit_msg = if fatal {
        "press any key to exit"
    } else {
        "press any key to return"
    };
    let s = format!(
        "{}\n\n{err}\n\n{}",
        styles.error_title_style.render("ERROR"),
        styles.subtle_style.render(exit_msg)
    );
    format!("\n{}", indent(&s, 3))
}

/// Starts the document search.
pub fn find_local_files(cfg: &Config) -> Cmd<Msg> {
    let cfg = cfg.clone();
    Cmd::Async(Box::new(move || {
        log::info("findLocalFiles", &[]);
        let cwd = if cfg.path.is_empty() {
            match std::env::current_dir() {
                Ok(p) => p.to_string_lossy().into_owned(),
                Err(e) => return Some(Msg::Err(e.to_string())),
            }
        } else {
            match std::fs::metadata(&cfg.path) {
                Ok(info) if info.is_dir() => match crate::glow::absolute_path(&cfg.path) {
                    Ok(p) => p,
                    Err(e) => return Some(Msg::Err(e)),
                },
                Ok(_) => cfg.path.clone(),
                Err(e) => {
                    return Some(Msg::Err(format!(
                        "open {}: {}",
                        cfg.path,
                        crate::deps::go_errno(&e)
                    )))
                }
            }
        };

        log::debug("local directory is", &[("cwd", cwd.clone())]);

        let list: Vec<String> = MARKDOWN_EXTENSIONS.iter().map(|s| s.to_string()).collect();
        let result = if cfg.show_all_files {
            gitcha::find_files(&cwd, &list, &[], false)
        } else {
            gitcha::find_files(&cwd, &list, &ignore::ignore_patterns(&cfg), true)
        };

        match result {
            Ok(rx) => Some(Msg::InitLocalFileSearch {
                cwd,
                rx: Arc::new(Mutex::new(rx)),
            }),
            Err(e) => {
                log::error("error finding local files", &[("error", e.clone())]);
                Some(Msg::Err(e))
            }
        }
    }))
}

/// Waits for the next document the search finds.
fn find_next_local_file(finder: Option<Arc<Mutex<Receiver<SearchResult>>>>) -> Cmd<Msg> {
    Cmd::Async(Box::new(move || {
        let finder = finder?;
        let guard = finder.lock().expect("finder mutex");
        match guard.recv() {
            Ok(res) => Some(Msg::FoundLocalFile(res)),
            Err(_) => {
                log::debug("local file search finished", &[]);
                Some(Msg::LocalFileSearchFinished)
            }
        }
    }))
}

/// Turns a search result into a document.
pub fn local_file_to_markdown(cwd: &str, res: &SearchResult) -> Markdown {
    Markdown {
        local_path: res.path.clone(),
        note: strip_absolute_path(&res.path, cwd),
        modtime: Some(res.modified),
        ..Markdown::default()
    }
}

/// Drops the working directory prefix from a path.
pub fn strip_absolute_path(full_path: &str, cwd: &str) -> String {
    let fp = std::fs::canonicalize(full_path)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| full_path.to_string());
    let cp = std::fs::canonicalize(cwd)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| cwd.to_string());
    fp.replace(&format!("{cp}/"), "")
}

/// A lightweight version of reflow's indent.
pub fn indent(s: &str, n: usize) -> String {
    if n == 0 || s.is_empty() {
        return s.to_string();
    }
    let i = " ".repeat(n);
    s.split('\n')
        .map(|v| format!("{i}{v}\n"))
        .collect::<String>()
}

/// A TUI program that has not started yet.
pub struct Program {
    model: Model,
}

impl Program {
    /// Runs the program to completion.
    pub fn run(self) -> Result<(), String> {
        bubbletea::run(self.model).map(|_| ())
    }
}

/// Creates the TUI program.
pub fn new_program(cfg: Config, content: &str) -> Program {
    log::debug(
        "Starting glow",
        &[("glamour", cfg.glamour_enabled.to_string())],
    );
    Program {
        model: Model::new(cfg, content),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indent_adds_a_trailing_newline_to_every_line() {
        assert_eq!(indent("a\nb", 2), "  a\n  b\n");
        assert_eq!(indent("a", 0), "a");
        assert_eq!(indent("", 3), "");
    }

    #[test]
    fn the_error_view_names_the_way_out() {
        let styles = Styles::new(true);
        let fatal = error_view(&styles, "boom", true);
        assert!(fatal.contains("ERROR"));
        assert!(fatal.contains("boom"));
        assert!(fatal.contains("press any key to exit"));
        let recoverable = error_view(&styles, "boom", false);
        assert!(recoverable.contains("press any key to return"));
    }

    #[test]
    fn a_path_under_the_working_directory_becomes_relative() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let root = std::fs::canonicalize(dir.path()).expect("canonical");
        std::fs::create_dir(root.join("sub")).expect("mkdir");
        let file = root.join("sub/a.md");
        std::fs::write(&file, "x").expect("write");
        assert_eq!(
            strip_absolute_path(&file.to_string_lossy(), &root.to_string_lossy()),
            "sub/a.md"
        );
    }

    #[test]
    fn a_missing_path_is_a_fatal_error() {
        let cfg = Config {
            path: "/nonexistent/glow-test".into(),
            ..Config::default()
        };
        let m = Model::new(cfg, "");
        assert_eq!(
            m.fatal_err.as_deref(),
            Some("open /nonexistent/glow-test: no such file or directory")
        );
    }

    #[test]
    fn content_without_a_path_opens_the_pager() {
        let m = Model::new(Config::default(), "# hi\n");
        assert_eq!(m.state, State::ShowDocument);
        assert_eq!(m.pager.current_document.body, "# hi\n");
    }

    #[test]
    fn a_directory_opens_the_listing() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let cfg = Config {
            path: dir.path().to_string_lossy().into_owned(),
            ..Config::default()
        };
        let m = Model::new(cfg, "");
        assert_eq!(m.state, State::ShowStash);
    }

    #[test]
    fn a_file_opens_the_pager_with_its_note() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let path = dir.path().join("doc.md");
        std::fs::write(&path, "# hi\n").expect("write");
        let cfg = Config {
            path: path.to_string_lossy().into_owned(),
            ..Config::default()
        };
        let m = Model::new(cfg, "");
        assert_eq!(m.state, State::ShowDocument);
        assert!(m.pager.current_document.note.ends_with("doc.md"));
        assert!(m.pager.current_document.modtime.is_some());
    }
}
