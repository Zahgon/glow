//! The root command: option resolution, source resolution and the CLI pipeline.

use std::io::{Read, Write};
use std::path::Path;

use crate::config_cmd;
use crate::deps::cobra::{App, Args, Command, Flag, FlagSet, Outcome};
use crate::deps::glamour::{self, render};
use crate::deps::term;
use crate::deps::url::Url;
use crate::deps::viper::{self, Viper};
use crate::deps::{cobra, go_errno, go_exec_error, shell};
use crate::log;
use crate::man_cmd;
use crate::source::Source;
use crate::style::{keyword, paragraph};
use crate::ui;
use crate::url::{is_url, readme_url};
use crate::utils;

/// Version, as provided by the release pipeline.
const VERSION: Option<&str> = option_env!("GLOW_VERSION");
/// Commit SHA, as provided by the release pipeline.
const COMMIT_SHA: Option<&str> = option_env!("GLOW_COMMIT_SHA");

/// The file names, in search order, that count as a directory's README.
const README_NAMES: [&str; 6] = [
    "README.md",
    "README",
    "Readme.md",
    "Readme",
    "readme.md",
    "readme",
];

/// Glow's process-wide state: the command tree, the configuration registry and
/// the option values the commands read.
///
/// The Go original keeps these in package-level variables that `pflag` writes
/// into directly; keeping them together in one value preserves that observable
/// behaviour — parsing flags mutates the state — without making the state
/// global, so tests cannot interfere with one another.
pub struct Glow {
    /// The command tree.
    pub app: App,
    /// The configuration registry.
    pub viper: Viper,
    /// Path of the configuration file, from `--config` or the search path.
    pub config_file: String,
    /// Whether to display through a pager.
    pub pager: bool,
    /// Whether to display through the TUI.
    pub tui: bool,
    /// Style name or JSON path.
    pub style: String,
    /// Word-wrap column.
    pub width: u64,
    /// Whether the TUI lists system files.
    pub show_all_files: bool,
    /// Whether the pager numbers lines.
    pub show_line_numbers: bool,
    /// Whether soft line breaks survive rendering.
    pub preserve_new_lines: bool,
    /// Whether the TUI tracks the mouse wheel.
    pub mouse: bool,
}

impl Glow {
    /// Builds the command tree and loads configuration, as the original's
    /// `init` does.
    pub fn new() -> Glow {
        let mut viper = Viper::new();
        let (config_file, dirs) = try_load_config_from_default_places(&mut viper);

        let version = match VERSION {
            Some(v) if !v.is_empty() => v.to_string(),
            _ => "unknown (built from source)".to_string(),
        };
        let mut version_template = "glow version {{.Version}}\n".to_string();
        if let Some(sha) = COMMIT_SHA {
            if sha.len() >= 7 {
                version_template = format!("glow version {{{{.Version}}}} ({})\n", &sha[0..7]);
            }
        }

        let mut root = Command::new(
            "root",
            "glow [SOURCE|DIR]",
            "Render markdown on the CLI, with pizzazz!",
        );
        root.long = paragraph(&format!(
            "\nRender markdown on the CLI, {}!",
            keyword("with pizzazz")
        ));
        root.args = Args::MaximumN(1);

        // "Glow Classic" cli arguments. The order they are declared in does not
        // reach the user — pflag prints them sorted — but the config file's
        // default note does, and it is resolved here, before the file that the
        // search may be about to create exists.
        root.persistent_flags.add(Flag::string(
            "config",
            "",
            "",
            &format!("config file (default {})", viper.config_file_used()),
        ));
        root.flags
            .add(Flag::bool("pager", "p", false, "display with pager"));
        root.flags
            .add(Flag::bool("tui", "t", false, "display with tui"));
        root.flags.add(Flag::string(
            "style",
            "s",
            "auto",
            "style name or JSON path",
        ));
        root.flags.add(Flag::uint(
            "width",
            "w",
            0,
            "word-wrap at width (set to 0 to disable)",
        ));
        root.flags.add(Flag::bool(
            "all",
            "a",
            false,
            "show system files and directories (TUI-mode only)",
        ));
        root.flags.add(Flag::bool(
            "line-numbers",
            "l",
            false,
            "show line numbers (TUI-mode only)",
        ));
        root.flags.add(Flag::bool(
            "preserve-new-lines",
            "n",
            false,
            "preserve newlines in the output",
        ));
        root.flags
            .add(Flag::bool("mouse", "m", false, "enable mouse wheel (TUI-mode only)").hidden());
        root.flags
            .add(Flag::bool("help", "h", false, "help for glow"));
        root.flags
            .add(Flag::bool("version", "v", false, "version for glow"));

        // Sub-commands, in the order cobra sorts them for display.
        root.children.push(completion_command());
        root.children.push(config_cmd::command());
        root.children.push(help_command());
        root.children.push(man_cmd::command());

        // Config bindings.
        for (key, flag) in [
            ("pager", "pager"),
            ("tui", "tui"),
            ("style", "style"),
            ("width", "width"),
            ("mouse", "mouse"),
            ("preserveNewLines", "preserve-new-lines"),
            ("showLineNumbers", "line-numbers"),
            ("all", "all"),
        ] {
            viper.bind_pflag(key, flag);
        }

        viper.set_default("style", viper::Value::Str("auto".into()));
        viper.set_default("width", viper::Value::Int(0));
        viper.set_default("all", viper::Value::Bool(true));

        let mut glow = Glow {
            app: App {
                root,
                version,
                version_template,
            },
            viper,
            config_file,
            pager: false,
            tui: false,
            style: String::new(),
            width: 0,
            show_all_files: false,
            show_line_numbers: false,
            preserve_new_lines: false,
            mouse: false,
        };

        // The original writes a default configuration file when the search
        // found none. `dirs` is empty only when the platform reports no
        // configuration directory at all.
        if glow.viper.config_file_used().is_empty() && !dirs.is_empty() {
            if let Err(e) = config_cmd::ensure_config_file(&mut glow.config_file, "") {
                log::error("Could not create default configuration", &[("error", e)]);
            }
        }

        glow
    }

    /// pflag's `ParseFlags`: parses `argv` against the root command's flags and
    /// copies the results into the option fields, the way `BoolVarP` and
    /// friends write straight into their variables.
    pub fn parse_flags(&mut self, argv: &[String]) -> Result<FlagSet, cobra::Error> {
        let flags = self.app.parse_root_flags(argv)?;
        self.config_file = flags.string("config");
        self.pager = flags.bool("pager");
        self.tui = flags.bool("tui");
        self.style = flags.string("style");
        self.width = flags.uint("width");
        self.show_all_files = flags.bool("all");
        self.show_line_numbers = flags.bool("line-numbers");
        self.preserve_new_lines = flags.bool("preserve-new-lines");
        self.mouse = flags.bool("mouse");
        Ok(flags)
    }

    /// Checks that a style is built in, or names a file that exists.
    pub fn validate_style(&self, style: &str) -> Result<(), String> {
        if style != "auto" && glamour::styles::default_style(style).is_none() {
            let style = utils::expand_path(style);
            match std::fs::metadata(&style) {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    return Err(format!("specified style does not exist: {style}"))
                }
                Err(e) => return Err(format!("unable to stat file: {e}")),
            }
        }
        Ok(())
    }

    /// Resolves every option from the configuration registry, then validates
    /// the combination — the original's `PersistentPreRunE`.
    pub fn validate_options(&mut self, flags: &FlagSet) -> Result<(), String> {
        self.width = self.viper.get_uint("width", flags);
        self.mouse = self.viper.get_bool("mouse", flags);
        self.pager = self.viper.get_bool("pager", flags);
        self.tui = self.viper.get_bool("tui", flags);
        self.show_all_files = self.viper.get_bool("all", flags);
        self.preserve_new_lines = self.viper.get_bool("preserveNewLines", flags);
        self.show_line_numbers = self.viper.get_bool("showLineNumbers", flags);

        if self.pager && self.tui {
            return Err("cannot use both pager and tui".into());
        }

        self.style = self.viper.get_string("style", flags);
        self.validate_style(&self.style.clone())?;

        let is_terminal = term::stdout_is_terminal();
        // A no-TTY style is used when stdout is not a terminal and no style was
        // asked for explicitly.
        if !is_terminal && !flags.changed("style") {
            self.style = glamour::styles::NOTTY_STYLE.to_string();
        }

        if !flags.changed("width") {
            if is_terminal && self.width == 0 {
                if let Some((w, _)) = term::size() {
                    self.width = u64::from(w);
                }
                if self.width > 120 {
                    self.width = 120;
                }
            }
            if self.width == 0 {
                self.width = 80;
            }
        }
        Ok(())
    }

    /// Parses an argument and creates a readable source for it.
    pub fn source_from_arg(&self, arg: &str) -> Result<Source, String> {
        // From stdin.
        if arg == "-" {
            return Ok(Source::from_reader(Box::new(std::io::stdin())));
        }

        // A GitHub or GitLab URL, even without the protocol. A failure here is
        // not fatal: the remaining strategies still get their turn.
        if let Ok(Some(src)) = readme_url(arg) {
            return Ok(src);
        }

        // HTTP(S) URLs.
        if arg.contains("://") {
            if let Ok(u) = Url::parse_request_uri(arg) {
                if !u.scheme.is_empty() {
                    if u.scheme != "http" && u.scheme != "https" {
                        return Err(format!("{} is not a supported protocol", u.scheme));
                    }
                    let resp = crate::http::get(&u.to_string())
                        .map_err(|e| format!("unable to get url: {e}"))?;
                    if resp.status != 200 {
                        return Err(format!("HTTP status {}", resp.status));
                    }
                    return Ok(Source {
                        reader: resp.into_reader(),
                        url: u.to_string(),
                    });
                }
            }
        }

        // A directory: walk it for a README.
        let arg = if arg.is_empty() { "." } else { arg };
        if Path::new(arg).is_dir() {
            if let Some(src) = find_readme(arg) {
                return Ok(src);
            }
            return Err("missing markdown source".into());
        }

        let file = std::fs::File::open(arg)
            .map_err(|e| format!("unable to open file: open {arg}: {}", go_errno(&e)))?;
        let abs = absolute_path(arg).map_err(|e| format!("unable to get absolute path: {e}"))?;
        Ok(Source {
            reader: Box::new(file),
            url: abs,
        })
    }

    /// The original's `RunE`: decide between stdin, the TUI and the CLI.
    pub fn execute(&mut self, flags: &FlagSet, args: &[String]) -> Result<(), String> {
        // A piped stdin always wins, the way an explicit `-` does.
        if term::stdin_is_pipe()? {
            let src = Source::from_reader(Box::new(std::io::stdin()));
            return self.execute_cli(flags, src, &mut std::io::stdout());
        }

        match args.len() {
            // TUI running on the working directory.
            0 => self.run_tui("", ""),
            // A directory argument opens the TUI on it; anything else falls
            // through to the CLI.
            1 if Path::new(&args[0]).is_dir() => match absolute_path(&args[0]) {
                Ok(p) => self.run_tui(&p, ""),
                Err(_) => self.execute_args(flags, args),
            },
            _ => self.execute_args(flags, args),
        }
    }

    fn execute_args(&mut self, flags: &FlagSet, args: &[String]) -> Result<(), String> {
        for arg in args {
            let src = self.source_from_arg(arg)?;
            self.execute_cli(flags, src, &mut std::io::stdout())?;
        }
        Ok(())
    }

    /// Renders a source and sends it to the pager, the TUI or the writer.
    pub fn execute_cli(
        &mut self,
        flags: &FlagSet,
        mut src: Source,
        w: &mut dyn Write,
    ) -> Result<(), String> {
        let mut buf = Vec::new();
        src.reader
            .read_to_end(&mut buf)
            .map_err(|e| format!("unable to read from reader: {e}"))?;
        let body = utils::remove_frontmatter(&buf).to_vec();

        let mut base_url = String::new();
        if let Ok(mut u) = Url::parse_request_uri(&src.url) {
            u.path = crate::deps::url::dir(&u.path);
            base_url = format!("{u}/");
        }

        let is_code = !utils::is_markdown_file(&src.url);

        let styles = glamour::resolve_style(&utils::glamour_style(&self.style, is_code))
            .map_err(|e| format!("unable to create renderer: {e}"))?;
        let renderer = render::Renderer::new(render::Options {
            base_url,
            word_wrap: self.width as i64,
            preserve_new_lines: true,
            styles,
            ..Default::default()
        });

        let mut content = String::from_utf8_lossy(&body).into_owned();
        let ext = utils::extension(&src.url);
        if is_code {
            content = utils::wrap_code_block(&content, &ext);
        }

        let out = renderer.render(&content);

        if self.pager || flags.changed("pager") {
            return run_pager(&out);
        }
        if self.tui || flags.changed("tui") {
            let path = if is_url(&src.url) { "" } else { &src.url };
            return self.run_tui(path, &content);
        }
        w.write_all(out.as_bytes())
            .map_err(|e| format!("unable to write to writer: {e}"))
    }

    /// Starts the Bubble Tea program.
    pub fn run_tui(&self, path: &str, content: &str) -> Result<(), String> {
        let mut cfg = ui::Config::from_env().map_err(|e| format!("error parsing config: {e}"))?;

        // Use the style set in the environment, or the resolved one if it is
        // not usable.
        if self.validate_style(&cfg.glamour_style).is_err() {
            cfg.glamour_style = self.style.clone();
        }

        cfg.path = path.to_string();
        cfg.show_all_files = self.show_all_files;
        cfg.show_line_numbers = self.show_line_numbers;
        cfg.glamour_max_width = self.width;
        cfg.enable_mouse = self.mouse;
        cfg.preserve_new_lines = self.preserve_new_lines;

        ui::new_program(cfg, content)
            .run()
            .map(|_| ())
            .map_err(|e| format!("unable to run tui program: {e}"))
    }

    /// Runs one command line to completion, returning the process exit code.
    pub fn run(&mut self, argv: &[String]) -> i32 {
        let parsed = match self.app.parse(argv) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Error: {e}");
                return 1;
            }
        };
        // pflag writes straight into the option variables, so the resolved flag
        // set is mirrored into them before anything runs.
        self.config_file = parsed.flags.string("config");
        self.pager = parsed.flags.bool("pager");
        self.tui = parsed.flags.bool("tui");
        self.style = parsed.flags.string("style");
        self.width = parsed.flags.uint("width");
        self.show_all_files = parsed.flags.bool("all");
        self.show_line_numbers = parsed.flags.bool("line-numbers");
        self.preserve_new_lines = parsed.flags.bool("preserve-new-lines");
        self.mouse = parsed.flags.bool("mouse");

        let (id, args) = match parsed.outcome {
            Outcome::Print(text) => {
                print!("{text}");
                return 0;
            }
            Outcome::Run { id, args } => (id, args),
        };

        // `PersistentPreRunE` runs for every command in the tree.
        if let Err(e) = self.validate_options(&parsed.flags) {
            eprintln!("Error: {e}");
            return 1;
        }

        let result = match id {
            "root" => self.execute(&parsed.flags, &args),
            "config" => {
                let used = self.viper.config_file_used().to_string();
                config_cmd::run(&mut self.config_file, &used)
            }
            "man" => man_cmd::run(&self.app),
            "completion-bash" => print_completion("bash"),
            "completion-zsh" => print_completion("zsh"),
            "completion-fish" => print_completion("fish"),
            "completion-powershell" => print_completion("powershell"),
            other => Err(format!("unknown command \"{other}\" for \"glow\"")),
        };

        match result {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("Error: {e}");
                1
            }
        }
    }
}

impl Default for Glow {
    fn default() -> Self {
        Glow::new()
    }
}

/// The process entry point: set logging up, run, and report the exit code.
pub fn main() -> i32 {
    term::enable_ansi_colors();
    let mut glow = Glow::new();
    if let Err(e) = log::setup() {
        println!("{e}");
        return 1;
    }
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let code = glow.run(&argv);
    log::close();
    code
}

fn print_completion(shell: &str) -> Result<(), String> {
    let script =
        cobra::completion_script(shell).ok_or_else(|| format!("unknown shell: {shell}"))?;
    print!("{script}");
    Ok(())
}

/// Sends rendered output through `$PAGER`, defaulting to `less -r`.
fn run_pager(out: &str) -> Result<(), String> {
    let pager_cmd = match std::env::var("PAGER") {
        Ok(v) if !v.is_empty() => v,
        _ => "less -r".to_string(),
    };
    let fields = shell::fields(&pager_cmd, |k| std::env::var(k).ok()).unwrap_or_default();
    if fields.is_empty() {
        return Err(format!("unable to parse PAGER command: {pager_cmd}"));
    }

    let mut child = std::process::Command::new(&fields[0])
        .args(&fields[1..])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("unable to run command: {}", go_exec_error(&fields[0], &e)))?;
    if let Some(stdin) = child.stdin.as_mut() {
        // A pager that exits before reading everything closes the pipe; that is
        // not a failure of the command itself.
        let _ = stdin.write_all(out.as_bytes());
    }
    drop(child.stdin.take());
    let status = child
        .wait()
        .map_err(|e| format!("unable to run command: {e}"))?;
    if !status.success() {
        return Err(format!(
            "unable to run command: exit status {}",
            status.code().unwrap_or(1)
        ));
    }
    Ok(())
}

/// Walks `dir` depth-first for the first file whose name is a README.
fn find_readme(dir: &str) -> Option<Source> {
    let mut stack = vec![std::path::PathBuf::from(dir)];
    while let Some(path) = stack.pop() {
        if path.is_dir() {
            let mut entries: Vec<_> = std::fs::read_dir(&path)
                .ok()?
                .filter_map(Result::ok)
                .map(|e| e.path())
                .collect();
            // `filepath.Walk` visits lexically; the stack reverses the order.
            entries.sort();
            entries.reverse();
            stack.extend(entries);
            continue;
        }
        let base = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if README_NAMES.iter().any(|v| v.eq_ignore_ascii_case(&base)) {
            if let Ok(file) = std::fs::File::open(&path) {
                let url = absolute_path(&path.to_string_lossy()).unwrap_or_default();
                return Some(Source {
                    reader: Box::new(file),
                    url,
                });
            }
        }
    }
    None
}

/// `filepath.Abs`: join with the working directory and clean the result.
pub fn absolute_path(path: &str) -> Result<String, String> {
    let p = Path::new(path);
    let joined = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().map_err(|e| e.to_string())?.join(p)
    };
    Ok(clean(&joined.to_string_lossy()))
}

/// `path.Clean`: collapse `.`, `..` and repeated separators.
pub fn clean(path: &str) -> String {
    let rooted = path.starts_with('/');
    let mut out: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if out.last().is_some_and(|l| *l != "..") {
                    out.pop();
                } else if !rooted {
                    out.push("..");
                }
            }
            other => out.push(other),
        }
    }
    let joined = out.join("/");
    if rooted {
        format!("/{joined}")
    } else if joined.is_empty() {
        ".".to_string()
    } else {
        joined
    }
}

/// Loads configuration from the platform's search path.
///
/// Returns the configuration path the original would fall back to, and the
/// directories that were searched.
fn try_load_config_from_default_places(v: &mut Viper) -> (String, Vec<String>) {
    let mut dirs = crate::deps::gap::Scope::user("glow").config_dirs();

    if let Ok(c) = std::env::var("XDG_CONFIG_HOME") {
        if !c.is_empty() {
            dirs.insert(0, format!("{}/glow", c.trim_end_matches('/')));
        }
    }
    if let Ok(c) = std::env::var("GLOW_CONFIG_HOME") {
        if !c.is_empty() {
            dirs.insert(0, c);
        }
    }

    for d in &dirs {
        v.add_config_path(d);
    }

    v.set_config_name("glow");
    v.set_config_type("yaml");
    v.set_env_prefix("glow");
    v.automatic_env();

    if let Err(e) = v.read_in_config() {
        if !matches!(e, viper::ReadError::NotFound) {
            log::warn(
                "Could not parse configuration file",
                &[("err", e.to_string())],
            );
        }
    }

    let used = v.config_file_used().to_string();
    if !used.is_empty() {
        log::debug("Using configuration file", &[("path", used)]);
        return (String::new(), dirs);
    }

    let fallback = dirs
        .first()
        .map(|d| format!("{}/glow.yml", d.trim_end_matches('/')))
        .unwrap_or_default();
    (fallback, dirs)
}

/// Cobra's generated `completion` command tree.
fn completion_command() -> Command {
    let mut completion = Command::new(
        "completion",
        "completion",
        "Generate the autocompletion script for the specified shell",
    );
    completion.long = "Generate the autocompletion script for glow for the specified shell.\nSee each sub-command's help for details on how to use the generated script.\n".into();
    completion.runnable = false;
    completion
        .flags
        .add(Flag::bool("help", "h", false, "help for completion"));

    let mut bash = Command::new(
        "completion-bash",
        "bash",
        "Generate the autocompletion script for bash",
    );
    bash.long = "Generate the autocompletion script for the bash shell.\n\nThis script depends on the 'bash-completion' package.\nIf it is not installed already, you can install it via your OS's package manager.\n\nTo load completions in your current shell session:\n\n\tsource <(glow completion bash)\n\nTo load completions for every new session, execute once:\n\n#### Linux:\n\n\tglow completion bash > /etc/bash_completion.d/glow\n\n#### macOS:\n\n\tglow completion bash > $(brew --prefix)/etc/bash_completion.d/glow\n\nYou will need to start a new shell for this setup to take effect.\n".into();
    bash.args = Args::NoArgs;
    bash.disable_flags_in_use_line = true;

    let mut zsh = Command::new(
        "completion-zsh",
        "zsh",
        "Generate the autocompletion script for zsh",
    );
    zsh.long = "Generate the autocompletion script for the zsh shell.\n\nIf shell completion is not already enabled in your environment you will need\nto enable it.  You can execute the following once:\n\n\techo \"autoload -U compinit; compinit\" >> ~/.zshrc\n\nTo load completions in your current shell session:\n\n\tsource <(glow completion zsh)\n\nTo load completions for every new session, execute once:\n\n#### Linux:\n\n\tglow completion zsh > \"${fpath[1]}/_glow\"\n\n#### macOS:\n\n\tglow completion zsh > $(brew --prefix)/share/zsh/site-functions/_glow\n\nYou will need to start a new shell for this setup to take effect.\n".into();
    zsh.args = Args::NoArgs;

    let mut fish = Command::new(
        "completion-fish",
        "fish",
        "Generate the autocompletion script for fish",
    );
    fish.long = "Generate the autocompletion script for the fish shell.\n\nTo load completions in your current shell session:\n\n\tglow completion fish | source\n\nTo load completions for every new session, execute once:\n\n\tglow completion fish > ~/.config/fish/completions/glow.fish\n\nYou will need to start a new shell for this setup to take effect.\n".into();
    fish.args = Args::NoArgs;

    let mut powershell = Command::new(
        "completion-powershell",
        "powershell",
        "Generate the autocompletion script for powershell",
    );
    powershell.long = "Generate the autocompletion script for powershell.\n\nTo load completions in your current shell session:\n\n\tglow completion powershell | Out-String | Invoke-Expression\n\nTo load completions for every new session, add the output of the above command\nto your powershell profile.\n".into();
    powershell.args = Args::NoArgs;

    for (cmd, name) in [
        (&mut bash, "bash"),
        (&mut zsh, "zsh"),
        (&mut fish, "fish"),
        (&mut powershell, "powershell"),
    ] {
        cmd.flags
            .add(Flag::bool("help", "h", false, &format!("help for {name}")));
        cmd.flags.add(Flag::bool(
            "no-descriptions",
            "",
            false,
            "disable completion descriptions",
        ));
    }

    completion.children = vec![bash, fish, powershell, zsh];
    completion
}

/// Cobra's generated `help` command.
fn help_command() -> Command {
    let mut help = Command::new("help", "help [command]", "Help about any command");
    help.long = "Help provides help for any command in the application.\nSimply type glow help [path to command] for full details.".into();
    help.flags
        .add(Flag::bool("help", "h", false, "help for help"));
    help
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// A Glow that never touches the user's real configuration.
    fn isolated() -> (tempfile::TempDir, Glow) {
        let dir = tempfile::tempdir().expect("a temp dir");
        std::env::set_var("GLOW_CONFIG_HOME", dir.path());
        let glow = Glow::new();
        std::env::remove_var("GLOW_CONFIG_HOME");
        (dir, glow)
    }

    #[test]
    fn clean_collapses_dot_segments() {
        assert_eq!(clean("/a/./b/../c"), "/a/c");
        assert_eq!(clean("/a//b/"), "/a/b");
        assert_eq!(clean("a/../.."), "..");
        assert_eq!(clean("/.."), "/");
    }

    #[test]
    fn go_errno_drops_the_os_error_suffix() {
        let e = std::io::Error::from(std::io::ErrorKind::NotFound);
        assert_eq!(go_errno(&e), "no such file or directory");
    }

    #[test]
    fn an_unsupported_protocol_is_rejected() {
        let (_dir, glow) = isolated();
        assert_eq!(
            glow.source_from_arg("ftp://example.com/x.md").unwrap_err(),
            "ftp is not a supported protocol"
        );
    }

    #[test]
    fn a_missing_file_reports_gos_open_error() {
        let (_dir, glow) = isolated();
        assert_eq!(
            glow.source_from_arg("/nonexistent/file.md").unwrap_err(),
            "unable to open file: open /nonexistent/file.md: no such file or directory"
        );
    }

    #[test]
    fn a_directory_without_a_readme_has_no_source() {
        let (_dir, glow) = isolated();
        let empty = tempfile::tempdir().expect("a temp dir");
        assert_eq!(
            glow.source_from_arg(&empty.path().to_string_lossy())
                .unwrap_err(),
            "missing markdown source"
        );
    }

    #[test]
    fn a_directory_resolves_to_its_readme_case_insensitively() {
        let (_dir, glow) = isolated();
        let d = tempfile::tempdir().expect("a temp dir");
        std::fs::write(d.path().join("Readme.md"), "# hi\n").expect("write");
        let src = glow
            .source_from_arg(&d.path().to_string_lossy())
            .expect("a source");
        assert!(src.url.ends_with("/Readme.md"), "{}", src.url);
    }

    #[test]
    fn a_built_in_style_needs_no_file() {
        let (_dir, glow) = isolated();
        for name in [
            "auto",
            "dark",
            "light",
            "notty",
            "pink",
            "dracula",
            "tokyo-night",
            "ascii",
        ] {
            assert!(glow.validate_style(name).is_ok(), "{name}");
        }
        assert_eq!(
            glow.validate_style("nosuchstyle").unwrap_err(),
            "specified style does not exist: nosuchstyle"
        );
    }

    #[test]
    fn pager_and_tui_together_are_rejected() {
        let (_dir, mut glow) = isolated();
        let flags = glow.parse_flags(&argv(&["-p", "-t"])).expect("parses");
        assert_eq!(
            glow.validate_options(&flags).unwrap_err(),
            "cannot use both pager and tui"
        );
    }

    /// The original's `TestGlowFlags`: parsing writes straight into the option
    /// state, and the assertions run in order against one command tree.
    #[test]
    fn parsing_flags_updates_the_option_state() {
        let (_dir, mut glow) = isolated();
        glow.parse_flags(&argv(&["-p"])).expect("parses");
        assert!(glow.pager);
        glow.parse_flags(&argv(&["-s", "light"])).expect("parses");
        assert_eq!(glow.style, "light");
        glow.parse_flags(&argv(&["-w", "40"])).expect("parses");
        assert_eq!(glow.width, 40);
    }

    #[test]
    fn rendering_a_markdown_file_writes_to_the_given_writer() {
        let (_dir, mut glow) = isolated();
        let d = tempfile::tempdir().expect("a temp dir");
        let path = d.path().join("doc.md");
        std::fs::write(&path, "# Title\n").expect("write");

        let flags = glow
            .parse_flags(&argv(&["-s", "notty", "-w", "80"]))
            .expect("parses");
        glow.validate_options(&flags).expect("valid options");

        let src = glow
            .source_from_arg(&path.to_string_lossy())
            .expect("a source");
        let mut out: Vec<u8> = Vec::new();
        glow.execute_cli(&flags, src, &mut out).expect("renders");
        let text = String::from_utf8(out).expect("utf-8");
        assert_eq!(text, format!("\n  # Title{}\n\n", " ".repeat(69)));
    }

    #[test]
    fn a_code_source_is_wrapped_in_a_fenced_block() {
        let (_dir, mut glow) = isolated();
        let d = tempfile::tempdir().expect("a temp dir");
        let path = d.path().join("main.rs");
        std::fs::write(&path, "fn main() {}\n").expect("write");

        let flags = glow
            .parse_flags(&argv(&["-s", "notty", "-w", "80"]))
            .expect("parses");
        glow.validate_options(&flags).expect("valid options");

        let src = glow
            .source_from_arg(&path.to_string_lossy())
            .expect("a source");
        let mut out: Vec<u8> = Vec::new();
        glow.execute_cli(&flags, src, &mut out).expect("renders");
        assert!(String::from_utf8_lossy(&out).contains("fn main() {}"));
    }

    #[test]
    fn help_output_matches_cobras_template() {
        let (_dir, glow) = isolated();
        let help = glow.app.help(&[]);
        assert!(help.contains("Usage:\n  glow [SOURCE|DIR] [flags]\n  glow [command]"));
        assert!(help.contains("Available Commands:\n  completion  Generate the autocompletion script for the specified shell\n  config      Edit the glow config file\n  help        Help about any command\n"));
        assert!(help
            .contains("  -s, --style string         style name or JSON path (default \"auto\")"));
        assert!(!help.contains("--mouse"), "the mouse flag is hidden");
        assert!(
            help.ends_with("Use \"glow [command] --help\" for more information about a command.\n")
        );
    }

    #[test]
    fn the_version_template_carries_the_name() {
        let (_dir, glow) = isolated();
        assert_eq!(
            glow.app
                .version_template
                .replace("{{.Version}}", &glow.app.version),
            "glow version unknown (built from source)\n"
        );
    }
}
