fn main() {
    // A vendored protoc, so `git clone && cargo build` works with no system
    // protobuf-compiler on any platform in the release matrix.
    std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path().unwrap());

    let proto_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../protos");
    let proto = format!("{proto_dir}/service.proto");
    println!("cargo:rerun-if-changed={proto}");

    tonic_build::configure()
        // The server half is generated so integration tests can stand up a
        // mock backend and exercise the real wire (tests/grpc_roundtrip_tests.rs).
        // It is unused by the app itself, which is only ever the client.
        .build_server(true)
        // Serde on every message, so the nodea-infer harness and the inference
        // contract tests can read and write requests as JSON using the proto
        // field names. `default` lets partial JSON parse.
        .message_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .message_attribute(".", "#[serde(default)]")
        .enum_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        // service.proto uses proto3 `optional` fields for sampling params.
        // Older protoc versions (e.g. on CI) gate these behind this flag.
        .protoc_arg("--experimental_allow_proto3_optional")
        .compile(&[proto.as_str()], &[proto_dir])
        .unwrap_or_else(|e| panic!("protobuf compile error: {}", e));
}
