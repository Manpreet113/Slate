use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=templates");

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("embedded_templates.rs");
    let mut f = fs::File::create(&dest_path).unwrap();

    let mut templates = Vec::new();
    collect_templates("templates", &mut templates);

    f.write_all(b"pub static TEMPLATES: &[(&str, &str)] = &[\n")
        .unwrap();
    for (path, full_path) in templates {
        // path is relative to templates/, full_path is relative to crate root
        // We want the key to be the relative path
        let key = path.replace("\\", "/");
        let abs_path = fs::canonicalize(&full_path).unwrap();
        f.write_all(
            format!(
                "    (\"{}\", include_str!(\"{}\")),\n",
                key,
                abs_path.display()
            )
            .as_bytes(),
        )
        .unwrap();
    }
    f.write_all(b"];\n").unwrap();
}

fn collect_templates(dir: &str, templates: &mut Vec<(String, String)>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_templates(path.to_str().unwrap(), templates);
            } else {
                let full_path = path.to_str().unwrap().to_string();
                // key should be relative to "templates/"
                // e.g. templates/foo/bar.txt -> foo/bar.txt
                let key = full_path.strip_prefix("templates/").unwrap().to_string();
                templates.push((key, full_path));
            }
        }
    }
}
