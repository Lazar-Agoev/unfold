//! Sets the linker up for a Node-API addon (on macOS, Node's symbols resolve at load time).

fn main() {
    napi_build::setup();
}
