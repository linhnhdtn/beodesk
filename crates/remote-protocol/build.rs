fn main() {
    println!("cargo:rerun-if-changed=proto/control.proto");
    let mut config = prost_build::Config::new();
    config.protoc_executable(protoc_bin_vendored::protoc_bin_path().expect("bundled protoc"));
    config
        .compile_protos(&["proto/control.proto"], &["proto"])
        .expect("compile BeoDesk control schema");
}
