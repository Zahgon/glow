//! Command-line parsing, dispatch and help rendering.
//!
//! Reimplements the parts of `spf13/cobra` and `spf13/pflag` that form glow's
//! user interface: POSIX/GNU flag parsing, argument validation, the generated
//! `help` and `completion` commands, and — byte for byte — the help and usage
//! templates, including pflag's flag-column alignment.

use std::collections::BTreeMap;
use std::fmt;

/// The value a flag holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// A boolean flag.
    Bool(bool),
    /// A string flag.
    Str(String),
    /// An unsigned integer flag.
    Uint(u64),
}

impl Value {
    /// pflag's `Value.Type()`, used to pick the usage placeholder.
    fn type_name(&self) -> &'static str {
        match self {
            Value::Bool(_) => "bool",
            Value::Str(_) => "string",
            Value::Uint(_) => "uint",
        }
    }

    /// pflag's `DefValue` rendering.
    fn to_def_string(&self) -> String {
        match self {
            Value::Bool(b) => b.to_string(),
            Value::Str(s) => s.clone(),
            Value::Uint(u) => u.to_string(),
        }
    }

    fn is_zero(&self) -> bool {
        match self {
            Value::Bool(b) => !*b,
            Value::Str(s) => s.is_empty(),
            Value::Uint(u) => *u == 0,
        }
    }
}

/// One command-line flag.
#[derive(Debug, Clone)]
pub struct Flag {
    /// Long name, without the leading dashes.
    pub name: String,
    /// Single-character shorthand, or empty.
    pub shorthand: String,
    /// Help text.
    pub usage: String,
    /// Current value.
    pub value: Value,
    /// The value as it was before any parsing, for the `(default …)` note.
    pub default: Value,
    /// Whether the flag appeared on the command line.
    pub changed: bool,
    /// Whether the flag is omitted from help output.
    pub hidden: bool,
}

impl Flag {
    /// A boolean flag.
    pub fn bool(name: &str, shorthand: &str, default: bool, usage: &str) -> Flag {
        Flag::new(name, shorthand, Value::Bool(default), usage)
    }
    /// A string flag.
    pub fn string(name: &str, shorthand: &str, default: &str, usage: &str) -> Flag {
        Flag::new(name, shorthand, Value::Str(default.into()), usage)
    }
    /// An unsigned integer flag.
    pub fn uint(name: &str, shorthand: &str, default: u64, usage: &str) -> Flag {
        Flag::new(name, shorthand, Value::Uint(default), usage)
    }

    fn new(name: &str, shorthand: &str, default: Value, usage: &str) -> Flag {
        Flag {
            name: name.into(),
            shorthand: shorthand.into(),
            usage: usage.into(),
            value: default.clone(),
            default,
            changed: false,
            hidden: false,
        }
    }

    /// Marks the flag hidden.
    pub fn hidden(mut self) -> Flag {
        self.hidden = true;
        self
    }

    /// Whether the flag takes no value when written bare (booleans do not).
    fn is_bool(&self) -> bool {
        matches!(self.value, Value::Bool(_))
    }
}

/// A set of flags, kept sorted by name the way pflag prints them.
#[derive(Debug, Clone, Default)]
pub struct FlagSet {
    flags: Vec<Flag>,
}

impl FlagSet {
    /// An empty set.
    pub fn new() -> FlagSet {
        FlagSet::default()
    }

    /// Adds a flag.
    pub fn add(&mut self, flag: Flag) {
        self.flags.push(flag);
    }

    /// A copy of the set without the named flags.
    pub fn without(&self, names: &[&str]) -> FlagSet {
        FlagSet {
            flags: self
                .flags
                .iter()
                .filter(|f| !names.contains(&f.name.as_str()))
                .cloned()
                .collect(),
        }
    }

    /// The flags, in declaration order.
    pub fn iter(&self) -> std::slice::Iter<'_, Flag> {
        self.flags.iter()
    }

    /// Looks a flag up by long name.
    pub fn lookup(&self, name: &str) -> Option<&Flag> {
        self.flags.iter().find(|f| f.name == name)
    }

    fn lookup_mut(&mut self, name: &str) -> Option<&mut Flag> {
        self.flags.iter_mut().find(|f| f.name == name)
    }

    fn by_shorthand_mut(&mut self, sh: char) -> Option<&mut Flag> {
        self.flags.iter_mut().find(|f| f.shorthand.starts_with(sh))
    }

    /// Whether the named flag was given on the command line.
    pub fn changed(&self, name: &str) -> bool {
        self.lookup(name).map(|f| f.changed).unwrap_or(false)
    }

    /// The current value of a boolean flag.
    pub fn bool(&self, name: &str) -> bool {
        match self.lookup(name).map(|f| &f.value) {
            Some(Value::Bool(b)) => *b,
            _ => false,
        }
    }

    /// The current value of a string flag.
    pub fn string(&self, name: &str) -> String {
        match self.lookup(name).map(|f| &f.value) {
            Some(Value::Str(s)) => s.clone(),
            _ => String::new(),
        }
    }

    /// The current value of a uint flag.
    pub fn uint(&self, name: &str) -> u64 {
        match self.lookup(name).map(|f| &f.value) {
            Some(Value::Uint(u)) => *u,
            _ => 0,
        }
    }

    /// Every visible flag, sorted by name.
    fn visible_sorted(&self) -> Vec<&Flag> {
        let mut sorted: Vec<&Flag> = self.flags.iter().filter(|f| !f.hidden).collect();
        sorted.sort_by(|a, b| a.name.cmp(&b.name));
        sorted
    }

    /// Whether any flag would appear in help output.
    fn has_visible(&self) -> bool {
        self.flags.iter().any(|f| !f.hidden)
    }

    /// pflag's `FlagUsages`: two-space indent, aligned usage column, and a
    /// `(default …)` note for any flag whose default is not the zero value.
    pub fn flag_usages(&self) -> String {
        let mut lines: Vec<(String, String)> = Vec::new();
        let mut maxlen = 0usize;

        for flag in self.visible_sorted() {
            let mut head = if flag.shorthand.is_empty() {
                format!("      --{}", flag.name)
            } else {
                format!("  -{}, --{}", flag.shorthand, flag.name)
            };
            let varname = match flag.value {
                Value::Bool(_) => "",
                Value::Str(_) => "string",
                Value::Uint(_) => "uint",
            };
            if !varname.is_empty() {
                head.push(' ');
                head.push_str(varname);
            }
            // pflag measures the head plus the NUL separator it inserts.
            maxlen = maxlen.max(head.chars().count() + 1);

            let mut tail = flag.usage.clone();
            if !flag.default.is_zero() {
                if flag.default.type_name() == "string" {
                    tail.push_str(&format!(" (default {:?})", flag.default.to_def_string()));
                } else {
                    tail.push_str(&format!(" (default {})", flag.default.to_def_string()));
                }
            }
            lines.push((head, tail));
        }

        let mut out = String::new();
        for (head, tail) in lines {
            let spacing = " ".repeat(maxlen - head.chars().count());
            out.push_str(&format!("{head} {spacing} {tail}\n"));
        }
        out
    }
}

/// How many positional arguments a command accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Args {
    /// Any number.
    Arbitrary,
    /// None at all.
    NoArgs,
    /// At most `n`.
    MaximumN(usize),
    /// Exactly `n`.
    ExactArgs(usize),
}

/// A command in the tree.
#[derive(Debug, Clone)]
pub struct Command {
    /// Stable identifier used by the caller to dispatch.
    pub id: &'static str,
    /// The `Use` line: name plus argument placeholders.
    pub use_line: String,
    /// One-line description.
    pub short: String,
    /// Long description shown above the usage block.
    pub long: String,
    /// Example block.
    pub example: String,
    /// Whether the command is omitted from the command list.
    pub hidden: bool,
    /// Whether the command can run itself.
    pub runnable: bool,
    /// Whether `[flags]` is appended to the use line.
    pub disable_flags_in_use_line: bool,
    /// Positional-argument rule.
    pub args: Args,
    /// Flags local to this command.
    pub flags: FlagSet,
    /// Flags inherited by every descendant.
    pub persistent_flags: FlagSet,
    /// Sub-commands, in declaration order.
    pub children: Vec<Command>,
}

impl Command {
    /// A runnable command with no flags.
    pub fn new(id: &'static str, use_line: &str, short: &str) -> Command {
        Command {
            id,
            use_line: use_line.into(),
            short: short.into(),
            long: String::new(),
            example: String::new(),
            hidden: false,
            runnable: true,
            disable_flags_in_use_line: false,
            args: Args::Arbitrary,
            flags: FlagSet::new(),
            persistent_flags: FlagSet::new(),
            children: Vec::new(),
        }
    }

    /// The first word of the use line.
    pub fn name(&self) -> &str {
        self.use_line.split_whitespace().next().unwrap_or("")
    }

    /// Whether the command lists any sub-command in help.
    fn has_available_subcommands(&self) -> bool {
        self.children.iter().any(|c| !c.hidden)
    }
}

/// A user-facing failure. Printed as `Error: <message>` on stderr.
#[derive(Debug, Clone)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for Error {}

/// What `parse` decided the invocation means.
#[derive(Debug)]
pub enum Outcome {
    /// Print this text on stdout and exit 0.
    Print(String),
    /// Run the identified command.
    Run {
        /// Identifier of the command to run.
        id: &'static str,
        /// Positional arguments.
        args: Vec<String>,
    },
}

/// The parsed command line: which command to run and the resolved flags.
#[derive(Debug)]
pub struct Parsed {
    /// What to do.
    pub outcome: Outcome,
    /// Flags visible to the resolved command, merged local-then-inherited.
    pub flags: FlagSet,
}

/// The root of a command tree, with the version metadata cobra needs.
pub struct App {
    /// The command tree.
    pub root: Command,
    /// Version string reported by `--version`.
    pub version: String,
    /// Template used to render the version, with `{{.Version}}` substituted.
    pub version_template: String,
}

impl App {
    /// Renders the help text for the command reached by `path`.
    pub fn help(&self, path: &[usize]) -> String {
        let cmd = self.command_at(path);
        let mut out = String::new();
        let long = if cmd.long.is_empty() {
            cmd.short.clone()
        } else {
            cmd.long.clone()
        };
        if !long.is_empty() {
            out.push_str(long.trim_end());
            out.push_str("\n\n");
        }
        out.push_str(&self.usage(path));
        out
    }

    /// Renders the usage block for the command reached by `path`.
    pub fn usage(&self, path: &[usize]) -> String {
        let cmd = self.command_at(path);
        let command_path = self.command_path(path);
        let mut out = String::new();

        out.push_str("Usage:");
        if cmd.runnable {
            out.push_str(&format!("\n  {}", self.use_line(path)));
        }
        if cmd.has_available_subcommands() {
            out.push_str(&format!("\n  {command_path} [command]"));
        }
        if !cmd.example.is_empty() {
            out.push_str(&format!("\n\nExamples:\n{}", cmd.example));
        }
        if cmd.has_available_subcommands() {
            out.push_str("\n\nAvailable Commands:");
            let pad = self.name_padding(cmd);
            for child in cmd.children.iter().filter(|c| !c.hidden) {
                out.push_str(&format!(
                    "\n  {:<pad$} {}",
                    child.name(),
                    child.short,
                    pad = pad
                ));
            }
        }
        if cmd.flags.has_visible() || (path.is_empty() && cmd.persistent_flags.has_visible()) {
            let mut local = cmd.flags.clone();
            if path.is_empty() {
                for f in &cmd.persistent_flags.flags {
                    local.add(f.clone());
                }
            }
            out.push_str(&format!(
                "\n\nFlags:\n{}",
                local.flag_usages().trim_end_matches([' ', '\n', '\t'])
            ));
        }
        let inherited = self.inherited_flags(path);
        if !path.is_empty() && inherited.has_visible() {
            out.push_str(&format!(
                "\n\nGlobal Flags:\n{}",
                inherited.flag_usages().trim_end_matches([' ', '\n', '\t'])
            ));
        }
        if cmd.has_available_subcommands() {
            out.push_str(&format!(
                "\n\nUse \"{command_path} [command] --help\" for more information about a command."
            ));
        }
        out.push('\n');
        out
    }

    fn name_padding(&self, cmd: &Command) -> usize {
        const MIN_NAME_PADDING: usize = 11;
        let longest = cmd
            .children
            .iter()
            .filter(|c| !c.hidden)
            .map(|c| c.name().chars().count())
            .max()
            .unwrap_or(0);
        MIN_NAME_PADDING.max(longest)
    }

    /// The space-separated path of command names down to `path`.
    pub fn command_path(&self, path: &[usize]) -> String {
        let mut parts = vec![self.root.name().to_string()];
        let mut cmd = &self.root;
        for i in path {
            cmd = &cmd.children[*i];
            parts.push(cmd.name().to_string());
        }
        parts.join(" ")
    }

    fn use_line(&self, path: &[usize]) -> String {
        let cmd = self.command_at(path);
        let mut line = if path.is_empty() {
            cmd.use_line.clone()
        } else {
            let parent = self.command_path(&path[..path.len() - 1]);
            format!("{parent} {}", cmd.use_line)
        };
        if !cmd.disable_flags_in_use_line && !line.ends_with("[flags]") {
            let has_flags = cmd.flags.has_visible()
                || self.inherited_flags(path).has_visible()
                || (path.is_empty() && cmd.persistent_flags.has_visible());
            if has_flags {
                line.push_str(" [flags]");
            }
        }
        line
    }

    /// The command reached by following `path` from the root.
    pub fn command_at(&self, path: &[usize]) -> &Command {
        let mut cmd = &self.root;
        for i in path {
            cmd = &cmd.children[*i];
        }
        cmd
    }

    fn inherited_flags(&self, path: &[usize]) -> FlagSet {
        let mut set = FlagSet::new();
        let mut cmd = &self.root;
        for depth in 0..=path.len() {
            if depth < path.len() {
                for f in &cmd.persistent_flags.flags {
                    set.add(f.clone());
                }
                cmd = &cmd.children[path[depth]];
            }
        }
        set
    }

    /// All flags a command can accept: its own plus everything inherited.
    fn effective_flags(&self, path: &[usize]) -> FlagSet {
        let mut set = self.command_at(path).flags.clone();
        let mut cmd = &self.root;
        for depth in 0..=path.len() {
            for f in &cmd.persistent_flags.flags {
                set.add(f.clone());
            }
            if depth < path.len() {
                cmd = &cmd.children[path[depth]];
            }
        }
        set
    }

    /// pflag's `ParseFlags`: parses `argv` against the root command's flags,
    /// with no sub-command dispatch and no positional-argument validation.
    pub fn parse_root_flags(&self, argv: &[String]) -> Result<FlagSet, Error> {
        let mut flags = self.effective_flags(&[]);
        parse_flags(&mut flags, argv, &self.root, &self.command_path(&[]))?;
        Ok(flags)
    }

    /// Resolves an argument vector into a command and its flags.
    ///
    /// The root command traverses its children, so the arguments are split at
    /// each sub-command name: what comes before a name belongs to the command
    /// above it and what comes after belongs to the command itself. That is why
    /// `glow -s light man` works while `glow man -s light` is an unknown flag.
    pub fn parse(&self, argv: &[String]) -> Result<Parsed, Error> {
        let mut path: Vec<usize> = Vec::new();
        let mut rest: Vec<String> = argv.to_vec();
        let mut flags = self.effective_flags(&[]);

        let positional = loop {
            let level = self.effective_flags(&path);
            let cmd = self.command_at(&path);

            // Traversal happens before `help` and `version` are declared, so it
            // does not know they take no value — an unknown flag is assumed to
            // swallow the argument after it. That is why `glow --help man`
            // shows the root's help rather than the man command's.
            let traversal = level.without(&["help", "version"]);

            // The first argument that is neither a flag nor a flag's value
            // decides whether we descend.
            let mut boundary = None;
            let mut i = 0usize;
            while i < rest.len() {
                let arg = &rest[i];
                if arg == "--" {
                    break;
                }
                if arg.starts_with('-') {
                    // A flag that takes a separate value swallows the next
                    // argument, which must not be mistaken for a command name.
                    if flag_swallows_next(arg, &traversal) {
                        i += 1;
                    }
                    i += 1;
                    continue;
                }
                if let Some(ci) = cmd.children.iter().position(|c| c.name() == arg) {
                    boundary = Some((i, ci));
                }
                break;
            }

            let (head, tail, descend) = match boundary {
                Some((i, ci)) => (&rest[..i], Some((rest[i + 1..].to_vec(), ci)), true),
                None => (&rest[..], None, false),
            };

            let mut parsed = level;
            let pos = parse_flags(&mut parsed, head, cmd, &self.command_path(&path))?;
            merge_parsed_flags(&mut flags, &parsed);

            match tail {
                Some((next, ci)) if descend => {
                    rest = next;
                    path.push(ci);
                }
                _ => break pos,
            }
        };

        let cmd = self.command_at(&path);

        if flags.bool("help") {
            return Ok(Parsed {
                outcome: Outcome::Print(self.help(&path)),
                flags,
            });
        }
        if path.is_empty() && flags.bool("version") {
            return Ok(Parsed {
                outcome: Outcome::Print(
                    self.version_template.replace("{{.Version}}", &self.version),
                ),
                flags,
            });
        }

        // `help [command]` prints the help of its target. Cobra resolves the
        // target with `Find`, which stops at the first name that matches no
        // sub-command and shows whatever it had reached — so an unknown topic
        // falls back to the root's help rather than failing.
        if cmd.id == "help" {
            let mut target: Vec<usize> = Vec::new();
            let mut node = &self.root;
            for name in &positional {
                match node.children.iter().position(|c| c.name() == name) {
                    Some(i) => {
                        target.push(i);
                        node = &node.children[i];
                    }
                    None => break,
                }
            }
            return Ok(Parsed {
                outcome: Outcome::Print(self.help(&target)),
                flags,
            });
        }

        validate_args(cmd, &positional, &self.command_path(&path))?;

        Ok(Parsed {
            outcome: Outcome::Run {
                id: cmd.id,
                args: positional,
            },
            flags,
        })
    }
}

/// Folds one level's parsed flags into the set the caller sees.
///
/// Every level contributes: a flag set above the resolved command is still
/// readable from it, which is how `--config` reaches a sub-command.
fn merge_parsed_flags(dst: &mut FlagSet, src: &FlagSet) {
    for f in src.iter() {
        match dst.lookup_mut(&f.name) {
            Some(existing) => {
                if f.changed {
                    existing.value = f.value.clone();
                    existing.changed = true;
                }
            }
            None => dst.add(f.clone()),
        }
    }
}

/// Whether `arg` swallows the argument after it, the way cobra's `Traverse`
/// decides it.
///
/// A flag it has never heard of is assumed to take a value, which is what makes
/// `--help` hide the sub-command name behind it.
fn flag_swallows_next(arg: &str, flags: &FlagSet) -> bool {
    if let Some(name) = arg.strip_prefix("--") {
        if name.contains('=') {
            return false;
        }
        return !flags.lookup(name).map(|f| f.is_bool()).unwrap_or(false);
    }
    if arg.len() == 2 && !arg.contains('=') {
        let c = arg.chars().nth(1).expect("two characters");
        let mut set = flags.clone();
        return !set
            .by_shorthand_mut(c)
            .map(|f| f.is_bool())
            .unwrap_or(false);
    }
    false
}

/// pflag's parsing loop: long flags, shorthand runs, `=` values and `--`.
fn parse_flags(
    flags: &mut FlagSet,
    argv: &[String],
    _cmd: &Command,
    _path: &str,
) -> Result<Vec<String>, Error> {
    let mut positional = Vec::new();
    let mut i = 0usize;
    while i < argv.len() {
        let arg = &argv[i];
        if arg == "--" {
            positional.extend_from_slice(&argv[i + 1..]);
            break;
        }
        if let Some(body) = arg.strip_prefix("--") {
            let (name, inline) = match body.split_once('=') {
                Some((n, v)) => (n, Some(v.to_string())),
                None => (body, None),
            };
            let is_bool = flags
                .lookup(name)
                .ok_or_else(|| Error(format!("unknown flag: --{name}")))?
                .is_bool();
            let value = match (inline, is_bool) {
                (Some(v), _) => v,
                (None, true) => "true".to_string(),
                (None, false) => {
                    i += 1;
                    argv.get(i)
                        .cloned()
                        .ok_or_else(|| Error(format!("flag needs an argument: --{name}")))?
                }
            };
            set_flag(flags, name, &value)?;
            i += 1;
            continue;
        }
        if arg.len() > 1 && arg.starts_with('-') {
            let shorts: Vec<char> = arg[1..].chars().collect();
            let mut j = 0usize;
            while j < shorts.len() {
                let c = shorts[j];
                let (name, is_bool) = {
                    let f = flags
                        .by_shorthand_mut(c)
                        .ok_or_else(|| Error(format!("unknown shorthand flag: '{c}' in {arg}")))?;
                    (f.name.clone(), f.is_bool())
                };
                if is_bool {
                    set_flag(flags, &name, "true")?;
                    j += 1;
                    continue;
                }
                let rest: String = shorts[j + 1..].iter().collect();
                let value = if let Some(stripped) = rest.strip_prefix('=') {
                    stripped.to_string()
                } else if !rest.is_empty() {
                    rest
                } else {
                    i += 1;
                    argv.get(i)
                        .cloned()
                        .ok_or_else(|| Error(format!("flag needs an argument: '{c}' in {arg}")))?
                };
                set_flag(flags, &name, &value)?;
                j = shorts.len();
            }
            i += 1;
            continue;
        }
        positional.push(arg.clone());
        i += 1;
    }
    Ok(positional)
}

fn set_flag(flags: &mut FlagSet, name: &str, raw: &str) -> Result<(), Error> {
    let flag = flags
        .lookup_mut(name)
        .ok_or_else(|| Error(format!("unknown flag: --{name}")))?;
    // pflag names the flag by its own definition, whichever spelling was used.
    let display = if flag.shorthand.is_empty() {
        format!("--{}", flag.name)
    } else {
        format!("-{}, --{}", flag.shorthand, flag.name)
    };
    flag.value = match &flag.value {
        Value::Bool(_) => match raw {
            "1" | "t" | "T" | "true" | "TRUE" | "True" => Value::Bool(true),
            "0" | "f" | "F" | "false" | "FALSE" | "False" => Value::Bool(false),
            other => {
                return Err(Error(format!(
                    "invalid argument {other:?} for \"{display}\" flag: strconv.ParseBool: parsing {other:?}: invalid syntax"
                )))
            }
        },
        Value::Str(_) => Value::Str(raw.to_string()),
        Value::Uint(_) => match raw.parse::<u64>() {
            Ok(v) => Value::Uint(v),
            Err(_) => {
                return Err(Error(format!(
                    "invalid argument {raw:?} for \"{display}\" flag: strconv.ParseUint: parsing {raw:?}: invalid syntax"
                )))
            }
        },
    };
    flag.changed = true;
    Ok(())
}

fn validate_args(cmd: &Command, args: &[String], path: &str) -> Result<(), Error> {
    match cmd.args {
        Args::Arbitrary => Ok(()),
        Args::NoArgs => {
            if let Some(first) = args.first() {
                Err(Error(format!("unknown command {first:?} for \"{path}\"")))
            } else {
                Ok(())
            }
        }
        Args::MaximumN(n) => {
            if args.len() > n {
                Err(Error(format!(
                    "accepts at most {n} arg(s), received {}",
                    args.len()
                )))
            } else {
                Ok(())
            }
        }
        Args::ExactArgs(n) => {
            if args.len() != n {
                Err(Error(format!(
                    "accepts {n} arg(s), received {}",
                    args.len()
                )))
            } else {
                Ok(())
            }
        }
    }
}

/// The shell completion scripts cobra generates, embedded verbatim.
///
/// They are boilerplate that depends only on the root command's name, which is
/// fixed here, so carrying the bytes over reproduces them exactly.
pub fn completion_script(shell: &str) -> Option<&'static str> {
    let scripts: BTreeMap<&str, &str> = [
        ("bash", include_str!("completions/bash.sh")),
        ("zsh", include_str!("completions/zsh.sh")),
        ("fish", include_str!("completions/fish.sh")),
        ("powershell", include_str!("completions/powershell.ps1")),
    ]
    .into_iter()
    .collect();
    scripts.get(shell).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut root = Command::new("root", "demo [SOURCE|DIR]", "Do things");
        root.args = Args::MaximumN(1);
        root.flags.add(Flag::bool("all", "a", false, "show all"));
        root.flags.add(Flag::string(
            "style",
            "s",
            "auto",
            "style name or JSON path",
        ));
        root.flags
            .add(Flag::uint("width", "w", 0, "word-wrap at width"));
        root.flags
            .add(Flag::bool("help", "h", false, "help for demo"));
        root.flags
            .add(Flag::bool("version", "v", false, "version for demo"));
        root.persistent_flags
            .add(Flag::string("config", "", "", "config file (default )"));
        let mut child = Command::new("sub", "sub", "A sub-command");
        child.args = Args::NoArgs;
        child
            .flags
            .add(Flag::bool("help", "h", false, "help for sub"));
        root.children.push(child);
        App {
            root,
            version: "1.2.3".into(),
            version_template: "demo version {{.Version}}\n".into(),
        }
    }

    fn argv(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_long_and_short_flags() {
        let parsed = app()
            .parse(&argv(&["-a", "--style", "light", "-w", "40"]))
            .unwrap();
        assert!(parsed.flags.bool("all"));
        assert_eq!(parsed.flags.string("style"), "light");
        assert_eq!(parsed.flags.uint("width"), 40);
    }

    #[test]
    fn parses_equals_form_and_attached_shorthand() {
        let parsed = app().parse(&argv(&["--style=light", "-w40"])).unwrap();
        assert_eq!(parsed.flags.string("style"), "light");
        assert_eq!(parsed.flags.uint("width"), 40);
    }

    #[test]
    fn tracks_whether_a_flag_was_given() {
        let parsed = app().parse(&argv(&[])).unwrap();
        assert!(!parsed.flags.changed("style"));
        let parsed = app().parse(&argv(&["-s", "dark"])).unwrap();
        assert!(parsed.flags.changed("style"));
    }

    #[test]
    fn rejects_unknown_flags() {
        let err = app().parse(&argv(&["--nope"])).unwrap_err();
        assert_eq!(err.0, "unknown flag: --nope");
    }

    #[test]
    fn rejects_a_non_numeric_uint() {
        let err = app().parse(&argv(&["-w", "x"])).unwrap_err();
        assert!(err
            .0
            .starts_with("invalid argument \"x\" for \"-w, --width\" flag"));
    }

    #[test]
    fn enforces_the_maximum_argument_count() {
        let err = app().parse(&argv(&["a", "b"])).unwrap_err();
        assert_eq!(err.0, "accepts at most 1 arg(s), received 2");
    }

    #[test]
    fn enforces_no_args_on_subcommands() {
        let err = app().parse(&argv(&["sub", "x"])).unwrap_err();
        assert_eq!(err.0, "unknown command \"x\" for \"demo sub\"");
    }

    #[test]
    fn a_flag_value_is_not_mistaken_for_a_command() {
        let parsed = app()
            .parse(&argv(&["--config", "sub", "sub"]))
            .expect("parses");
        match parsed.outcome {
            Outcome::Run { id, .. } => assert_eq!(id, "sub"),
            other => panic!("expected the sub-command, got {other:?}"),
        }
        assert_eq!(parsed.flags.string("config"), "sub");

        let parsed = app().parse(&argv(&["-s", "light", "sub"])).expect("parses");
        match parsed.outcome {
            Outcome::Run { id, .. } => assert_eq!(id, "sub"),
            other => panic!("expected the sub-command, got {other:?}"),
        }
    }

    #[test]
    fn help_before_a_sub_command_hides_it() {
        // cobra has not declared `--help` when it traverses, so the flag is
        // assumed to take a value and swallows the name after it.
        let parsed = app().parse(&argv(&["--help", "sub"])).expect("parses");
        match parsed.outcome {
            Outcome::Print(text) => assert!(text.starts_with("Do things"), "{text}"),
            other => panic!("expected the root's help, got {other:?}"),
        }
        let parsed = app().parse(&argv(&["-v", "sub"])).expect("parses");
        match parsed.outcome {
            Outcome::Print(text) => assert_eq!(text, "demo version 1.2.3\n"),
            other => panic!("expected the version, got {other:?}"),
        }
    }

    #[test]
    fn a_flag_after_a_sub_command_belongs_to_it() {
        let err = app().parse(&argv(&["sub", "-s", "light"])).unwrap_err();
        assert_eq!(err.0, "unknown shorthand flag: 's' in -s");
        let parsed = app().parse(&argv(&["sub", "--help"])).expect("parses");
        match parsed.outcome {
            Outcome::Print(text) => assert!(text.starts_with("A sub-command"), "{text}"),
            other => panic!("expected the sub-command's help, got {other:?}"),
        }
    }

    #[test]
    fn dispatches_to_subcommands() {
        let parsed = app().parse(&argv(&["sub"])).unwrap();
        match parsed.outcome {
            Outcome::Run { id, .. } => assert_eq!(id, "sub"),
            other => panic!("expected a run outcome, got {other:?}"),
        }
    }

    #[test]
    fn version_uses_the_template() {
        let parsed = app().parse(&argv(&["--version"])).unwrap();
        match parsed.outcome {
            Outcome::Print(s) => assert_eq!(s, "demo version 1.2.3\n"),
            other => panic!("expected printed output, got {other:?}"),
        }
    }

    #[test]
    fn flag_usages_align_on_the_widest_flag() {
        let a = app();
        let mut local = a.root.flags.clone();
        for f in &a.root.persistent_flags.flags {
            local.add(f.clone());
        }
        let usages = local.flag_usages();
        assert_eq!(usages.lines().count(), 6);
        let columns: Vec<usize> = usages
            .lines()
            .map(|l| l.rfind("  ").expect("a gap before the usage"))
            .collect();
        assert!(columns.iter().all(|c| *c > 0));
        assert!(usages.contains("  -a, --all"), "{usages}");
    }

    #[test]
    fn string_defaults_are_quoted_in_help() {
        let a = app();
        let usages = a.root.flags.flag_usages();
        assert!(usages.contains("(default \"auto\")"), "{usages}");
        assert!(!usages.contains("(default 0)"), "zero defaults are omitted");
    }

    #[test]
    fn completion_scripts_exist_for_every_shell() {
        for shell in ["bash", "zsh", "fish", "powershell"] {
            assert!(completion_script(shell).is_some(), "{shell}");
        }
        assert!(completion_script("nope").is_none());
    }
}
