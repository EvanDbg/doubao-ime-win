use std::io::Result;

fn main() -> Result<()> {
    // Compile protobuf files
    prost_build::compile_protos(&["proto/asr.proto"], &["proto/"])?;
    
    // Tell Cargo to rerun if the proto file changes
    println!("cargo:rerun-if-changed=proto/asr.proto");
    
    Ok(())
}
