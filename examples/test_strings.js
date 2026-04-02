// String operations
let greeting = "Hello";
let name = "World";
console.log(greeting + ", " + name + "!");

// String methods
let str = "  Hello, World!  ";
console.log(str.trim());
console.log(str.trim().toUpperCase());
console.log(str.trim().toLowerCase());
console.log("abcdef".slice(2, 5));
console.log("hello world".indexOf("world"));
console.log("hello world".includes("world"));
console.log("ha".repeat(3));
console.log("foo bar baz".replace("bar", "qux"));
console.log("a,b,c,d".split(",").join(" - "));

// String concatenation with numbers
let x = 42;
console.log("The answer is: " + x);

// Template literals
let a = 10;
let b = 20;
console.log(`${a} + ${b} = ${a + b}`);

// String length
console.log("hello".length);

// typeof
console.log(typeof "hello");
console.log(typeof 42);
console.log(typeof true);
console.log(typeof null);
console.log(typeof undefined);
