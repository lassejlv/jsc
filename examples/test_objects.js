// Object literals
let person = { name: "Alice", age: 30 };
console.log(person.name);
console.log(person.age);

// Property assignment
person.age = 31;
console.log(person.age);

// Computed property access
console.log(person["name"]);

// Nested objects
let config = {
  db: { host: "localhost", port: 5432 },
  app: { name: "MyApp" }
};
console.log(config.db.host);
console.log(config.db.port);
console.log(config.app.name);

// Object.keys and Object.values
let obj = { x: 1, y: 2, z: 3 };
console.log(Object.keys(obj).join(", "));
console.log(Object.values(obj).join(", "));

// JSON.stringify
console.log(JSON.stringify({ a: 1, b: "hello", c: true, d: null }));
console.log(JSON.stringify([1, 2, 3]));
