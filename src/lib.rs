pub mod amber;
pub mod bash;

pub fn convert_bash_to_amber(source: &str, path: Option<String>) -> Result<String, String> {
    let program = bash::parser::parse(source, path)?;
    Ok(amber::render::render_program(&program))
}
