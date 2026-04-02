# js-compiler

A JavaScript-to-native compiler written in Rust. Compiles a subset of JavaScript directly to machine code via LLVM.

## How it works

```
JS source → oxc parser → AST → LLVM IR → clang → native executable
```

1. Parses JavaScript using [oxc](https://oxc.rs/) (a fast JS parser written in Rust)
2. Walks the AST and emits LLVM IR as text
3. Invokes `clang` to compile the IR to a native executable

## Supported features

- Variable declarations (`let`, `const`)
- Arithmetic (`+`, `-`, `*`, `/`, `%`)
- Comparisons (`==`, `!=`, `<`, `>`, `<=`, `>=`, `===`, `!==`)
- Logical operators (`&&`, `||`, `!`) with short-circuit evaluation
- Assignment (`=`)
- `if`/`else` statements
- `while` and `for` loops
- Function declarations and calls (including recursion)
- `console.log()` for output (numbers and string literals)
- Update expressions (`i++`, `i--`, `++i`, `--i`)
- Boolean literals (`true`/`false`)

All numeric values are represented as 64-bit floats (like JavaScript).

## Prerequisites

- Rust toolchain
- LLVM/clang installed and on your PATH

## Usage

```sh
# Build the compiler
cargo build --release

# Compile a JS file to a native executable
cargo run -- input.js            # produces input.exe
cargo run -- input.js -o out.exe # custom output name

# Run the compiled program
./input.exe
```

## Example

```js
function fib(n) {
  if (n <= 1) { return n; }
  return fib(n - 1) + fib(n - 2);
}

console.log(fib(10)); // prints 55
```

```sh
$ cargo run -- fib.js
$ ./fib.exe
55
```
