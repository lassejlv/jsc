use std::time::Instant;
use std::{env, fs, path::Path, process::Command};

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

mod codegen;

const RUNTIME_SRC: &str = concat!(
    include_str!("../runtime/js_types.c"),
    include_str!("../runtime/js_strings.c"),
    include_str!("../runtime/js_objects.c"),
    include_str!("../runtime/js_core.c"),
    include_str!("../runtime/js_methods.c"),
    include_str!("../runtime/js_builtins.c"),
    include_str!("../runtime/js_json.c"),
    include_str!("../runtime/js_operators.c"),
    include_str!("../runtime/js_fetch.c"),
    include_str!("../runtime/js_init.c"),
);
const VERSION: &str = env!("CARGO_PKG_VERSION");

// ANSI color helpers
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";
const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const CYAN: &str = "\x1b[36m";

fn print_help() {
    eprintln!(
        r#"
{BOLD}{CYAN}  js-compiler{RESET} {DIM}v{VERSION}{RESET}
  {DIM}Compile JavaScript to native executables{RESET}

{BOLD}USAGE{RESET}
  js-compiler <input.js> [options]

{BOLD}OPTIONS{RESET}
  {CYAN}-o <file>{RESET}      Output file path {DIM}(default: <input> without .js){RESET}
  {CYAN}--emit-ir{RESET}      Keep the generated LLVM IR (.ll) file
  {CYAN}--help{RESET}         Show this help message
  {CYAN}--version{RESET}      Print version

{BOLD}EXAMPLES{RESET}
  {DIM}${RESET} js-compiler app.js
  {DIM}${RESET} js-compiler app.js -o myapp
  {DIM}${RESET} js-compiler app.js --emit-ir
"#
    );
}

fn step(num: u8, msg: &str) {
    eprint!("  {DIM}[{num}/4]{RESET} {msg}");
}

fn done(detail: &str) {
    eprintln!(" {GREEN}{detail}{RESET}");
}

fn fail(msg: &str) -> ! {
    eprintln!("\n  {RED}{BOLD}error:{RESET} {msg}");
    std::process::exit(1);
}

fn format_size(bytes: u64) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

fn format_duration(ms: f64) -> String {
    if ms >= 1000.0 {
        format!("{:.2}s", ms / 1000.0)
    } else {
        format!("{:.0}ms", ms)
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 || args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        std::process::exit(if args.len() < 2 { 1 } else { 0 });
    }

    if args.iter().any(|a| a == "--version" || a == "-V") {
        eprintln!("js-compiler {VERSION}");
        std::process::exit(0);
    }

    let input_path = &args[1];
    let output_path = if args.len() >= 4 && args[2] == "-o" {
        args[3].clone()
    } else {
        let base = input_path.replace(".js", "");
        if cfg!(target_os = "windows") {
            format!("{}.exe", base)
        } else {
            base
        }
    };
    let keep_ir = args.iter().any(|a| a == "--emit-ir");

    let input_name = Path::new(input_path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let output_name = Path::new(&output_path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();

    eprintln!();
    eprintln!("  {BOLD}{CYAN}js-compiler{RESET} {DIM}v{VERSION}{RESET}");
    eprintln!("  {DIM}{input_name} -> {output_name}{RESET}");
    eprintln!();

    let total_start = Instant::now();

    // 1. Parse
    step(1, "Parsing...");
    let parse_start = Instant::now();

    let source = fs::read_to_string(input_path).unwrap_or_else(|e| {
        eprintln!();
        fail(&format!("Cannot read {input_path}: {e}"));
    });

    let allocator = Allocator::default();
    let source_type = SourceType::mjs();
    let ret = Parser::new(&allocator, &source, source_type).parse();

    if !ret.errors.is_empty() {
        eprintln!();
        for err in &ret.errors {
            eprintln!("  {RED}{BOLD}parse error:{RESET} {err}");
        }
        eprintln!();
        std::process::exit(1);
    }

    let lines = source.lines().count();
    done(&format!(
        "{lines} lines in {}",
        format_duration(parse_start.elapsed().as_secs_f64() * 1000.0)
    ));

    // 2. Codegen
    step(2, "Generating IR...");
    let codegen_start = Instant::now();
    let ir = codegen::CodeGen::compile(&ret.program);
    done(&format!(
        "{} bytes in {}",
        ir.len(),
        format_duration(codegen_start.elapsed().as_secs_f64() * 1000.0)
    ));

    // 3. Write temp files
    step(3, "Writing artifacts...");
    let ir_path = input_path.replace(".js", ".ll");
    fs::write(&ir_path, &ir).unwrap_or_else(|e| fail(&format!("Cannot write IR: {e}")));

    let rt_path = Path::new(&ir_path)
        .parent()
        .unwrap_or(Path::new("."))
        .join("__js_runtime.c");
    fs::write(&rt_path, RUNTIME_SRC)
        .unwrap_or_else(|e| fail(&format!("Cannot write runtime: {e}")));
    done("ok");

    // 4. Compile with clang
    step(4, "Compiling native...");
    let clang_start = Instant::now();

    let mut clang_args = vec![
        rt_path.to_str().unwrap().to_string(),
        ir_path.clone(),
        "-o".to_string(),
        output_path.clone(),
        "-O2".to_string(),
        "-Wno-override-module".to_string(),
    ];
    clang_args.push("-lcurl".to_string());
    if cfg!(target_os = "linux") {
        clang_args.push("-lm".to_string());
    }

    let status = Command::new("clang")
        .args(&clang_args)
        .status()
        .unwrap_or_else(|e| {
            eprintln!();
            fail(&format!(
                "Cannot invoke clang: {e}\n         Make sure LLVM/clang is installed and on your PATH."
            ));
        });

    if !status.success() {
        eprintln!();
        eprintln!("  {DIM}IR kept at {ir_path}{RESET}");
        let _ = fs::remove_file(&rt_path);
        fail("clang compilation failed");
    }

    let bin_size = fs::metadata(&output_path)
        .map(|m| m.len())
        .unwrap_or(0);
    done(&format!(
        "{} in {}",
        format_size(bin_size),
        format_duration(clang_start.elapsed().as_secs_f64() * 1000.0)
    ));

    // Cleanup
    let _ = fs::remove_file(&rt_path);
    if !keep_ir {
        let _ = fs::remove_file(&ir_path);
    }

    let total_ms = total_start.elapsed().as_secs_f64() * 1000.0;
    eprintln!();
    eprintln!(
        "  {GREEN}{BOLD}Done{RESET} in {BOLD}{}{RESET} -> {BOLD}{output_name}{RESET}",
        format_duration(total_ms)
    );

    if keep_ir {
        let ir_name = Path::new(&ir_path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        eprintln!("  {DIM}IR saved to {ir_name}{RESET}");
    }
    eprintln!();
}
