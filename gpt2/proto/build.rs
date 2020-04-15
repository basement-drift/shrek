fn main() {
    tonic_build::compile_protos("./gpt2.proto").unwrap();
}
