//! Golden tests for the ANSI renderer.
//!
//! Go delegated the whole of the rendering engine to `glamour`, so nothing in
//! the original suite covered it. These files are the bytes upstream glamour
//! produces for the same documents; the port has to match them exactly,
//! trailing padding included.

use glow::deps::glamour::{render, styles};

const BASE_URL: &str = "https://example.com/docs/";
const WIDTH: i64 = 80;

fn render_file(name: &str, style: &str) -> String {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/testdata");
    let source = std::fs::read_to_string(dir.join(format!("{name}.md")))
        .unwrap_or_else(|e| panic!("{name}.md: {e}"));
    let opts = render::Options {
        base_url: BASE_URL.to_string(),
        word_wrap: WIDTH,
        preserve_new_lines: true,
        styles: styles::default_style(style).expect("a built-in style"),
        ..Default::default()
    };
    render::Renderer::new(opts).render(&source)
}

fn golden(name: &str, style: &str) -> String {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/testdata");
    std::fs::read_to_string(dir.join(format!("{name}.{style}.golden")))
        .unwrap_or_else(|e| panic!("{name}.{style}.golden: {e}"))
}

fn assert_golden(name: &str, style: &str) {
    let got = render_file(name, style);
    let want = golden(name, style);
    assert_eq!(
        got, want,
        "{name} rendered with {style} does not match the golden file"
    );
}

macro_rules! golden_tests {
    ($($doc:ident),* $(,)?) => {
        $(
            mod $doc {
                #[test]
                fn notty() {
                    super::assert_golden(stringify!($doc), "notty");
                }
                #[test]
                fn ascii() {
                    super::assert_golden(stringify!($doc), "ascii");
                }
                #[test]
                fn pink() {
                    super::assert_golden(stringify!($doc), "pink");
                }
            }
        )*
    };
}

golden_tests!(
    brackets,
    code,
    deflist,
    frontmatter,
    headings,
    html,
    links,
    lists,
    quote,
    table,
    text,
    tiny
);

#[test]
fn word_wrap_of_zero_disables_wrapping_and_padding() {
    let opts = render::Options {
        word_wrap: 0,
        styles: styles::default_style("notty").expect("a built-in style"),
        ..Default::default()
    };
    let out = render::Renderer::new(opts).render("a b c\n");
    // The document's own margin still applies; only wrapping and the padding
    // that fills each line out to the width go away.
    assert_eq!(out, "\n  a b c\n\n");
}

#[test]
fn every_built_in_style_renders() {
    for name in styles::DEFAULT_STYLE_NAMES {
        let opts = render::Options {
            word_wrap: WIDTH,
            styles: styles::default_style(name).expect("a built-in style"),
            ..Default::default()
        };
        let out = render::Renderer::new(opts).render("# H\n\ntext\n");
        assert!(out.contains("H"), "{name}");
        assert!(out.contains("text"), "{name}");
    }
}
