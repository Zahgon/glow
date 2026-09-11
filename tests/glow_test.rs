//! The original `glow_test.go`, carried over.

use glow::glow::Glow;

/// Builds a Glow whose configuration search cannot reach the real one.
///
/// The original asserts against process-global state that `rootCmd.ParseFlags`
/// writes into, so the cases share one command tree and run in order; giving
/// each test its own tree is what keeps them from interfering.
fn root_cmd(dir: &std::path::Path) -> Glow {
    std::env::set_var("GLOW_CONFIG_HOME", dir);
    let glow = Glow::new();
    std::env::remove_var("GLOW_CONFIG_HOME");
    glow
}

/// One row of the table: the flags to parse and the state they should leave.
type Case = (Vec<String>, Box<dyn Fn(&Glow) -> bool>);

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn test_glow_flags() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let mut root = root_cmd(dir.path());

    // Table-driven over three flag parses, asserting the parsed state after
    // each one.
    let cases: Vec<Case> = vec![
        (args(&["-p"]), Box::new(|g: &Glow| g.pager)),
        (
            args(&["-s", "light"]),
            Box::new(|g: &Glow| g.style == "light"),
        ),
        (args(&["-w", "40"]), Box::new(|g: &Glow| g.width == 40)),
    ];

    for (argv, check) in cases {
        root.parse_flags(&argv).unwrap_or_else(|e| panic!("{e}"));
        assert!(check(&root), "Parsing flag failed: {argv:?}");
    }
}
