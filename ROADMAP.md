# Roadmap: Toward a Working JS Compiler

Current state: compiles numeric expressions, functions, control flow, and `console.log` to native executables. Everything is `f64`. No heap, no strings-as-values, no objects.

This roadmap outlines what's needed to support real-world JS patterns like `fetch()`, `prompt()`, string manipulation, arrays, and objects.

---

## Phase 1: Dynamic Type System

**The biggest blocker.** Right now every value is a `f64`. Real JS needs dynamic types.

- [ ] **Tagged value representation** — Implement a runtime value type (NaN-boxing or tagged union struct) that can hold: `number`, `string`, `boolean`, `null`, `undefined`, `object pointer`
- [ ] **Runtime type checking** — Emit type checks before operations (`+` on two numbers vs string concatenation)
- [ ] **Type coercion** — Implement JS coercion rules (`==` vs `===`, `"5" + 3 = "53"`, etc.)
- [ ] **Truthiness** — Proper truthy/falsy for all types (empty string, `null`, `undefined`, `0`, `NaN` are falsy)

## Phase 2: Strings as Values

Required for almost everything beyond math.

- [ ] **Heap-allocated strings** — Strings as reference-counted or GC'd heap objects
- [ ] **String concatenation** — `+` operator with string operands
- [ ] **String methods** — `.length`, `.slice()`, `.indexOf()`, `.includes()`, `.split()`, `.trim()`, `.toUpperCase()`, `.toLowerCase()`
- [ ] **Template literals** — `` `Hello ${name}` ``
- [ ] **String comparison** — `==`, `<`, `>` for strings
- [ ] **typeof operator** — Returns string like `"number"`, `"string"`, `"object"`, etc.

## Phase 3: Memory Management

Once we have heap-allocated values, we need to manage memory.

- [ ] **Reference counting** — Simplest approach; add refcount to heap objects, free when count hits 0
- [ ] **Or: simple GC** — Mark-and-sweep garbage collector (more correct for cycles, more complex)
- [ ] **Runtime allocator** — Small runtime library (in C or Rust) linked into compiled programs that handles `malloc`/`free`/GC

## Phase 4: Objects and Arrays

Core JS data structures.

- [ ] **Object literals** — `{ key: value }` as hash maps
- [ ] **Property access** — `obj.key` and `obj["key"]`
- [ ] **Property assignment** — `obj.key = value`
- [ ] **Arrays** — `[1, 2, 3]` with dynamic resizing
- [ ] **Array methods** — `.push()`, `.pop()`, `.length`, `.map()`, `.filter()`, `.forEach()`, `.reduce()`, `.join()`
- [ ] **for...of loops** — Iterate over arrays
- [ ] **Spread operator** — `[...arr]`, `{...obj}`
- [ ] **Destructuring** — `const { a, b } = obj`, `const [x, y] = arr`
- [ ] **JSON.stringify / JSON.parse**

## Phase 5: Closures and First-Class Functions

JS functions capture their environment.

- [ ] **Function expressions** — `const add = function(a, b) { return a + b; }`
- [ ] **Arrow functions** — `(a, b) => a + b`
- [ ] **Closures** — Capture variables from enclosing scope (requires heap-allocated activation records)
- [ ] **Callbacks** — Pass functions as arguments
- [ ] **Higher-order functions** — Functions returning functions
- [ ] **`this` binding** — Basic `this` semantics (at least for method calls)

## Phase 6: Error Handling

- [ ] **try / catch / finally** — Implement using LLVM's exception handling or setjmp/longjmp
- [ ] **throw** — Throw any value
- [ ] **Error objects** — `new Error("message")` with `.message` and `.stack`

## Phase 7: Built-in Functions and I/O Runtime

This is where `prompt()`, `fetch()`, etc. come in. These require a **runtime library** linked into every compiled program.

### Synchronous built-ins
- [ ] **prompt(message)** — Read line from stdin (link to C `fgets` or Rust `std::io::stdin`)
- [ ] **parseInt() / parseFloat()** — String to number conversion
- [ ] **Math object** — `Math.floor`, `Math.ceil`, `Math.round`, `Math.random`, `Math.sqrt`, `Math.pow`, `Math.abs`, `Math.min`, `Math.max`, `Math.PI`
- [ ] **String() / Number() / Boolean()** — Type conversion functions
- [ ] **isNaN() / isFinite()**
- [ ] **console.error()** — Print to stderr
- [ ] **Date.now()** — Millisecond timestamp

### Async built-ins (requires Phase 8 first)
- [ ] **setTimeout / setInterval** — Timer-based callbacks
- [ ] **fetch()** — HTTP requests (link to libcurl or a Rust HTTP client compiled as a static lib)

## Phase 8: Async / Await

Required for `fetch()` and modern JS patterns. This is the hardest phase.

- [ ] **Promises** — Basic Promise implementation with `.then()`, `.catch()`
- [ ] **async functions** — Compile to state machines (like Rust's async or C# coroutines)
- [ ] **await expressions** — Suspend and resume execution
- [ ] **Event loop** — A minimal event loop runtime for scheduling async work and timers

**Approach options:**
1. **State machine transform** — Compile each `async` function into a state machine (like what Rust and TypeScript compilers do). Complex but correct.
2. **Stackful coroutines** — Use platform fibers/coroutines. Simpler codegen but platform-specific.
3. **Link a runtime** — Use Tokio or libuv compiled as a C library for the event loop.

## Phase 9: Module System

- [ ] **import / export** — ES module syntax
- [ ] **Multiple file compilation** — Compile and link multiple JS files
- [ ] **Standard library modules** — Bundle built-in modules

## Phase 10: Polish and Compatibility

- [ ] **Source maps** — Map compiled code back to JS source for debugging
- [ ] **Better error messages** — Line numbers and context in compile errors
- [ ] **Tail call optimization** — For recursive functions
- [ ] **Cross-platform** — macOS and Linux support (currently Windows-only)
- [ ] **Test suite** — Automated test runner against expected outputs
- [ ] **Benchmarks** — Compare performance vs Node.js / Bun / Deno

---

## Suggested implementation order

```
Phase 1 (types)  ─→  Phase 2 (strings)  ─→  Phase 3 (memory)
                                                    │
                 Phase 5 (closures)  ←──────────────┤
                                                    │
                 Phase 4 (objects/arrays)  ←─────────┘
                        │
                 Phase 6 (errors)
                        │
                 Phase 7 sync builtins (prompt, Math, etc.)
                        │
                 Phase 8 (async/await)
                        │
                 Phase 7 async builtins (fetch, timers)
                        │
                 Phase 9 (modules)  →  Phase 10 (polish)
```

**The critical path is Phase 1 → 2 → 3.** Once you have a dynamic type system with heap-allocated strings and memory management, everything else builds on top incrementally.

## Architecture note: the runtime library

Phases 3+ require a **runtime library** — a small C or Rust library compiled to a static `.lib`/`.a` that gets linked into every compiled JS program. It would provide:

- Memory allocator / garbage collector
- Tagged value operations (type checks, coercion)
- Built-in function implementations (prompt, fetch, Math, etc.)
- String operations
- Object/array hash map implementation
- Event loop (for async)

This is similar to how Go, Zig, and other compiled languages ship a runtime. The compiler emits calls to runtime functions, and the runtime handles the complex parts.
