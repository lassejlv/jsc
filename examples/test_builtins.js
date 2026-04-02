// Math functions
console.log(Math.floor(4.7));
console.log(Math.ceil(4.2));
console.log(Math.round(4.5));
console.log(Math.abs(-42));
console.log(Math.sqrt(144));
console.log(Math.pow(2, 10));
console.log(Math.PI);
console.log(Math.min(5, 3, 8, 1, 9));
console.log(Math.max(5, 3, 8, 1, 9));
console.log(Math.trunc(4.9));
console.log(Math.sign(-5));

// parseInt / parseFloat
console.log(parseInt("42"));
console.log(parseInt("0xFF", 16));
console.log(parseFloat("3.14"));
console.log(isNaN(parseInt("hello")));

// isNaN / isFinite
console.log(isNaN(NaN));
console.log(isFinite(42));
console.log(isFinite(1 / 0));

// Number / String / Boolean conversions
console.log(Number("42"));
console.log(String(42));
console.log(Boolean(0));
console.log(Boolean(1));

// Date.now (just check it returns a number)
let now = Date.now();
console.log(typeof now);
console.log(now > 0);

// Ternary operator
let val = 10 > 5 ? "yes" : "no";
console.log(val);

// Array.isArray
console.log(Array.isArray([1, 2, 3]));
console.log(Array.isArray("hello"));

// Null and undefined
console.log(null);
console.log(undefined);
console.log(null == undefined);
console.log(null === undefined);
