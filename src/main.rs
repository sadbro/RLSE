mod utils;
use crate::utils as locals;

fn build() {
    let root_dir_path = vec!["data", "docs.gl-mainline"];
    let dir_paths = vec!["es1", "es2", "es3", "el3", "gl2", "gl3", "gl4", "sl4"];
    locals::save_index_to_file(root_dir_path, dir_paths, "index.json");
}

fn main() {
    locals::serve("127.0.0.1:5000", "index.json");
}