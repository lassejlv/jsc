function factorial(n) {
  if (n <= 1) {
    return 1;
  }
  return n * factorial(n - 1);
}

console.log("Factorial of 10:");
console.log(factorial(10));

let sum = 0;
let i = 1;
while (i <= 100) {
  sum = sum + i;
  i = i + 1;
}
console.log("Sum 1 to 100:");
console.log(sum);
