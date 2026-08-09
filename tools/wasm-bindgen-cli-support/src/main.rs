use std::env;
use std::error::Error;
use std::path::{Path, PathBuf};

use wasm_bindgen_cli_support::Bindgen;

fn required_path(
    args: &mut impl Iterator<Item = String>,
    label: &str,
) -> Result<PathBuf, Box<dyn Error>> {
    args.next()
        .map(PathBuf::from)
        .ok_or_else(|| format!("missing {label}").into())
}

fn output_name(input: &Path) -> Result<&str, Box<dyn Error>> {
    input
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .ok_or_else(|| "input must have a UTF-8 file stem".into())
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let input = required_path(&mut args, "input .wasm path")?;
    let output = required_path(&mut args, "output directory")?;
    if args.next().is_some() {
        return Err("usage: ade-wasm-bindgen-tool <input.wasm> <output-directory>".into());
    }
    if input.extension().and_then(|extension| extension.to_str()) != Some("wasm") {
        return Err("input path must end in .wasm".into());
    }

    let name = output_name(&input)?;
    let mut bindgen = Bindgen::new();
    bindgen.input_path(&input).out_name(name).typescript(false);
    bindgen.web(true)?;
    bindgen.generate(&output)?;
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("ade-wasm-bindgen-tool: {error}");
        std::process::exit(2);
    }
}
