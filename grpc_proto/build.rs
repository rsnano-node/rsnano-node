fn main() -> Result<(), Box<dyn std::error::Error>> {
    let descriptor_path =
        std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("file_descriptor_set.bin");

    tonic_build::configure()
        .file_descriptor_set_path(&descriptor_path)
        .compile_protos(
            &["proto/nano/v1/types.proto", "proto/nano/v1/services.proto"],
            &["proto/"],
        )?;
    Ok(())
}
