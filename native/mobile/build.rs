// The shared engine sources switch their desktop-only parts off with `cfg(liemo_mobile)`.
fn main() {
    println!("cargo:rustc-cfg=liemo_mobile");
}
