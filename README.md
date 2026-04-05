# js-compiler

A JavaScript-to-native compiler written in Rust. Compiles JavaScript to machine code via LLVM, producing standalone executables.

CLAUDE AND CODEX MADE ALL OF THIS, THIS PROJECT IS FOR FUN. USE IT WITH CAUTION AND AT YOUR OWN RISK!!!

## How it works

```
JS source → oxc parser → AST → LLVM IR → clang → native executable
```

1. Parses JavaScript using [oxc](https://oxc.rs/) (a fast JS parser written in Rust)
2. Walks the AST and emits LLVM IR text
3. Links with a C runtime library (NaN-boxing based dynamic type system)
4. Invokes `clang` to compile the IR + runtime to a native executable

## Supported features

### Types (dynamic, NaN-boxed)
- Numbers (64-bit float), strings, booleans, `null`, `undefined`
- Objects (hash map), arrays (dynamic), functions/closures

### Language
- Variable declarations (`let`, `const`)
- All arithmetic (`+`, `-`, `*`, `/`, `%`) with JS semantics (string concat, type coercion)
- Comparisons (`==`, `!=`, `<`, `>`, `<=`, `>=`, `===`, `!==`)
- Logical operators (`&&`, `||`, `!`) with short-circuit evaluation
- Assignment (`=`) including to object properties (`obj.x = 5`, `arr[i] = v`)
- `if`/`else`, `while`, `for`, `for...of`
- Function declarations, function expressions, arrow functions
- Closures (captures outer variables by value)
- Template literals (`` `hello ${name}` ``)
- Ternary operator (`a ? b : c`)
- `typeof` operator
- `throw` (exits with error message)
- Object literals (`{ key: value }`)
- Array literals (`[1, 2, 3]`)
- Property access (dot and bracket notation)
- `i++`, `i--`, `++i`, `--i`

### Built-in functions
- `console.log()`, `console.error()`
- `parseInt()`, `parseFloat()`, `isNaN()`, `isFinite()`
- `Number()`, `String()`, `Boolean()`
- `prompt()` (reads from stdin)
- `typeof`

### Math
- `Math.floor`, `ceil`, `round`, `sqrt`, `abs`, `pow`, `log`, `sin`, `cos`, `tan`, `random`, `min`, `max`, `trunc`, `sign`, `exp`, `atan2`
- `Math.PI`, `Math.E`, `Math.LN2`, `Math.SQRT2`, etc.

### String methods
- `length`, `charAt`, `charCodeAt`, `indexOf`, `includes`
- `slice`, `substring`, `toUpperCase`, `toLowerCase`, `trim`
- `split`, `replace`, `repeat`, `startsWith`, `endsWith`
- `padStart`, `padEnd`

### Array methods
- `length`, `push`, `pop`, `shift`, `unshift`
- `indexOf`, `includes`, `join`, `reverse`, `slice`, `concat`
- `map`, `filter`, `reduce`, `forEach`, `find`, `findIndex`
- `every`, `some`, `flat`

### Other
- `JSON.stringify()`
- `Object.keys()`, `Object.values()`
- `Array.isArray()`
- `Date.now()`

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
cargo run -- input.js --emit-ir  # keep the .ll file for inspection

# Run the compiled program
./input.exe
```

## Example

```js
function quicksort(arr) {
  if (arr.length <= 1) return arr;
  let pivot = arr[0];
  let left = arr.slice(1).filter((x) => x <= pivot);
  let right = arr.slice(1).filter((x) => x > pivot);
  return quicksort(left).concat([pivot]).concat(quicksort(right));
}

let sorted = quicksort([38, 27, 43, 3, 9, 82, 10]);
console.log("Sorted: " + sorted.join(", "));
// prints: Sorted: 3, 9, 10, 27, 38, 43, 82
```

```sh
$ cargo run -- sort.js
$ ./sort.exe
Sorted: 3, 9, 10, 27, 38, 43, 82
```
