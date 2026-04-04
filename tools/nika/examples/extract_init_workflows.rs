//! Extract init workflows to disk for testing
//!
//! Usage: cargo run --example extract_init_workflows

use std::fs;
use std::path::Path;

fn main() {
    let base = Path::new("/tmp/nika-test-workflows");

    // Clean and recreate
    if base.exists() {
        fs::remove_dir_all(base).unwrap();
    }

    let workflows = nika_init::get_all_workflows();
    let context_files = nika_init::get_all_context_files();
    let schemas = nika_init::get_all_schemas();

    // Write workflows
    for w in &workflows {
        let dir = base.join("workflows").join(w.tier_dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(w.filename);
        fs::write(&path, w.content).unwrap();
        println!("  {}", path.display());
    }

    // Write context files
    for f in &context_files {
        let dir = base.join("workflows").join(f.dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(f.filename);
        fs::write(&path, f.content).unwrap();
        println!("  {}", path.display());
    }

    // Write schemas
    for f in &schemas {
        let dir = base.join("workflows").join(f.dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(f.filename);
        fs::write(&path, f.content).unwrap();
        println!("  {}", path.display());
    }

    // Write README
    let readme_path = base.join("workflows/README.md");
    fs::write(&readme_path, nika_init::WORKFLOWS_README).unwrap();
    println!("  {}", readme_path.display());

    println!(
        "\nExtracted {} workflows, {} context, {} schemas",
        workflows.len(),
        context_files.len(),
        schemas.len()
    );
}
