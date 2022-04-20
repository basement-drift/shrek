# GPT2 gRPC Server and Client
This set of crates exists to limit the amount of code that needs to run in a specialized Dockerfile
with the `ROCm` stack. As such, it is a slightly unusual configuration for a `tonic` gRPC project.
These crates are also in a separate workspace to facilitate the docker build.

## proto
This crate is nothing but the proto file itself and the export of the generated code.

## server
This crate has the actual logic of the server, and is the only crate with a dependency on
`rust_bert`.  This allows us to depend on the client code from the wider project without having a
dependency on the same libtorch version as `rust_bert`.

## client
This crate is nothing but a re-export of the client and data structs from the `proto` crate.
