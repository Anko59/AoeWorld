#[path = "build/shader.rs"]
mod shader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    shader::generate()
}
