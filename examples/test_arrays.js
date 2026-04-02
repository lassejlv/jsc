// Array operations
let arr = [1, 2, 3, 4, 5];
console.log(arr.length);
console.log(arr.join(", "));

// push / pop
arr.push(6);
console.log(arr.join(", "));
let popped = arr.pop();
console.log(popped);

// Array indexing
console.log(arr[0]);
console.log(arr[4]);

// slice
console.log(arr.slice(1, 3).join(", "));

// indexOf / includes
console.log(arr.indexOf(3));
console.log(arr.includes(3));
console.log(arr.includes(99));

// reverse
console.log([5, 3, 1, 4, 2].reverse().join(", "));

// concat
console.log([1, 2].concat([3, 4]).join(", "));

// Higher-order functions
let nums = [1, 2, 3, 4, 5];

let doubled = nums.map((x) => x * 2);
console.log(doubled.join(", "));

let evens = nums.filter((x) => x % 2 === 0);
console.log(evens.join(", "));

let sum = nums.reduce((acc, x) => acc + x, 0);
console.log(sum);

// forEach (prints each element)
nums.forEach((x) => console.log(x));

// find / findIndex
console.log(nums.find((x) => x > 3));
console.log(nums.findIndex((x) => x > 3));

// every / some
console.log(nums.every((x) => x > 0));
console.log(nums.some((x) => x > 4));

// for...of
let fruits = ["apple", "banana", "cherry"];
for (let fruit of fruits) {
  console.log(fruit);
}
