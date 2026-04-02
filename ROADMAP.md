# Roadmap: Toward a Working JS Compiler

Current state: compiles JavaScript to native executables via LLVM IR. Phases 1–8 are complete. Comprehensive language support including: NaN-boxed dynamic types, strings (27+ methods), objects, arrays (25+ methods), closures, `this` binding, try/catch/finally, destructuring, spread, JSON.parse/stringify, all synchronous builtins, switch/do-while/for-in, break/continue, compound assignments, bitwise operators, optional chaining (`?.`), nullish coalescing (`??`), Promises, async/await, setTimeout/setInterval, event loop, and fetch() with full HTTP support. Cross-platform (macOS, Linux, Windows).

Remaining work: modules, classes, and polish.

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
- [x] **Runtime allocator** — C runtime library linked into every compiled program, uses malloc/free

## Phase 4: Objects and Arrays ✅

- [x] **Object literals** — `{ key: value }` as hash maps (FNV-1a hashing, linear probing)
- [x] **Property access** — `obj.key` and `obj["key"]`
- [x] **Property assignment** — `obj.key = value` and `obj["key"] = value`
- [x] **Property deletion** — `delete obj.key`
- [x] **Arrays** — `[1, 2, 3]` with dynamic resizing
- [x] **Array methods** — `.push()`, `.pop()`, `.shift()`, `.unshift()`, `.length`, `.indexOf()`, `.includes()`, `.join()`, `.reverse()`, `.slice()`, `.concat()`, `.map()`, `.filter()`, `.forEach()`, `.reduce()`, `.find()`, `.findIndex()`, `.every()`, `.some()`, `.flat()`, `.sort()`, `.splice()`, `.fill()`, `.toString()`
- [x] **for...of loops** — Iterate over arrays
- [x] **for...in loops** — Iterate over object keys / array indices
- [x] **Spread operator** — `[...arr]`, `{...obj}`
- [x] **Destructuring** — `const { a, b } = obj`, `const [x, y] = arr`, nested, defaults, rest elements, function parameters
- [x] **JSON.stringify / JSON.parse** — Full round-trip support
- [x] **Object.keys() / Object.values() / Object.entries()** — Implemented
- [x] **Object.assign()** — Merge objects
- [x] **Array.isArray() / Array.from()** — Implemented

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

## Phase 7: Built-in Functions and I/O Runtime ✅

### Synchronous built-ins
- [x] **prompt(message)** — Read line from stdin
- [x] **parseInt() / parseFloat()** — String to number conversion
- [x] **Math object** — `Math.floor`, `Math.ceil`, `Math.round`, `Math.random`, `Math.sqrt`, `Math.pow`, `Math.abs`, `Math.min`, `Math.max`, `Math.PI`, `Math.E`, `Math.LN2`, `Math.LN10`, `Math.SQRT2`, `Math.LOG2E`, `Math.LOG10E`, `Math.sin`, `Math.cos`, `Math.tan`, `Math.atan2`, `Math.exp`, `Math.trunc`, `Math.sign`, `Math.log`, `Math.log2`, `Math.log10`
- [x] **String() / Number() / Boolean()** — Type conversion functions
- [x] **isNaN() / isFinite()**
- [x] **console.error()** — Print to stderr
- [x] **Date.now()** — Millisecond timestamp

### Async built-ins
- [x] **setTimeout / setInterval** — Timer-based callbacks with event loop
- [x] **clearTimeout / clearInterval** — Cancel scheduled timers
- [x] **fetch()** — Full HTTP client via libcurl (GET/POST/PUT/DELETE/PATCH/HEAD, headers, body, redirect control, timeout, response.json()/text()/headers.get())

## Phase 7.5: Operators and Control Flow ✅

- [x] **Compound assignments** — `+=`, `-=`, `*=`, `/=`, `%=`, `**=`, `&=`, `|=`, `^=`, `<<=`, `>>=`, `>>>=`
- [x] **Bitwise operators** — `&`, `|`, `^`, `~`, `<<`, `>>`, `>>>`
- [x] **Exponentiation** — `**` operator
- [x] **Nullish coalescing** — `??` (correctly checks null/undefined, not truthiness)
- [x] **Optional chaining** — `obj?.prop`, `obj?.[key]`
- [x] **`in` operator** — `"key" in obj`
- [x] **`delete` operator** — `delete obj.prop`
- [x] **`void` operator** — `void expr`
- [x] **`instanceof` operator** — Basic support (placeholder for full prototype chain)
- [x] **switch statement** — With case/default, break, and fall-through
- [x] **do...while loop** — Post-condition loop
- [x] **for...in loop** — Iterate over object keys
- [x] **break / continue** — In all loop types and switch
- [x] **Sequence expression** — Comma operator `(a, b, c)`
- [x] **Labeled statements** — Basic support

## Phase 8: Async / Await ✅

- [x] **Promises** — Full implementation: `new Promise(executor)`, `.then()`, `.catch()`, `.finally()` with chaining and error propagation
- [x] **Promise static methods** — `Promise.resolve()`, `Promise.reject()`, `Promise.all()`, `Promise.race()`, `Promise.allSettled()`
- [x] **async functions** — `async function`, `async` arrow functions, `async` function expressions — return values wrapped in Promise
- [x] **await expressions** — `await` unwraps Promises (throws on rejection), passes through non-Promises
- [x] **Top-level await** — Works at module/script level
- [x] **Event loop** — Timer-based event loop runs after main code, processes timers in correct time order, exits when no active timers remain

## Phase 9: Module System

- [ ] **import / export** — ES module syntax
- [ ] **Multiple file compilation** — Compile and link multiple JS files
- [ ] **Standard library modules** — Bundle built-in modules

## Phase 10: Classes

- [ ] **Class declarations** — `class Foo { constructor() {} }`
- [ ] **Methods** — Instance methods on prototype
- [ ] **`extends` / `super`** — Inheritance
- [ ] **Static methods** — `static foo() {}`
- [ ] **Getters / setters** — `get prop()`, `set prop(v)`

## Phase 11: Polish and Compatibility

- [ ] **Source maps** — Map compiled code back to JS source for debugging
- [ ] **Better error messages** — Line numbers and context in compile errors
- [ ] **Tail call optimization** — For recursive functions
- [x] **Cross-platform** — macOS, Linux, and Windows support
- [ ] **Test suite** — Automated test runner against expected outputs
- [ ] **Benchmarks** — Compare performance vs Node.js / Bun / Deno

---

## What's left

The big remaining items are:
1. **Classes** (Phase 10) — Very common in modern JS, needed for most frameworks
2. **Modules** (Phase 9) — Multi-file programs with import/export
3. **Test suite / benchmarks** (Phase 11) — Automated testing and performance comparison

## Architecture note: the runtime library

The runtime is split into 11 modular C files (~2,200 lines total) under `runtime/`, concatenated at compile time:

| File | Purpose |
|------|---------|
| `js_types.c` | NaN-boxing, type definitions, forward declarations |
| `js_strings.c` | String alloc, concat, compare |
| `js_objects.c` | Objects (hash map) and arrays |
| `js_core.c` | Type coercion, errors, arithmetic, comparisons, property access |
| `js_methods.c` | Method dispatch (string/array/object/Promise methods) |
| `js_builtins.c` | prompt, Math, parseInt, Date.now, etc. |
| `js_json.c` | JSON.stringify and JSON.parse |
| `js_operators.c` | Bitwise, spread, this, in/delete, sort/splice |
| `js_fetch.c` | HTTP client via libcurl |
| `js_promise.c` | Promises, async/await, setTimeout/setInterval, event loop |
| `js_init.c` | Runtime initialization |

This is compiled alongside the generated LLVM IR by clang into the final native executable.
