//! Build-time UniFFI scaffolding generation.

fn main() {
    println!("cargo:rerun-if-changed=src/speech_clerk.udl");

    if let Err(error) = uniffi::generate_scaffolding("src/speech_clerk.udl") {
        eprintln!("failed to generate UniFFI scaffolding: {error}");
        std::process::exit(1);
    }
}
