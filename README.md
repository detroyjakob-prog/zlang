# Z Programming Language

**Version:** v0.2.0 (specification) — extends v0.1.0

Z is a keyword-driven language. Every action is introduced by an explicit keyword. All v0.1.0 syntax is unchanged: `fn`, `call`, `abl`, `callclass`, `callerror`, classes declared by bare name, `while`, `for`, `import`, `dir`.

Rules that hold everywhere:
- Functions are run with `call`, class and object methods with `callclass`, errors raised with `callerror`.
- Blocks use `{ }`. No semicolons. Newlines end statements.
- Types are always optional.

## Contents
1. Program Structure
2. Comments
3. Values and Types
4. Variables and Constants
5. Operators
6. Strings
7. Collections
8. Conditionals
9. Loops
10. Functions
11. Classes and OOP
12. Errors and Exceptions
13. Modules and Imports
14. Built-in Functions
15. Standard Libraries
16. Concurrency
17. Memory Model
18. Command Line
19. Reserved Words
20. Style Guide
21. Complete Example

---

## 1. Program Structure

A Z file ends in `.z`. Top-level statements run from top to bottom.

```Z
call printer("Hello, Z!")
```

If a top-level `fn Main()` exists, it runs automatically after all other top-level code.

```Z
fn Main() {
    call printer("Starting")
}
```

A class method called through `callclass Name.Main()` works as in v0.1.0.

---

## 2. Comments

```Z
// single line comment

/* multi
   line comment */
```

---

## 3. Values and Types

| Type | Example | Notes |
|------|---------|-------|
| `int` | `42`, `1_000`, `0xFF`, `0b101`, `0o17` | Arbitrary size |
| `float` | `3.14`, `1.5e3` | 64-bit |
| `str` | `"text"`, `'text'` | UTF-8, immutable |
| `bool` | `True`, `False` | |
| `None` | `None` | Absence of a value |
| `list` | `[1, 2, 3]` | Ordered, mutable |
| `tuple` | `(1, 2, 3)` | Ordered, immutable |
| `dict` | `{"a": 1}` | Key/value, insertion ordered |
| `set` | `call set([1, 2])` | Unique values |
| `fn` | `fn(x) => x + 1` | Functions are values |

Checking and converting types:

```Z
abl n = call int("42")
abl t = call type(n)
abl ok = call isinstance(n, int)
```

Falsy values: `False`, `None`, `0`, `0.0`, `""`, `[]`, `()`, `{}`. Everything else is truthy.

---

## 4. Variables and Constants

```Z
abl x = 1
abl int y = 2          // optional type
abl a, b = 10, 20      // multiple assignment
a, b = b, a            // swap
```

Constants cannot be reassigned:

```Z
const PI = 3.14159
```

Scope: variables are visible inside the block where they are declared and its inner blocks. Use `global` to assign a top-level variable from a function, and `outer` to assign a variable from an enclosing function.

```Z
abl counter = 0

fn Increase() {
    global counter
    counter = counter + 1
}
```

Delete a variable with `del`:

```Z
del x
```

---

## 5. Operators

| Category | Operators |
|----------|-----------|
| Arithmetic | `+  -  *  /  //  %  **` |
| Comparison | `==  !=  <  >  <=  >=` |
| Logical | `and  or  not` |
| Membership | `in  not in` |
| Identity | `is  is not` |
| Bitwise | `&  \|  ^  ~  <<  >>` |
| Assignment | `=  +=  -=  *=  /=  //=  %=  **=` |
| Conditional | `value_if_true if condition else value_if_false` |

`/` always gives a float. `//` is integer division.

Precedence, highest first:

1. `( )` grouping, `call`, `callclass`, indexing `[ ]`, attribute `.`
2. `**`
3. unary `-  +  ~`
4. `*  /  //  %`
5. `+  -`
6. `<<  >>`
7. `&`, then `^`, then `|`
8. `==  !=  <  >  <=  >=  in  is`
9. `not`
10. `and`
11. `or`
12. `if ... else`

```Z
abl label = "adult" if age >= 18 else "minor"
total += 5
```

---

## 6. Strings

```Z
abl s = "Hello"
abl multi = """line one
line two"""
abl raw = r"C:\new\folder"
abl name = "Z"
abl msg = f"Hello {name}, 1+1 = {1 + 1}"
```

Escapes: `\n  \t  \\  \"  \'  \u00e9`.

Indexing and slicing:

```Z
abl first = s[0]
abl last = s[-1]
abl part = s[1:3]
abl joined = "a" + "b"
abl repeated = "ab" * 3
```

String methods are called with `callclass`:

```Z
abl up = callclass s.upper()
abl parts = callclass "a,b,c".split(",")
abl clean = callclass "  hi  ".strip()
```

Available methods: `upper lower strip split join replace find startswith endswith count format title`.

---

## 7. Collections

### Lists

```Z
abl items = [1, 2, 3]
callclass items.append(4)
callclass items.remove(2)
abl first = items[0]
items[1] = 99
abl size = call len(items)
abl part = items[1:3]
```

Methods: `append insert remove pop sort reverse index count extend clear copy`.

### Tuples

```Z
abl point = (3, 4)
abl x, y = point
```

### Dictionaries

```Z
abl user = {"name": "Ana", "age": 30}
user["age"] = 31
abl name = user["name"]
abl email = callclass user.get("email", "none")

for key in callclass user.keys() {
    call printer(key)
}
```

Methods: `get keys values items pop update clear copy`.

### Sets

```Z
abl s = call set([1, 2, 2, 3])
callclass s.add(4)
abl both = s & call set([3, 4, 5])
```

Methods: `add remove union intersection difference clear`.

### Comprehensions

```Z
abl squares = [x * x for x in call range(5)]
abl evens = [x for x in items if x % 2 == 0]
abl lookup = {k: v for k, v in pairs}
```

### Unpacking

```Z
abl first, *rest = [1, 2, 3, 4]
abl merged = [*items, 5, 6]
```

---

## 8. Conditionals

```Z
if score >= 90 {
    call printer("A")
} elif score >= 80 {
    call printer("B")
} else {
    call printer("C")
}
```

Parentheses around conditions are optional.

### match

```Z
match command {
    case "start" {
        call printer("Starting")
    }
    case "stop", "quit" {
        call printer("Stopping")
    }
    case _ {
        call printer("Unknown")
    }
}
```

`_` matches anything.

---

## 9. Loops

```Z
while running {
    call Tick()
}

for x in items {
    call printer(x)
}

for i in call range(0, 10, 2) {
    call printer(i)
}

for i, x in call enumerate(items) {
    call printer(f"{i}: {x}")
}
```

Loop control:

| Keyword | Effect |
|---------|--------|
| `break` | Leave the loop |
| `continue` | Skip to the next iteration |
| `pass` | Do nothing (empty block) |

```Z
for n in numbers {
    if n < 0 {
        continue
    }
    if n > 100 {
        break
    }
    call printer(n)
}
```

---

## 10. Functions

### Return values

```Z
fn Add(a, b) {
    return a + b
}

abl total = call Add(2, 3)
abl nested = call Add(call Add(1, 2), 3)
```

A function without `return` returns `None`.

### Default and keyword arguments

```Z
fn Greet(name = "World", punctuation = "!") {
    return f"Hello, {name}{punctuation}"
}

call Greet()
call Greet("Ana")
call Greet(punctuation = "?")
```

### Variable arguments

```Z
fn Sum(*numbers) {
    abl total = 0
    for n in numbers {
        total += n
    }
    return total
}

fn Config(**options) {
    for key, value in callclass options.items() {
        call printer(f"{key} = {value}")
    }
}
```

### Type annotations (optional)

```Z
fn Add(int a, int b) -> int {
    return a + b
}
```

### Anonymous functions

```Z
abl double = fn(x) => x * 2
abl result = call double(4)

abl sorted_names = call sorted(names, key = fn(n) => call len(n))
```

### Closures

```Z
fn MakeCounter() {
    abl count = 0
    fn Step() {
        outer count
        count += 1
        return count
    }
    return Step
}

abl next = call MakeCounter()
call next()
```

### Recursion

```Z
fn Fact(n) {
    if n <= 1 {
        return 1
    }
    return n * call Fact(n - 1)
}
```

### Generators

```Z
fn Countdown(n) {
    while n > 0 {
        yield n
        n -= 1
    }
}

for x in call Countdown(3) {
    call printer(x)
}
```

---

## 11. Classes and OOP

### Declaring and constructing

A class is declared by its name and `{ }`. `init` is the constructor. `self` is the instance.

```Z
Animal {
    abl kind = "animal"          // class attribute

    fn init(self, name) {
        self.name = name
    }

    fn Speak(self) {
        call printer(f"{self.name} makes a sound")
    }
}

abl a = callclass Animal("Rex")  // create an instance
callclass a.Speak()              // call a method
call printer(a.name)             // read an attribute
a.name = "Max"                   // write an attribute
```

### Class-level methods

Methods without `self`, or marked `static`, belong to the class and are called on its name, exactly as in v0.1.0.

```Z
Tools {
    fn Main() {
        call printer("class-level")
    }

    static fn Square(x) {
        return x * x
    }
}

callclass Tools.Main()
abl n = callclass Tools.Square(5)
```

### Inheritance

```Z
Dog(Animal) {
    fn init(self, name, breed) {
        callclass super.init(name)
        self.breed = breed
    }

    fn Speak(self) {
        call printer(f"{self.name} barks")
    }
}

abl d = callclass Dog("Rex", "Collie")
callclass d.Speak()
```

Multiple inheritance: `Duck(Bird, Swimmer) { }`.

### Special methods

| Method | Triggered by |
|--------|--------------|
| `init` | Construction |
| `str` | `call str(obj)`, printing |
| `eq` | `==` |
| `lt` | `<` |
| `add` | `+` |
| `len` | `call len(obj)` |
| `get` / `set` | `obj[key]` read / write |
| `iter` / `next` | `for x in obj` |

```Z
Vec {
    fn init(self, x, y) {
        self.x = x
        self.y = y
    }

    fn add(self, other) {
        return callclass Vec(self.x + other.x, self.y + other.y)
    }

    fn str(self) {
        return f"({self.x}, {self.y})"
    }
}
```

### Privacy

Names starting with `_` are private by convention and not exported from modules.

### Enums

```Z
enum Color {
    RED = 1
    GREEN = 2
    BLUE = 3
}

abl c = Color.RED
```

### Introspection

```Z
call isinstance(d, Dog)
call isinstance(d, Animal)
call hasattr(d, "breed")
call type(d)
```

---

## 12. Errors and Exceptions

### Handling

```Z
try {
    abl n = call int("abc")
} except ValueError as e {
    call printer(f"Bad number: {e}")
} except TypeError, KeyError {
    call printer("Type or key problem")
} else {
    call printer("No error")
} finally {
    call printer("Always runs")
}
```

### Raising

`callerror` raises an error. Built-in names match Python's.

```Z
callerror ValueError("age must be positive")
```

### Custom errors

As in v0.1.0, a class can be a custom error. To make it catchable with a message, inherit from `Error`:

```Z
InvalidAge(Error) {
    fn init(self, age) {
        self.message = f"Invalid age: {age}"
    }
}

try {
    callerror InvalidAge(-5)
} except InvalidAge as e {
    call printer(e.message)
}
```

Re-raise inside `except` with a bare `callerror`.

### Assertions

```Z
assert x > 0, "x must be positive"
```

### Context managers

```Z
with call files.open("data.txt") as f {
    abl text = callclass f.read()
}
```

### Uncaught errors

An uncaught error stops the program and prints the error name, message, file and line, then exits with code 1.

---

## 13. Modules and Imports

```Z
import math
import random as rnd
from math import sqrt, pi
from math import sqrt as root
```

Import from a path:

```Z
dir (path/to/folder) import MyModule
dir (./lib) import Helpers, Utils
```

Rules:
- A file `tools.z` is imported as `tools`.
- A folder with an `init.z` file is a package.
- Top-level names not starting with `_` are exported.
- Imports run once; later imports reuse the loaded module.
- Circular imports raise `ImportError`.

```Z
import tools
call tools.Helper()
callclass tools.Tools.Main()
```

---

## 14. Built-in Functions

All built-ins are used with `call`.

| Function | Purpose |
|----------|---------|
| `len(x)` | Length |
| `range(a, b, step)` | Number sequence |
| `type(x)` | Type of a value |
| `isinstance(x, T)` | Type check |
| `hasattr(o, "n")` / `getattr` / `setattr` | Attribute access |
| `int float str bool list tuple dict set` | Conversions |
| `abs min max sum round pow divmod` | Numbers |
| `sorted reversed enumerate zip map filter` | Iteration |
| `any all` | Boolean reductions |
| `input(prompt)` | Read a line |
| `exit(code)` | Stop the program |
| `id(x)` / `hash(x)` | Identity and hash |

---

## 15. Standard Libraries

### printer
```Z
call printer("a", "b", sep = "-", end = "")
call printer.error("goes to stderr")
```

### math
`pi e sqrt sin cos tan log log10 exp floor ceil abs pow gcd factorial isnan inf`

### random
`random() randint(a, b) choice(list) shuffle(list) uniform(a, b) seed(n) sample(list, k)`

### time
`now() sleep(seconds) time() format(t, pattern)`

### files
```Z
call files.write("a.txt", "hello")
abl text = call files.read("a.txt")
abl exists = call files.exists("a.txt")
call files.delete("a.txt")
abl lines = call files.lines("a.txt")
```

### json
`parse(text) stringify(value, indent)`

### os
`args env(name) cwd() listdir(path) platform`

### strings
`digits letters punctuation`

### gg
Reserved for Z's special library. Its contents are defined by the language author.

---

## 16. Concurrency

```Z
async fn Fetch(url) {
    abl data = await call http.get(url)
    return data
}

async fn Main() {
    abl a = await call Fetch("one")
    abl results = await call gather(call Fetch("one"), call Fetch("two"))
}
```

`await` is only valid inside `async fn`. `gather` runs several async calls together. `time.sleep` blocks; `await call time.pause(seconds)` does not.

---

## 17. Memory Model

- Memory is managed automatically by a garbage collector.
- Assignment of lists, dicts, sets and objects copies the reference, not the value. Use `callclass x.copy()` for a shallow copy.
- `int`, `float`, `str`, `bool`, `None` and `tuple` are immutable.

---

## 18. Command Line

```
z run file.z          run a program
z check file.z        check syntax without running
z repl                interactive prompt
z --version           print the version
```

Exit code is 0 on success and 1 on an uncaught error. `call exit(n)` sets a custom code.

---

## 19. Reserved Words

```
abl  and  as  assert  async  await  break  call  callclass  callerror
case  const  continue  del  dir  elif  else  enum  except  False  finally
fn  for  from  global  if  import  in  is  match  None  not  or  outer
pass  return  self  static  super  True  try  while  with  yield
```

---

## 20. Style Guide

| Item | Convention | Example |
|------|-----------|---------|
| Functions and classes | PascalCase | `GetUser`, `Animal` |
| Variables | snake_case | `user_name` |
| Constants | UPPER_CASE | `MAX_SIZE` |
| Private | leading `_` | `_cache` |
| Indent | 4 spaces | |
| Line length | 100 characters | |

---

## 21. Complete Example

```Z
import random
from math import sqrt

const MAX_TRIES = 3

Shape {
    fn init(self, name) {
        self.name = name
    }

    fn Area(self) {
        return 0
    }

    fn str(self) {
        return f"{self.name} with area {callclass self.Area()}"
    }
}

Circle(Shape) {
    fn init(self, r) {
        callclass super.init("circle")
        self.r = r
    }

    fn Area(self) {
        return 3.14159 * self.r ** 2
    }
}

fn Main() {
    abl shapes = [callclass Circle(1), callclass Circle(2)]

    for i, s in call enumerate(shapes) {
        call printer(f"{i}: {call str(s)}")
    }

    try {
        abl bad = call int("x")
    } except ValueError as e {
        call printer(f"Caught: {e}")
    } finally {
        call printer("Done")
    }
}
```

---

© Z Programming Language documentation 2026
© Z Programming Language v0.2.0
