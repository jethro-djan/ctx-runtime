use context_runtime::workspace::Workspace;
use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();
    let filepath = if args.len() > 1 {
        &args[1]
    } else {
        eprintln!("Usage: cargo run --example debug <file.tex>");
        std::process::exit(1);
    };

    let source = fs::read_to_string(filepath).expect("Failed to read file");
    let mut workspace = Workspace::new();

    let uri = "main.tex";
    if !workspace.open(uri, &source) {
        eprintln!("Failed to parse document");
        std::process::exit(1);
    }

    println!("AST:");
    if let Some(ast) = workspace.ast(uri) {
        println!("{:#?}", ast);
    }

    println!("\nHighlights:");
    if let Some(highlights) = workspace.highlights(uri) {
        for h in highlights {
            println!("{:?} => {:?}", h.kind, h.range);
        }
    }
}

