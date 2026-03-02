use std::fs::File;
use std::io::Read;
use std::path::Path;

fn main() {
    let tar_gz = File::open("/Users/rickray/Dev/hawk/tmp_dir/vscode-icons.tar.gz").unwrap();
    let decompressed = flate2::read::GzDecoder::new(tar_gz);
    let mut archive = tar::Archive::new(decompressed);
    let dest = Path::new("/Users/rickray/Dev/hawk/tmp_dir/vscode-icons-unpacked");
    let _ = std::fs::remove_dir_all(dest);
    std::fs::create_dir_all(dest).unwrap();
    archive.unpack(dest).unwrap();

    let manifest_path = dest.join("extension.toml");
    println!("Exists: {}", manifest_path.exists());
    println!("Is file: {}", manifest_path.is_file());
}
