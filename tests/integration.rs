//! Integration tests for the Z language.
//!
//! Each test compiles and runs a Z program and compares the printed output
//! against the expected result.

use zlang::compiler;
use zlang::Vm;

/// Compile and run `source`, returning everything it printed.
fn run(source: &str) -> Result<String, String> {
    let module = compiler::compile(source, None).map_err(|e| e.report())?;
    let mut vm = Vm::new();
    vm.install_builtins();
    vm.capture_output = true;
    vm.run(&module).map_err(|e| e.report())?;
    Ok(vm.output)
}

#[test]
fn hello_world() {
    let out = run(r#"
fn Main() {
    call printer("Hello, Z!")
}
"#).expect("program ran");
    assert_eq!(out.trim(), "Hello, Z!");
}

#[test]
fn arithmetic_and_precedence() {
    let out = run(r#"
fn Main() {
    call printer(2 + 3 * 4)
    call printer((2 + 3) * 4)
    call printer(10 // 3)
    call printer(10 % 3)
    call printer(2 ** 8)
    call printer(-5 + 2)
    call printer(7 / 2)
}
"#).expect("program ran");
    assert_eq!(out.trim().lines().collect::<Vec<_>>(),
        vec!["14", "20", "3", "1", "256", "-3", "3.5"]);
}

#[test]
fn variables_and_constants() {
    let out = run(r#"
const LIMIT = 10

fn Main() {
    abl x = 1
    abl int y = 2
    abl a, b = 10, 20
    a, b = b, a
    call printer(x + y)
    call printer(a)
    call printer(b)
    call printer(LIMIT)
    x += 5
    call printer(x)
}
"#).expect("program ran");
    assert_eq!(out.trim().lines().collect::<Vec<_>>(),
        vec!["3", "20", "10", "10", "6"]);
}

#[test]
fn conditionals_and_match() {
    let out = run(r#"
fn Classify(n) {
    if n >= 90 {
        return "A"
    } elif n >= 80 {
        return "B"
    } else {
        return "C"
    }
}

fn Describe(cmd) {
    match cmd {
        case "start" {
            return "starting"
        }
        case "stop", "quit" {
            return "stopping"
        }
        case _ {
            return "unknown"
        }
    }
}

fn Main() {
    call printer(call Classify(95))
    call printer(call Classify(85))
    call printer(call Classify(70))
    call printer(call Describe("stop"))
    call printer(call Describe("quit"))
    call printer(call Describe("x"))
    abl t = "yes" if 1 else "no"
    call printer(t)
}
"#).expect("program ran");
    assert_eq!(out.trim().lines().collect::<Vec<_>>(),
        vec!["A", "B", "C", "stopping", "stopping", "unknown", "yes"]);
}

#[test]
fn loops_and_control_flow() {
    let out = run(r#"
fn Main() {
    abl total = 0
    for i in call range(1, 6) {
        total += i
    }
    call printer(total)

    abl n = 0
    abl found = 0
    while True {
        n += 1
        if n == 3 {
            continue
        }
        if n > 5 {
            break
        }
        found = n
    }
    call printer(found)
}
"#).expect("program ran");
    assert_eq!(out.trim().lines().collect::<Vec<_>>(), vec!["15", "5"]);
}

#[test]
fn functions_and_defaults() {
    let out = run(r#"
fn Greet(name = "World", punctuation = "!") {
    return f"Hello, {name}{punctuation}"
}

fn Sum(*numbers) {
    abl total = 0
    for n in numbers {
        total += n
    }
    return total
}

fn Main() {
    call printer(call Greet())
    call printer(call Greet("Ana"))
    call printer(call Greet(punctuation = "?"))
    call printer(call Sum(1, 2, 3, 4))
}
"#).expect("program ran");
    assert_eq!(out.trim().lines().collect::<Vec<_>>(),
        vec!["Hello, World!", "Hello, Ana!", "Hello, World?", "10"]);
}

#[test]
fn closures_and_outer_scope() {
    let out = run(r#"
fn MakeCounter(start) {
    abl count = start
    fn Step() {
        outer count
        count += 1
        return count
    }
    return Step
}

fn MakeAdder(n) {
    return fn(x) => x + n
}

fn Main() {
    abl next = call MakeCounter(10)
    call printer(call next())
    call printer(call next())
    abl addFive = call MakeAdder(5)
    call printer(call addFive(3))
}
"#).expect("program ran");
    assert_eq!(out.trim().lines().collect::<Vec<_>>(),
        vec!["11", "12", "8"]);
}

#[test]
fn generators() {
    let out = run(r#"
fn Countdown(n) {
    while n > 0 {
        yield n
        n -= 1
    }
}

fn Main() {
    for v in call Countdown(3) {
        call printer(v)
    }
}
"#).expect("program ran");
    assert_eq!(out.trim().lines().collect::<Vec<_>>(), vec!["3", "2", "1"]);
}

#[test]
fn classes_and_inheritance() {
    let out = run(r#"
Animal {
    abl kind = "animal"

    fn init(self, name) {
        self.name = name
    }

    fn Speak(self) {
        return "generic sound"
    }
}

Dog(Animal) {
    fn init(self, name, breed) {
        callclass super.init(name)
        self.breed = breed
    }

    fn Speak(self) {
        return f"{self.name} barks"
    }
}

fn Main() {
    abl a = callclass Animal("rex")
    abl d = callclass Dog("fido", "collie")
    call printer(callclass a.Speak())
    call printer(callclass d.Speak())
    call printer(d.breed)
    call printer(d.name)
    call printer(d.kind)
}
"#).expect("program ran");
    assert_eq!(out.trim().lines().collect::<Vec<_>>(),
        vec!["generic sound", "fido barks", "collie", "fido", "animal"]);
}

#[test]
fn special_methods() {
    let out = run(r#"
Vec {
    fn init(self, x, y) {
        self.x = x
        self.y = y
    }

    fn add(self, other) {
        return callclass Vec(self.x + other.x, self.y + other.y)
    }

    fn eq(self, other) {
        return self.x == other.x and self.y == other.y
    }

    fn str(self) {
        return f"({self.x}, {self.y})"
    }
}

fn Main() {
    abl a = callclass Vec(1, 2)
    abl b = callclass Vec(2, 3)
    abl c = callclass Vec(1, 2)
    abl sum = a + b
    call printer(call str(sum))
    call printer(a == c)
    call printer(a == b)
}
"#).expect("program ran");
    assert_eq!(out.trim().lines().collect::<Vec<_>>(),
        vec!["(3, 5)", "True", "False"]);
}

#[test]
fn enums() {
    let out = run(r#"
enum Color {
    RED = 1
    GREEN = 2
    BLUE = 3
}

fn Main() {
    abl c = Color.GREEN
    call printer(c)
}
"#).expect("program ran");
    assert_eq!(out.trim(), "2");
}

#[test]
fn strings_and_methods() {
    let out = run(r#"
fn Main() {
    abl s = "  hello world  "
    call printer(callclass s.strip())
    call printer(callclass s.upper())
    call printer(callclass "a,b,c".split(","))
    call printer(callclass "  hi  ".strip())
    abl name = "Z"
    call printer(f"Hello {name}, 1+1 = {1 + 1}")
    abl parts = callclass "x-y-z".split("-")
    call printer(parts[1])
    abl joined = callclass "-".join(parts)
    call printer(joined)
}
"#).expect("program ran");
    assert_eq!(out.trim().lines().collect::<Vec<_>>(),
        vec!["hello world", "HELLO WORLD", "['a', 'b', 'c']", "hi", "Hello Z, 1+1 = 2", "y", "x-y-z"]);
}

#[test]
fn collections() {
    let out = run(r#"
fn Main() {
    abl items = [1, 2, 3]
    callclass items.append(4)
    callclass items.remove(2)
    call printer(items)
    call printer(call len(items))
    abl part = items[1:3]
    call printer(part)

    abl user = {"name": "Ana", "age": 30}
    user["age"] = 31
    call printer(user["name"])
    call printer(user["age"])
    call printer(callclass user.get("email", "none"))

    abl s = call set([1, 2, 2, 3])
    call printer(call len(s))
}
"#).expect("program ran");
    assert_eq!(out.trim().lines().collect::<Vec<_>>(),
        vec!["[1, 3, 4]", "3", "[3, 4]", "Ana", "31", "none", "3"]);
}

#[test]
fn comprehensions() {
    let out = run(r#"
fn Main() {
    abl squares = [x * x for x in call range(6)]
    call printer(squares)
    abl evens = [x for x in squares if x % 2 == 0]
    call printer(evens)
    abl lookup = {k: k * k for k in [1, 2, 3]}
    call printer(lookup)
    abl doubled = [x * 2 for x in [1, 2, 3] for y in [0]]
    call printer(doubled)
}
"#).expect("program ran");
    let lines: Vec<&str> = out.trim().lines().collect();
    assert_eq!(lines[0], "[0, 1, 4, 9, 16, 25]");
    assert_eq!(lines[1], "[0, 4, 16]");
    assert_eq!(lines[2], "{1: 1, 2: 4, 3: 9}");
}

#[test]
fn try_except_finally() {
    let out = run(r#"
fn Main() {
    try {
        abl bad = call int("abc")
    } except ValueError as e {
        call printer("caught value error")
    } else {
        call printer("no error")
    } finally {
        call printer("finally")
    }

    try {
        abl n = call int("42")
    } except ValueError as e {
        call printer("unexpected")
    } else {
        call printer("no error")
    }

    try {
        callerror ValueError("boom")
    } except ValueError as e {
        call printer(e)
    }
}
"#).expect("program ran");
    assert_eq!(out.trim().lines().collect::<Vec<_>>(),
        vec!["caught value error", "finally", "no error", "boom"]);
}

#[test]
fn custom_errors() {
    let out = run(r#"
InvalidAge(Error) {
    fn init(self, age) {
        self.age = age
        self.message = f"Invalid age: {age}"
    }
}

fn Main() {
    try {
        callerror InvalidAge(-5)
    } except InvalidAge as e {
        call printer(e.message)
    }
}
"#).expect("program ran");
    assert_eq!(out.trim(), "Invalid age: -5");
}

#[test]
fn assertions() {
    let out = run(r#"
fn Main() {
    abl x = 5
    assert x > 0, "x must be positive"
    call printer("passed")
}
"#).expect("program ran");
    assert_eq!(out.trim(), "passed");

    let err = run(r#"
fn Main() {
    assert False, "always fails"
}
"#);
    assert!(err.is_err(), "a failed assertion stops the program");
    assert!(err.unwrap_err().contains("AssertionError"));
}

#[test]
fn membership_and_identity() {
    let out = run(r#"
fn Main() {
    abl items = [1, 2, 3]
    call printer(2 in items)
    call printer(5 in items)
    call printer(5 not in items)
    call printer("ell" in "hello")
    abl a = [1, 2]
    abl b = a
    call printer(a is b)
    abl c = [1, 2]
    call printer(a is c)
    call printer(a == c)
}
"#).expect("program ran");
    assert_eq!(out.trim().lines().collect::<Vec<_>>(),
        vec!["True", "False", "True", "True", "True", "False", "True"]);
}

#[test]
fn builtins() {
    let out = run(r#"
fn Main() {
    call printer(call len([1, 2, 3]))
    call printer(call max([3, 1, 2]))
    call printer(call min([3, 1, 2]))
    call printer(call sum([1, 2, 3]))
    call printer(call abs(-5))
    call printer(call sorted([3, 1, 2]))
    call printer(call type(1))
    call printer(call type("x"))
    call printer(call int("42"))
    call printer(call float(3))
    call printer(call str(42))
    call printer(call bool(0))
    call printer(call isinstance(1, int))
    call printer(call isinstance("x", str))
    call printer(call range(3))
    call printer(call list(call range(2)))
    abl pairs = [1, 2, 3]
    call printer(call enumerate(pairs))
}
"#).expect("program ran");
    assert_eq!(out.trim().lines().collect::<Vec<_>>(), vec![
        "3", "3", "1", "6", "5", "[1, 2, 3]", "int", "str", "42", "3.0",
        "42", "False", "True", "True", "[0, 1, 2]", "[0, 1]",
        "[(0, 1), (1, 2), (2, 3)]"
    ]);
}

#[test]
fn syntax_error_reporting() {
    // A missing expression after `=` must be rejected.
    let result = compiler::compile("fn Main() { abl x = }", None);
    assert!(result.is_err(), "expected a syntax error");
}

#[test]
fn runtime_error_reporting() {
    let err = run(r#"
fn Main() {
    abl x = [1, 2]
    call printer(x[10])
}
"#);
    assert!(err.is_err());
    assert!(err.unwrap_err().contains("IndexError"));
}

#[test]
fn undefined_name_error() {
    let err = run(r#"
fn Main() {
    call printer(missing)
}
"#);
    assert!(err.is_err());
    assert!(err.unwrap_err().contains("NameError"));
}

#[test]
fn division_by_zero() {
    let err = run(r#"
fn Main() {
    call printer(1 // 0)
}
"#);
    assert!(err.is_err());
    assert!(err.unwrap_err().contains("ZeroDivisionError"));
}

#[test]
fn module_imports() {
    // Writes two temporary modules and imports them.
    let dir = std::env::temp_dir().join(format!("ztest_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    let lib = dir.join("mymath.z");
    std::fs::write(&lib, r#"
const SCALE = 2

fn Triple(n) {
    return n * 3
}
"#).unwrap();

    let main = dir.join("main.z");
    std::fs::write(&main, r#"
import mymath
from mymath import Triple

fn Main() {
    call printer(call mymath.Triple(5))
    call printer(call Triple(4))
    call printer(mymath.SCALE)
}
"#).unwrap();

    let source = std::fs::read_to_string(&main).unwrap();
    let module = compiler::compile(&source, Some(&format!("{}", main.display()))).unwrap();
    let mut vm = Vm::new().with_base_dir(dir.clone());
    vm.install_builtins();
    // stdout is used by `printer`, so just check the run succeeds.
    let result = vm.run(&module);
    std::fs::remove_dir_all(&dir).ok();
    assert!(result.is_ok(), "imports failed: {:?}", result.err().map(|e| e.report()));
}
