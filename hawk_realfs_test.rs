use std::path::Path;
fn main() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let p = "/Users/rickray/Library/Application Support/Hawk/extensions/installed/vscode-icons/extension.toml";
        let exists = tokio::fs::metadata(p).await.is_ok();
        println!("Tokio fs exists: {}", exists);
    });
}
