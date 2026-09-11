# Dependency decisions

Glow's behaviour is largely its dependencies' behaviour: the rendered bytes come
from Glamour, the help text from Cobra, the config precedence from Viper, the
filter order from `sahilm/fuzzy`. Swapping in a Rust crate whose semantics
merely resemble one of those changes the output, so each module in `deps/`
reproduces the *observable* behaviour of one Go package rather than wrapping a
look-alike. This file records what was reproduced, what was borrowed, and where
the two still differ.

## Reproduced from the Go source

| Module | Stands in for | What had to match |
|---|---|---|
| `ansi` | `charmbracelet/x/ansi` | `Wrap`, `StringWidth`, `Truncate`: the wrap algorithm's hard-wrap fallback, its escape awareness, and the exact column it breaks at |
| `bubbles` | `charm.land/bubbles/v2` | viewport scrolling and padding, paginator arithmetic, spinner frames and rate, text-input editing keys and its windowed view |
| `bubbletea` | `charm.land/bubbletea/v2` | `KeyPressMsg.String()` — the names *are* the key bindings — plus the alternate screen, cell-motion mouse tracking and suspend |
| `clipboard` | `termenv` + `atotto/clipboard` | the OSC 52 payload and the platform clipboard tools |
| `cobra` | `spf13/cobra` + `spf13/pflag` | the help and usage templates, flag-column alignment, `(default …)` annotations, argument validation and every error string |
| `editor` | `charmbracelet/x/editor` | the `$EDITOR` lookup with its `nano` fallback and the per-editor line-number options |
| `fuzzy` | `sahilm/fuzzy` | the scoring rules, verbatim: first-character, camel-case, separator and adjacency bonuses, and the leading-character penalty with its floor |
| `gap` | `muesli/go-app-paths` | the priority order of the configuration and cache directories on each platform |
| `gitcha` | `muesli/gitcha` | the lexical walk, `filepath.Match` globbing, and the `.gitignore` of the enclosing repository |
| `glamour` | `charm.land/glamour/v2` | the whole renderer: the style model, the cascade, block layout, and the goldmark text segmentation that decides where a styled run begins and ends |
| `humanize` | `dustin/go-humanize` | the relative-time magnitude table and `english.Plural` |
| `lipgloss` | `charm.land/lipgloss/v2` | the SGR parameter order, width and height padding, and the light/dark variant choice |
| `mango` + `roff` | `muesli/mango`, `mango-cobra`, `muesli/roff` | the man page's section order, its macros, and the "never emit a blank line" write rule |
| `shell` | `mvdan.cc/sh/v3/shell` | `Fields`: quoting, escapes, parameter expansion and IFS splitting for `$PAGER` |
| `url` | `net/url` | `Parse`, `ParseRequestURI`, `Hostname`, `JoinPath`, `ResolveReference` and `String`, including how permissive they are |
| `viper` | `spf13/viper` | the search path, the supported extensions, and the precedence flag → environment → file → default → flag default |

## Borrowed from the Rust ecosystem

* `pulldown-cmark` parses CommonMark and GFM. Its tree is then reshaped in
  `glamour::ast` to match goldmark's, because goldmark's *text segmentation* —
  which characters end a text node — decides where the renderer opens and closes
  a styled run, and that is part of the output.
* `ignore` supplies the `.gitignore` matcher behind `gitcha`.
* `crossterm` supplies raw mode, the alternate screen and the event stream that
  `bubbletea` drives.
* `notify` supplies the directory watch behind the pager's live reload.
* `unicode-normalization` supplies the NFD/NFC passes the filter normaliser
  needs; `unicode-width` and `unicode-segmentation` supply display widths.
* `ureq` performs the HTTP requests. `net/http` returns a response whatever the
  status and leaves the check to the caller, so `http::get` unwraps `ureq`'s
  4xx/5xx error back into a response.
* `serde_json`, `serde_yaml` and `chrono` parse stylesheets, configuration and
  time.

## Known differences

* **Syntax colouring inside code blocks.** Chroma's lexer corpus is not
  portable at this scale, so a code block carries the stylesheet's own colour
  rather than per-token colours. Structure, indentation and text match; only the
  colour of a token inside a fenced block differs, and only for the four styles
  that define a Chroma theme (`dark`, `light`, `dracula`, `tokyo-night`).
* **Trigger characters after the last space of a line.** goldmark ends a line's
  final text segment at the last inline-parser trigger. `` ` ``, `!`, `<` and
  `(` are triggers there but are not treated as such here: `pulldown-cmark`
  resolves backslash escapes and HTML entities before we see them, and an
  escaped `` \` `` must not end a segment while a literal one must. Whitespace,
  `[`, `]`, `*`, `_` and `~` are handled, and characters that arrived through an
  escape or an entity are shielded from all of those passes.
* **`--preserve-new-lines` on the CLI.** The Go original passes
  `glamour.WithPreservedNewLines()` unconditionally in `executeCLI`, so the flag
  only reaches the TUI. That is reproduced rather than corrected.
* **Windows.** The console ANSI shim is in place and the platform-specific code
  is behind `cfg`, but the tree has not been built or tested on Windows.
