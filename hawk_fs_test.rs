use std::path::Path;
fn main() {
    let p = "/Users/rickray/Library/Application Support/Hawk/extensions/installed/vscode-icons/extension.toml";
    let p = Path::new(p);
    println!("File exists: {}", p.is_file());
}
