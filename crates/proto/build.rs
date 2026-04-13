use std::io::Result;
use std::path::PathBuf;

fn main() -> Result<()> {
    // Tell cargo to rerun if proto files change
    println!("cargo:rerun-if-changed=proto/");

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let descriptor_path = out_dir.join("descriptor.bin");

    // Configure prost-build to generate descriptor file
    let mut config = prost_build::Config::new();
    config.file_descriptor_set_path(&descriptor_path);

    // Compile the proto files with prost
    // Note: pbjson will add serde Serialize/Deserialize traits automatically
    config.compile_protos(
        &[
            "proto/ledger/v1/http.proto",
            "proto/position_manager/v1/rpc.proto",
            "proto/user_service/v1/http.proto",
        ],
        &["proto/"]
    )?;

    // Read the descriptor file
    let descriptor_set = std::fs::read(&descriptor_path)?;

    // Generate pbjson code for JSON serialization with snake_case field names
    pbjson_build::Builder::new()
        .register_descriptors(&descriptor_set)?
        .preserve_proto_field_names()
        .build(&[".ledger.v1", ".position_manager.v1", ".user_service.v1"])?;

    Ok(())
}
