#[cfg(feature = "gui")]
fn main() {
    llm_wiki_desktop_lib::run();
}

#[cfg(not(feature = "gui"))]
fn main() {
    panic!("GUI feature must be enabled to run the desktop application. Use: cargo run --features gui");
}
