//! Man page generation from a command tree.
//!
//! Reimplements `github.com/muesli/mango` together with the `mango-cobra` and
//! `mango-pflag` adapters: the command/flag model, the section order, and the
//! roff macros each part emits.

use std::collections::BTreeMap;

use crate::deps::cobra;
use crate::deps::roff::Document;

/// A command-line flag as the man page sees it.
#[derive(Debug, Clone)]
pub struct Flag {
    /// Long name, without dashes.
    pub name: String,
    /// Single-character shorthand, or empty.
    pub short: String,
    /// Help text.
    pub usage: String,
    /// Whether the flag uses the `--` long-form prefix.
    pub pflag: bool,
}

/// A command in the man page's tree.
#[derive(Debug, Clone, Default)]
pub struct Command {
    /// Command name.
    pub name: String,
    /// One-line description.
    pub short: String,
    /// The `Use` line.
    pub usage: String,
    /// Example block.
    pub example: String,
    /// Flags, keyed — and therefore ordered — by name.
    pub flags: BTreeMap<String, Flag>,
    /// Sub-commands, keyed — and therefore ordered — by name.
    pub commands: BTreeMap<String, Command>,
}

impl Command {
    /// A command with no flags or children.
    pub fn new(name: &str, short: &str, usage: &str) -> Command {
        Command {
            name: name.into(),
            short: short.into(),
            usage: usage.into(),
            ..Command::default()
        }
    }

    /// Adds a flag, keeping the first definition of a name.
    pub fn add_flag(&mut self, f: Flag) {
        self.flags.entry(f.name.clone()).or_insert(f);
    }

    /// Adds a sub-command, keeping the first definition of a name.
    pub fn add_command(&mut self, c: Command) {
        self.commands.entry(c.name.clone()).or_insert(c);
    }
}

/// An extra section appended after the generated ones.
#[derive(Debug, Clone)]
pub struct Section {
    /// Section heading.
    pub name: String,
    /// Section body.
    pub text: String,
}

/// A man page generator.
#[derive(Debug, Clone)]
pub struct ManPage {
    /// The root command.
    pub root: Command,
    /// Manual section number.
    pub section: u32,
    /// The one-line description used in the heading and `NAME`.
    pub description: String,
    /// The body of `DESCRIPTION`.
    pub long_description: String,
    /// Extra sections.
    pub sections: Vec<Section>,
}

impl ManPage {
    /// A generator for `title`, described by `description`.
    pub fn new(section: u32, title: &str, description: &str) -> ManPage {
        ManPage {
            root: Command::new(title, "", ""),
            section,
            description: description.into(),
            long_description: String::new(),
            sections: Vec::new(),
        }
    }

    /// Sets the `DESCRIPTION` body.
    pub fn with_long_description(mut self, desc: &str) -> ManPage {
        self.long_description = desc.into();
        self
    }

    /// Renders the man page, dated `date` (`YYYY-MM-DD`).
    pub fn build(&self, date: &str) -> String {
        let mut w = Document::new();

        w.heading(self.section, &self.root.name, &self.description, date);

        w.section("Name");
        w.text(&format!("{} - {}", self.root.name, self.description));

        w.section("Synopsis");
        w.text_bold(&self.root.name);
        w.text(" [");
        w.text_italic("options...");
        w.text("] [");
        w.text_italic("argument...");
        w.text("]");

        w.section("Description");
        w.text(&self.long_description);

        self.build_command(&mut w, &self.root);

        for v in &self.sections {
            w.section(&v.name);
            w.text(&v.text);
        }

        w.to_string()
    }

    fn build_command(&self, w: &mut Document, c: &Command) {
        let is_root = c.name == self.root.name;

        if !c.flags.is_empty() {
            if is_root {
                w.section("Options");
                w.tagged_paragraph(-1);
            } else {
                w.tagged_paragraph(-1);
                w.text_bold("OPTIONS");
                w.indent(4);
            }

            for (i, opt) in c.flags.values().enumerate() {
                if i > 0 {
                    w.tagged_paragraph(-1);
                }

                let prefix = if opt.pflag { "--" } else { "-" };
                if opt.short.is_empty() {
                    w.text_bold(&format!("{prefix}{}", opt.name));
                } else {
                    w.text_bold(&format!("-{}, {prefix}{}", opt.short, opt.name));
                }
                w.end_section();
                w.text(&opt.usage.replace('\n', " "));
            }

            if !is_root {
                w.indent_end();
            }
        }

        if !c.commands.is_empty() {
            if is_root {
                w.section("Commands");
                w.tagged_paragraph(-1);
            } else {
                w.tagged_paragraph(-1);
                w.text_bold("COMMANDS");
                w.indent(4);
            }

            for (i, sub) in c.commands.values().enumerate() {
                if i > 0 {
                    w.tagged_paragraph(-1);
                }

                w.text_bold(&sub.name);
                if !sub.usage.is_empty() {
                    w.text(
                        sub.usage
                            .strip_prefix(sub.name.as_str())
                            .unwrap_or(&sub.usage),
                    );
                }
                w.indent(4);
                w.text(&sub.short.replace('\n', " "));
                w.indent_end();

                self.build_command(w, sub);
            }

            if !is_root {
                w.indent_end();
            }
        }

        if !c.example.is_empty() {
            if is_root {
                w.section("Examples");
                w.tagged_paragraph(-1);
            } else {
                w.tagged_paragraph(-1);
                w.text_bold("EXAMPLES");
                w.indent(4);
            }
            w.text(&c.example);

            if is_root {
                w.end_section();
            } else {
                w.indent_end();
            }
        }
    }
}

/// Builds a man page generator from a cobra command tree.
///
/// Hidden sub-commands are skipped. The `help` and `version` flags are skipped
/// too: cobra adds them to the command it is about to execute, so the root
/// command never carries them while `man` is the one running.
pub fn from_cobra(section: u32, root: &cobra::Command) -> ManPage {
    let mut page =
        ManPage::new(section, root.name(), &root.short).with_long_description(&root.long);
    let mut item = Command::new(root.name(), "", "");
    item.example = root.example.clone();
    add_flags(&mut item, root);
    for sub in &root.children {
        if sub.hidden {
            continue;
        }
        item.add_command(sub_command(sub));
    }
    page.root = item;
    page
}

fn sub_command(c: &cobra::Command) -> Command {
    let mut item = Command::new(c.name(), &c.short, &c.use_line);
    item.example = c.example.clone();
    add_flags(&mut item, c);
    for sub in &c.children {
        if sub.hidden {
            continue;
        }
        item.add_command(sub_command(sub));
    }
    item
}

fn add_flags(item: &mut Command, c: &cobra::Command) {
    for f in c.flags.iter().chain(c.persistent_flags.iter()) {
        if f.name == "help" || f.name == "version" {
            continue;
        }
        item.add_flag(Flag {
            name: f.name.clone(),
            short: f.shorthand.clone(),
            usage: f.usage.clone(),
            pflag: true,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_page_has_the_four_standard_sections() {
        let page = ManPage::new(1, "demo", "Do things").with_long_description("Long text.");
        let out = page.build("2024-01-02");
        assert_eq!(
            out,
            concat!(
                ".TH DEMO 1 \"2024-01-02\" \"demo\" \"Do things\"\n",
                ".SH NAME\n",
                "demo - Do things\n",
                ".SH SYNOPSIS\n",
                "\\fBdemo\\fP [\\fIoptions\\&.\\&.\\&.\\fP] [\\fIargument\\&.\\&.\\&.\\fP]\n",
                ".SH DESCRIPTION\n",
                "Long text\\&."
            )
        );
    }

    #[test]
    fn flags_are_sorted_and_tagged() {
        let mut page = ManPage::new(1, "demo", "Do things");
        page.root.add_flag(Flag {
            name: "width".into(),
            short: "w".into(),
            usage: "word-wrap at width".into(),
            pflag: true,
        });
        page.root.add_flag(Flag {
            name: "all".into(),
            short: "a".into(),
            usage: "show all".into(),
            pflag: true,
        });
        let out = page.build("2024-01-02");
        let options = out
            .split(".SH OPTIONS\n")
            .nth(1)
            .expect("an options section");
        assert_eq!(
            options,
            ".TP\n\\fB-a, --all\\fP\nshow all\n.TP\n\\fB-w, --width\\fP\nword-wrap at width"
        );
    }

    #[test]
    fn a_flag_without_a_shorthand_omits_the_comma() {
        let mut page = ManPage::new(1, "demo", "Do things");
        page.root.add_flag(Flag {
            name: "config".into(),
            short: String::new(),
            usage: "config file".into(),
            pflag: true,
        });
        assert!(page
            .build("2024-01-02")
            .contains("\\fB--config\\fP\nconfig file"));
    }

    #[test]
    fn sub_commands_nest_their_own_sections() {
        let mut page = ManPage::new(1, "demo", "Do things");
        let mut child = Command::new("sub", "A sub-command", "sub [thing]");
        child.add_flag(Flag {
            name: "no-descriptions".into(),
            short: String::new(),
            usage: "disable descriptions".into(),
            pflag: true,
        });
        page.root.add_command(child);
        let out = page.build("2024-01-02");
        let commands = out
            .split(".SH COMMANDS\n")
            .nth(1)
            .expect("a commands section");
        assert_eq!(
            commands,
            concat!(
                ".TP\n\\fBsub\\fP [thing]\n.RS 4\nA sub-command\n.RE\n",
                ".TP\n\\fBOPTIONS\\fP\n.RS 4\n\\fB--no-descriptions\\fP\n",
                "disable descriptions\n.RE\n"
            )
        );
    }

    #[test]
    fn hidden_sub_commands_are_skipped() {
        let mut root = cobra::Command::new("root", "demo", "Do things");
        let mut hidden = cobra::Command::new("man", "man", "Generates manpages");
        hidden.hidden = true;
        root.children.push(hidden);
        root.children
            .push(cobra::Command::new("config", "config", "Edit the config"));
        root.flags
            .add(cobra::Flag::bool("help", "h", false, "help for demo"));
        root.flags
            .add(cobra::Flag::bool("all", "a", false, "show all"));

        let page = from_cobra(1, &root);
        assert!(page.root.commands.contains_key("config"));
        assert!(
            !page.root.commands.contains_key("man"),
            "hidden commands are skipped"
        );
        assert!(
            !page.root.flags.contains_key("help"),
            "cobra adds help at execution time"
        );
        assert!(page.root.flags.contains_key("all"));
    }
}
