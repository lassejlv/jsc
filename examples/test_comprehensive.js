// Comprehensive test: a small program using most features

function range(start, end) {
  let arr = [];
  for (let i = start; i < end; i++) {
    arr.push(i);
  }
  return arr;
}

// FizzBuzz with arrays and string concatenation
let results = range(1, 21).map((n) => {
  if (n % 15 === 0) return "FizzBuzz";
  if (n % 3 === 0) return "Fizz";
  if (n % 5 === 0) return "Buzz";
  return String(n);
});
console.log(results.join(", "));

// Object manipulation
function createPerson(name, age) {
  return { name: name, age: age };
}

let people = [
  createPerson("Alice", 30),
  createPerson("Bob", 25),
  createPerson("Charlie", 35),
];

// Get names of people older than 27
let olderPeople = people.filter((p) => p.age > 27).map((p) => p.name);
console.log("People over 27: " + olderPeople.join(", "));

// Recursive quicksort
function quicksort(arr) {
  if (arr.length <= 1) return arr;
  let pivot = arr[0];
  let left = arr.slice(1).filter((x) => x <= pivot);
  let right = arr.slice(1).filter((x) => x > pivot);
  return quicksort(left).concat([pivot]).concat(quicksort(right));
}

let unsorted = [38, 27, 43, 3, 9, 82, 10];
let sorted = quicksort(unsorted);
console.log("Sorted: " + sorted.join(", "));

// String processing
let csv = "name,age,city\nAlice,30,NYC\nBob,25,LA";
let lines = csv.split("\n");
let header = lines[0].split(",");
console.log("Columns: " + header.join(" | "));

// Math operations
let values = [16, 25, 36, 49, 64];
let roots = values.map((v) => Math.sqrt(v));
console.log("Square roots: " + roots.join(", "));

// Type checking
function describe(val) {
  if (typeof val === "string") return `"${val}" is a string`;
  if (typeof val === "number") return `${val} is a number`;
  if (Array.isArray(val)) return "it's an array with " + val.length + " items";
  return "unknown type";
}

console.log(describe("hello"));
console.log(describe(42));
console.log(describe([1, 2, 3]));

console.log("All tests passed!");
