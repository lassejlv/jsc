// Arrow functions as values
let add = (a, b) => a + b;
console.log(add(3, 4));

// Function expressions
let multiply = function(a, b) { return a * b; };
console.log(multiply(6, 7));

// Passing functions as arguments
function apply(fn, x, y) {
  return fn(x, y);
}
console.log(apply(add, 10, 20));
console.log(apply((a, b) => a - b, 100, 42));

// Callbacks with array methods
let numbers = [1, 2, 3, 4, 5];
let squares = numbers.map((n) => n * n);
console.log(squares.join(", "));

// Multi-line arrow function
let greet = (name) => {
  let msg = "Hello, " + name + "!";
  return msg;
};
console.log(greet("World"));
