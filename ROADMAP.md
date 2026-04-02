# Roadmap: Toward a Working JS Compiler

Current state: compiles JavaScript to native executables via LLVM IR. Phases 1–7 (sync) are complete: NaN-boxed dynamic types, strings (27+ methods), objects, arrays (20+ methods), closures, `this` binding, try/catch/finally, destructuring, spread, JSON.parse/stringify, and all synchronous builtins. Cross-platform (macOS, Linux, Windows).

Remaining work: async/await, modules, and polish.

---

## Phase 1: Dynamic Type System ✅

- [x] **Tagged value representation** — NaN-boxing: numbers as raw IEEE 754 doubles, booleans/null/undefined/strings/objects/arrays/functions encoded in NaN space
- [x] **Runtime type checking** — Type checks before operations (e.g. `+` dispatches to addition vs string concatenation)
- [x] **Type coercion** — JS coercion rules (`==` vs `===`, `"5" + 3 = "53"`, toNumber, toBoolean, toString)
- [x] **Truthiness** — Proper truthy/falsy for all types (empty string, `null`, `undefined`, `0`, `NaN` are falsy)

## Phase 2: Strings as Values ✅

- [x] **Heap-allocated strings** — `JSString` struct with reference counting
- [x] **String concatenation** — `+` operator with string operands
- [x] **String methods** — `.length`, `.charAt()`, `.charCodeAt()`, `.indexOf()`, `.includes()`, `.slice()`, `.substring()`, `.split()`, `.trim()`, `.toUpperCase()`, `.toLowerCase()`, `.replace()`, `.repeat()`, `.startsWith()`, `.endsWith()`, `.padStart()`, `.padEnd()`, and more (27+ methods)
- [x] **Template literals** — `` `Hello ${name}` ``
- [x] **String comparison** — `==`, `<`, `>` for strings
- [x] **typeof operator** — Returns `"number"`, `"string"`, `"boolean"`, `"object"`, `"function"`, `"undefined"`

## Phase 3: Memory Management ✅

- [x] **Reference counting** — `JSString` has refcount, freed when count hits 0
- [ ] **Cycle-safe GC** — Mark-and-sweep garbage collector (for circular references)
- [x] **Runtime allocator** — C runtime library (`runtime.c`) linked into every compiled program, uses malloc/free

## Phase 4: Objects and Arrays ✅

- [x] **Object literals** — `{ key: value }` as hash maps (FNV-1a hashing, linear probing)
- [x] **Property access** — `obj.key` and `obj["key"]`
- [x] **Property assignment** — `obj.key = value` and `obj["key"] = value`
- [x] **Arrays** — `[1, 2, 3]` with dynamic resizing
- [x] **Array methods** — `.push()`, `.pop()`, `.shift()`, `.unshift()`, `.length`, `.indexOf()`, `.includes()`, `.join()`, `.reverse()`, `.slice()`, `.concat()`, `.map()`, `.filter()`, `.forEach()`, `.reduce()`, `.find()`, `.findIndex()`, `.every()`, `.some()`, `.flat()`
- [x] **for...of loops** — Iterate over arrays
- [x] **Spread operator** — `[...arr]`, `{...obj}`
- [x] **Destructuring** — `const { a, b } = obj`, `const [x, y] = arr`, nested, defaults, rest elements, function parameters
- [x] **JSON.stringify** — Implemented
- [x] **JSON.parse** — Full recursive descent parser with string escapes, nested objects/arrays
- [x] **Object.keys() / Object.values()** — Implemented
- [x] **Array.isArray()** — Implemented

## Phase 5: Closures and First-Class Functions ✅

- [x] **Function expressions** — `const add = function(a, b) { return a + b; }`
- [x] **Arrow functions** — `(a, b) => a + b`
- [x] **Closures** — Capture variables from enclosing scope by value into heap-allocated closure environment
- [x] **Callbacks** — Pass functions as arguments
- [x] **Higher-order functions** — Functions returning functions
- [x] **`this` binding** — `this` in method calls via a runtime this-stack, user-defined methods on objects

## Phase 6: Error Handling ✅

- [x] **try / catch / finally** — Full implementation using setjmp/longjmp, supports catch with/without parameter, finally blocks, nesting
- [x] **throw** — Throw any value (implemented via setjmp/longjmp)
- [x] **Error objects** — `new Error("message")` with `.message` and `.name`
- [ ] **Stack traces** — `.stack` property on Error objects

## Phase 7: Built-in Functions and I/O Runtime

### Synchronous built-ins ✅
- [x] **prompt(message)** — Read line from stdin
- [x] **parseInt() / parseFloat()** — String to number conversion
- [x] **Math object** — `Math.floor`, `Math.ceil`, `Math.round`, `Math.random`, `Math.sqrt`, `Math.pow`, `Math.abs`, `Math.min`, `Math.max`, `Math.PI`, `Math.E`, `Math.LN2`, `Math.LN10`, `Math.SQRT2`, `Math.LOG2E`, `Math.LOG10E`, `Math.sin`, `Math.cos`, `Math.tan`, `Math.atan2`, `Math.exp`, `Math.trunc`, `Math.sign`, `Math.log`, `Math.log2`, `Math.log10`
- [x] **String() / Number() / Boolean()** — Type conversion functions
- [x] **isNaN() / isFinite()**
- [x] **console.error()** — Print to stderr
- [x] **Date.now()** — Millisecond timestamp

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
- [x] **Cross-platform** — macOS, Linux, and Windows support
- [ ] **Test suite** — Automated test runner against expected outputs
- [ ] **Benchmarks** — Compare performance vs Node.js / Bun / Deno

---

## What's left

The big remaining items are:
1. **Stack traces** (Phase 6) — `.stack` property on Error objects
2. **Async/await** (Phase 8) — The hardest remaining phase, needed for modern JS
3. **Modules** (Phase 9) — Multi-file programs
4. **Test suite / benchmarks** (Phase 10) — Automated testing and performance comparison

## Architecture note: the runtime library

The runtime library (`runtime/runtime.c`, ~1,200 lines of C) is already in place and provides:

- NaN-boxed value representation and type operations
- Reference-counted string allocation
- Object hash map implementation (FNV-1a hashing, linear probing)
- Dynamic arrays with 20+ methods
- Closure/function value support
- Error handling via setjmp/longjmp
- All synchronous built-in functions (Math, console, Date, JSON, etc.)
- Cross-platform support (Windows via `_strdup`/`GetSystemTimeAsFileTime`, POSIX via `strdup`/`gettimeofday`)

This is compiled alongside the generated LLVM IR by clang into the final native executable.
