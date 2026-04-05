# Roadmap: Toward a Working JS Compiler

Current state: compiles JavaScript/TypeScript to native executables via LLVM IR. Phases 1–10.5 are complete. Comprehensive language support including: NaN-boxed dynamic types, strings (27+ methods), objects, arrays (25+ methods), closures, `this` binding, try/catch/finally, destructuring, spread, JSON.parse/stringify, all synchronous builtins, switch/do-while/for-in, break/continue, compound assignments, bitwise operators, optional chaining (`?.`), nullish coalescing (`??`), Promises, async/await, setTimeout/setInterval, event loop, and fetch() with full HTTP support. Cross-platform (macOS, Linux, Windows).

Remaining work: Hono integration testing and polish.

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

## Phase 9: Module System ✅

- [x] **Named imports** — `import { x, y } from "./mod.js"`
- [x] **Default imports** — `import foo from "./mod.js"`
- [x] **Namespace imports** — `import * as mod from "./mod.js"`
- [x] **Named exports** — `export function foo() {}`, `export const x = 5`, `export { x, y }`
- [x] **Default exports** — `export default function() {}`, `export default expr`
- [x] **Multiple file compilation** — Automatic recursive dependency discovery, topological module init
- [x] **Module isolation** — Each module runs once (init guard), has its own scope and exports object
- [x] **Transitive imports** — Module A imports B which imports C — all resolved and compiled
- [ ] **Standard library modules** — Bundle built-in modules (e.g. `fs`, `path`)

## Phase 10: Classes ✅

- [x] **Class declarations** — `class Foo { constructor() {} }`, class expressions
- [x] **Methods** — Instance methods stamped onto each object, correct `this` binding
- [x] **`extends`** — Inheritance via super class instantiation + property copy
- [x] **Static methods** — `static foo() {}` attached to the class constructor object
- [x] **Getters / setters** — `get prop()`, `set prop(v)` via `__getters`/`__setters` with runtime intercept in `js_get_prop`
- [x] **`instanceof`** — Works via `__type` property comparison
- [x] **Method chaining** — `return this` works correctly

## Phase 10.5: Web Server & APIs ✅

- [x] **`JSC.serve()`** — HTTP server via POSIX sockets, single-threaded accept loop
- [x] **`new Request(url, init)`** — Full Web standard Request with method, url, headers, body, pathname
- [x] **`new Response(body, init)`** — Full Web standard Response with status, headers, text(), json()
- [x] **`Response.json(data)`** — Static method for JSON responses with auto Content-Type
- [x] **`Response.redirect(url, status)`** — Static method for redirects
- [x] **`new Headers(init)`** — Full Headers API: get, set, has, delete, forEach, entries, keys, values
- [x] **`new URL(url)`** — URL parsing: protocol, host, hostname, port, pathname, search, hash, origin
- [x] **node_modules resolution** — Bare specifier resolution, package.json exports/module/main, scoped packages, sub-path imports
- [x] **Request/Response integration** — HTTP parser creates Request, Response serializer writes to socket

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
1. **Hono integration** — Full testing with the Hono web framework from node_modules
2. **Test suite / benchmarks** (Phase 11) — Automated testing and performance comparison

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
