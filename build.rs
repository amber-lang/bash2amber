fn main() {
    println!("cargo:rerun-if-changed=tests/bash/");
    println!("cargo:rerun-if-changed=tests/amber/");
}
