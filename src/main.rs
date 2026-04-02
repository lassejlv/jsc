use std::{env, fs, process::Command};

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

mod codegen;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: js-compiler <input.js> [-o output]");
        std::process::exit(1);
    }

    let input_path = &args[1];
    let output_path = if args.len() >= 4 && args[2] == "-o" {
        args[3].clone()
    } else {
        input_path.replace(".js", ".exe")
    };

    // Read source file
    let source = fs::read_to_string(input_path).unwrap_or_else(|e| {
        eprintln!("Error reading {}: {}", input_path, e);
        std::process::exit(1);
    });

    // Parse with oxc
    let allocator = Allocator::default();
    let source_type = SourceType::mjs();
    let ret = Parser::new(&allocator, &source, source_type).parse();

    if !ret.errors.is_empty() {
        for err in &ret.errors {
            eprintln!("Parse error: {err}");
        }
        std::process::exit(1);
    }

    // Generate LLVM IR
    let ir = codegen::CodeGen::compile(&ret.program);

    // Write IR to temp file
    let ir_path = input_path.replace(".js", ".ll");
    fs::write(&ir_path, &ir).unwrap_or_else(|e| {
        eprintln!("Error writing IR file: {}", e);
        std::process::exit(1);
    });
    eprintln!("Generated LLVM IR: {}", ir_path);

    // Compile with clang
    let status = Command::new("clang")
        .args([&ir_path, "-o", &output_path, "-O2", "-Wno-override-module"])
        .status()
        .unwrap_or_else(|e| {
            eprintln!("Failed to invoke clang: {}", e);
            eprintln!("Make sure LLVM/clang is installed and on your PATH.");
            std::process::exit(1);
        });

    if !status.success() {
        eprintln!("clang compilation failed (IR file kept at {})", ir_path);
        std::process::exit(1);
    }

    // Clean up IR file
    let _ = fs::remove_file(&ir_path);

    eprintln!("Compiled successfully: {}", output_path);
}
