use cost_check::{render_json, Tool};

#[test]
fn ci_page_matches_the_example() {
    let json = include_bytes!("fixtures/ci.json");
    let md = include_str!("fixtures/ci.md");
    let page = render_json(Tool::Ci, json, Some(sample_diff(md))).expect("rapport");
    assert_eq!(page, md);
}

#[test]
fn cargo_has_no_network_crate() {
    let toml = include_str!("../Cargo.toml");
    for name in ["reqwest", "hyper", "ureq", "tokio", "curl"] {
        assert!(!toml.contains(name), "{name} hoort niet in Cargo.toml");
    }
}

fn sample_diff(md: &str) -> &str {
    let marker = "```diff\n";
    let start = md.find(marker).expect("diff") + marker.len();
    let end = md[start..].find("\n```").expect("einde") + start;
    &md[start..end]
}
