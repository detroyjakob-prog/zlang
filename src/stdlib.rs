//! Z standard library: `math`, `random`, `time`, `files`, `json`, `os`,
//! `strings`, `printer` and `gg`.

use crate::value::{BuiltinFn, Map, Value, ValueKey};
use std::collections::HashMap;
use std::rc::Rc;

use crate::error::ZResult;
use crate::vm::Vm;

fn str_of_key(k: &ValueKey) -> String {
    match k {
        ValueKey::Str(s) => s.clone(),
        ValueKey::Int(i) => i.to_string(),
        ValueKey::Float(b) => crate::vm::format_float(f64::from_bits(*b)),
        ValueKey::Bool(b) => b.to_string(),
        ValueKey::None => "null".to_string(),
        ValueKey::Tuple(t) => {
            let parts: Vec<String> = t.iter().map(str_of_key).collect();
            format!("[{}]", parts.join(","))
        }
    }
}

pub fn install(vm: &mut Vm) {
    math::install(vm);
    random::install(vm);
    time::install(vm);
    strings::install(vm);
    files::install(vm);
    json::install(vm);
    os::install(vm);
}

/// A simple macro-free helper for defining a module of plain functions.
pub fn define_module(vm: &mut Vm, name: &str, entries: Vec<(&str, BuiltinFn)>) {
    let mut fields = Map::new();
    for (key, func) in entries {
        fields.insert(ValueKey::Str(key.to_string()), Value::Builtin(Rc::new(func)));
    }
    let value = Value::dict(fields);
    vm.globals.borrow_mut().insert(name.to_string(), value);
    // Bare names are also exported so `from math import sqrt` works.
    for (key, func) in map_entries(&name) {
        let _ = func;
        let _ = key;
    }
}

fn map_entries(_name: &str) -> Vec<(String, BuiltinFn)> {
    Vec::new()
}

/// `log(x)` with an optional base: `log(x)` is natural, `log(x, b)` is base `b`.
pub fn log_builtin() -> BuiltinFn {
    BuiltinFn {
        name: "log".to_string(),
        arity: None,
        members: Rc::new(HashMap::new()),
        func: Rc::new(|_vm, args, _kw| {
            let x = args[0].as_float();
            match args.get(1) {
                Some(b) => Ok(crate::vm::from_float(x.log(b.as_float()))),
                None => Ok(crate::vm::from_float(x.ln())),
            }
        }),
    }
}

/// Register the names a module exposes so `from math import sqrt, pi` works.
///
/// The module itself is a dict namespace; each member is also bound at top
/// level, but a member never shadows the module it lives in.
pub fn install_module(vm: &mut Vm, module: &str, entries: Vec<(&str, BuiltinFn)>) {
    install_module_named(vm, module, entries);
}

fn install_module_named(vm: &mut Vm, module: &str, entries: Vec<(&str, BuiltinFn)>) {
    let members: Vec<(String, Value)> = entries
        .into_iter()
        .map(|(k, f)| (k.to_string(), Value::Builtin(Rc::new(f))))
        .collect();

    // The module namespace: used by `import math` then `call math.sqrt(4)`.
    let mut fields = Map::new();
    for (k, v) in &members {
        fields.insert(ValueKey::Str(k.clone()), v.clone());
    }
    let namespace = Value::dict(fields);
    vm.globals
        .borrow_mut()
        .insert(module.to_string(), namespace.clone());

    // Bind each member at top level for `from math import sqrt`, never
    // overwriting the module's own name.
    for (k, v) in members {
        if k == module {
            continue;
        }
        vm.globals.borrow_mut().insert(k, v);
    }
}

pub fn unary(
    name: &str,
    f: impl Fn(f64) -> f64 + 'static,
) -> BuiltinFn {
    BuiltinFn {
        name: name.to_string(),
        arity: Some(1),
        members: Rc::new(HashMap::new()),
        func: Rc::new(move |_vm, args, _kw| {
            let x = args[0].as_float();
            Ok(crate::vm::from_float(f(x)))
        }),
    }
}

mod math {
    use super::*;

    pub fn install(vm: &mut Vm) {
        let mut e: Vec<(&str, BuiltinFn)> = Vec::new();
        e.push(("pi", const_fn("pi", std::f64::consts::PI)));
        e.push(("e", const_fn("e", std::f64::consts::E)));
        e.push(("tau", const_fn("tau", std::f64::consts::TAU)));
        e.push(("inf", const_fn("inf", f64::INFINITY)));
        e.push(("sqrt", super::unary("sqrt", f64::sqrt)));
        e.push(("sin", super::unary("sin", f64::sin)));
        e.push(("cos", super::unary("cos", f64::cos)));
        e.push(("tan", super::unary("tan", f64::tan)));
        e.push(("asin", super::unary("asin", f64::asin)));
        e.push(("acos", super::unary("acos", f64::acos)));
        e.push(("atan", super::unary("atan", f64::atan)));
        e.push(("exp", super::unary("exp", f64::exp)));
        e.push(("log", super::log_builtin()));
        e.push(("log2", super::unary("log2", f64::log2)));
        e.push(("log10", super::unary("log10", f64::log10)));
        e.push(("floor", floor_fn()));
        e.push(("ceil", ceil_fn()));
        e.push(("abs", super::unary("abs", f64::abs)));
        e.push(("pow", pow_fn()));
        e.push(("gcd", gcd_fn()));
        e.push(("lcm", lcm_fn()));
        e.push(("factorial", factorial_fn()));
        e.push(("isnan", isnan_fn()));
        e.push(("isinf", isinf_fn()));
        e.push(("degrees", super::unary("degrees", f64::to_degrees)));
        e.push(("radians", super::unary("radians", f64::to_radians)));
        super::install_module(vm, "math", e);
    }

    fn const_fn(name: &str, v: f64) -> BuiltinFn {
        BuiltinFn {
            name: name.to_string(),
            arity: Some(0),
            members: Rc::new(HashMap::new()),
            func: Rc::new(move |_vm, _args, _kw| Ok(crate::vm::from_float(v))),
        }
    }

    fn floor_fn() -> BuiltinFn {
        BuiltinFn {
            name: "floor".to_string(),
            arity: Some(1),
            members: Rc::new(HashMap::new()),
            func: Rc::new(|_vm, args, _kw| Ok(Value::Int(args[0].as_float().floor() as i64))),
        }
    }

    fn ceil_fn() -> BuiltinFn {
        BuiltinFn {
            name: "ceil".to_string(),
            arity: Some(1),
            members: Rc::new(HashMap::new()),
            func: Rc::new(|_vm, args, _kw| Ok(Value::Int(args[0].as_float().ceil() as i64))),
        }
    }

    fn pow_fn() -> BuiltinFn {
        BuiltinFn {
            name: "pow".to_string(),
            arity: Some(2),
            members: Rc::new(HashMap::new()),
            func: Rc::new(|_vm, args, _kw| {
                let r = args[0].as_float().powf(args[1].as_float());
                Ok(crate::vm::from_float(r))
            }),
        }
    }

    fn gcd_fn() -> BuiltinFn {
        BuiltinFn {
            name: "gcd".to_string(),
            arity: None,
            members: Rc::new(HashMap::new()),
            func: Rc::new(|_vm, args, _kw| {
                let mut g: i64 = 0;
                for a in args {
                    g = gcd(g, a.as_float() as i64);
                }
                Ok(Value::Int(g.abs()))
            }),
        }
    }

    fn lcm_fn() -> BuiltinFn {
        BuiltinFn {
            name: "lcm".to_string(),
            arity: None,
            members: Rc::new(HashMap::new()),
            func: Rc::new(|_vm, args, _kw| {
                let mut l: i64 = 1;
                for a in args {
                    let n = a.as_float() as i64;
                    if n == 0 {
                        return Ok(Value::Int(0));
                    }
                    l = (l / gcd(l, n).max(1) * n).abs();
                }
                Ok(Value::Int(l))
            }),
        }
    }

    fn factorial_fn() -> BuiltinFn {
        BuiltinFn {
            name: "factorial".to_string(),
            arity: Some(1),
            members: Rc::new(HashMap::new()),
            func: Rc::new(|_vm, args, _kw| {
                let n = args[0].as_float() as i64;
                if n < 0 {
                    return Err(crate::error::ZError::raised(
                        "ValueError",
                        "factorial() not defined for negative values",
                    ));
                }
                let mut acc: i64 = 1;
                let mut i: i64 = 2;
                while i <= n {
                    acc = acc.checked_mul(i).ok_or_else(|| {
                        crate::error::ZError::raised("OverflowError", "math range error")
                    })?;
                    i += 1;
                }
                Ok(Value::Int(acc))
            }),
        }
    }

    fn isnan_fn() -> BuiltinFn {
        BuiltinFn {
            name: "isnan".to_string(),
            arity: Some(1),
            members: Rc::new(HashMap::new()),
            func: Rc::new(|_vm, args, _kw| Ok(Value::Bool(args[0].as_float().is_nan()))),
        }
    }

    fn isinf_fn() -> BuiltinFn {
        BuiltinFn {
            name: "isinf".to_string(),
            arity: Some(1),
            members: Rc::new(HashMap::new()),
            func: Rc::new(|_vm, args, _kw| Ok(Value::Bool(args[0].as_float().is_infinite()))),
        }
    }

    fn gcd(a: i64, b: i64) -> i64 {
        if b == 0 {
            a
        } else {
            gcd(b, a % b)
        }
    }
}

mod time {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    pub fn install(vm: &mut Vm) {
        let mut e: Vec<(&str, BuiltinFn)> = Vec::new();
        e.push(("time", BuiltinFn {
            name: "time".to_string(),
            arity: Some(0),
            members: Rc::new(HashMap::new()),
            func: Rc::new(|_vm, _args, _kw| {
                let d = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default();
                Ok(crate::vm::from_float(d.as_secs_f64()))
            }),
        }));
        e.push(("now", BuiltinFn {
            name: "now".to_string(),
            arity: Some(0),
            members: Rc::new(HashMap::new()),
            func: Rc::new(|_vm, _args, _kw| {
                let d = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default();
                Ok(crate::vm::from_float(d.as_secs_f64()))
            }),
        }));
        e.push(("sleep", BuiltinFn {
            name: "sleep".to_string(),
            arity: Some(1),
            members: Rc::new(HashMap::new()),
            func: Rc::new(|_vm, args, _kw| {
                std::thread::sleep(std::time::Duration::from_secs_f64(args[0].as_float()));
                Ok(Value::None)
            }),
        }));
        e.push(("pause", BuiltinFn {
            name: "pause".to_string(),
            arity: Some(1),
            members: Rc::new(HashMap::new()),
            func: Rc::new(|_vm, args, _kw| {
                std::thread::sleep(std::time::Duration::from_secs_f64(args[0].as_float()));
                Ok(Value::None)
            }),
        }));
        e.push(("format", BuiltinFn {
            name: "format".to_string(),
            arity: Some(2),
            members: Rc::new(HashMap::new()),
            func: Rc::new(|vm, args, _kw| {
                let secs = args[0].as_float();
                let pattern = match &args[1] {
                    Value::Str(s) => (**s).clone(),
                    _ => "%Y-%m-%d %H:%M:%S".to_string(),
                };
                Ok(Value::str(format_time(vm, secs, &pattern)))
            }),
        }));
        super::install_module(vm, "time", e);
    }

    pub fn format_time(_vm: &Vm, secs: f64, pattern: &str) -> String {
        // Naive formatting; `%Y-%m-%d %H:%M:%S` with only whole seconds supported.
        let total = secs as i64;
        let days = total / 86_400;
        let within = total % 86_400;
        let (h, m, s) = (within / 3_600, (within % 3_600) / 60, within % 60);
        // Days since epoch -> civil date (Howard Hinnant's algorithm).
        let z = days + 719_468;
        let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = if month <= 2 { y + 1 } else { y };
        pattern
            .replace("%Y", &format!("{:04}", year))
            .replace("%m", &format!("{:02}", month))
            .replace("%d", &format!("{:02}", d))
            .replace("%H", &format!("{:02}", h))
            .replace("%M", &format!("{:02}", m))
            .replace("%S", &format!("{:02}", s))
    }
}

mod random {
    use super::*;

    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            // xorshift64*
            let mut x = self.0;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            self.0 = x;
            x.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }

        fn below(&mut self, n: u64) -> u64 {
            if n == 0 {
                return 0;
            }
            self.next() % n
        }
    }

    fn seed_from_time() -> u64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        now.as_nanos() as u64 | 1
    }

    pub fn install(vm: &mut Vm) {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        vm.rng_seed = Some(seed);
        let mut e: Vec<(&str, BuiltinFn)> = Vec::new();
        e.push(("random", BuiltinFn {
            name: "random".to_string(),
            arity: Some(0),
            members: Rc::new(HashMap::new()),
            func: Rc::new(|vm, _args, _kw| {
                let seed = vm.rng_seed.get_or_insert_with(seed_from_time);
                let mut rng = Rng(*seed);
                let v = rng.next();
                *seed = rng.0;
                Ok(crate::vm::from_float((v >> 11) as f64 / (1u64 << 53) as f64))
            }),
        }));
        e.push(("randint", BuiltinFn {
            name: "randint".to_string(),
            arity: Some(2),
            members: Rc::new(HashMap::new()),
            func: Rc::new(|vm, args, _kw| {
                let a = args[0].as_float() as i64;
                let b = args[1].as_float() as i64;
                let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
                let span = (hi - lo + 1).max(1) as u64;
                let seed = vm.rng_seed.get_or_insert_with(seed_from_time);
                let mut rng = Rng(*seed);
                let r = rng.below(span) as i64;
                *seed = rng.0;
                Ok(Value::Int(lo + r))
            }),
        }));
        e.push(("choice", BuiltinFn {
            name: "choice".to_string(),
            arity: Some(1),
            members: Rc::new(HashMap::new()),
            func: Rc::new(|vm, args, _kw| {
                let items = vm.iterate(&args[0])?;
                if items.is_empty() {
                    return Err(crate::error::ZError::raised(
                        "IndexError",
                        "cannot choose from an empty sequence",
                    ));
                }
                let seed = vm.rng_seed.get_or_insert_with(seed_from_time);
                let mut rng = Rng(*seed);
                let i = rng.below(items.len() as u64) as usize;
                *seed = rng.0;
                Ok(items[i].clone())
            }),
        }));
        e.push(("shuffle", BuiltinFn {
            name: "shuffle".to_string(),
            arity: Some(1),
            members: Rc::new(HashMap::new()),
            func: Rc::new(|vm, args, _kw| {
                let mut items = vm.iterate(&args[0])?;
                let seed = vm.rng_seed.get_or_insert_with(seed_from_time);
                let mut rng = Rng(*seed);
                for i in (1..items.len()).rev() {
                    let j = rng.below((i + 1) as u64) as usize;
                    items.swap(i, j);
                }
                *seed = rng.0;
                vm.replace_container(&args[0], Value::list(items))?;
                Ok(Value::None)
            }),
        }));
        e.push(("uniform", BuiltinFn {
            name: "uniform".to_string(),
            arity: Some(2),
            members: Rc::new(HashMap::new()),
            func: Rc::new(|vm, args, _kw| {
                let a = args[0].as_float();
                let b = args[1].as_float();
                let seed = vm.rng_seed.get_or_insert_with(seed_from_time);
                let mut rng = Rng(*seed);
                let t = (rng.next() >> 11) as f64 / (1u64 << 53) as f64;
                *seed = rng.0;
                Ok(crate::vm::from_float(a + t * (b - a)))
            }),
        }));
        e.push(("seed", BuiltinFn {
            name: "seed".to_string(),
            arity: Some(1),
            members: Rc::new(HashMap::new()),
            func: Rc::new(|vm, args, _kw| {
                vm.rng_seed = Some(args[0].as_float() as u64 | 1);
                Ok(Value::None)
            }),
        }));
        e.push(("sample", BuiltinFn {
            name: "sample".to_string(),
            arity: Some(2),
            members: Rc::new(HashMap::new()),
            func: Rc::new(|vm, args, _kw| {
                let items = vm.iterate(&args[0])?;
                let k = args[1].as_float() as usize;
                if k > items.len() {
                    return Err(crate::error::ZError::raised(
                        "ValueError",
                        "sample larger than population",
                    ));
                }
                let seed = vm.rng_seed.get_or_insert_with(seed_from_time);
                let mut rng = Rng(*seed);
                let mut pool = items.clone();
                let mut out = Vec::with_capacity(k);
                for _ in 0..k {
                    let i = rng.below(pool.len() as u64) as usize;
                    out.push(pool.remove(i));
                }
                *seed = rng.0;
                Ok(Value::list(out))
            }),
        }));
        super::install_module(vm, "random", e);
    }
}

mod strings {
    use super::*;

    pub fn install(vm: &mut Vm) {
        let digits = String::from("0123456789");
        let letters = String::from("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ");
        let punctuation = String::from("!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~");
        let mut e: Vec<(&str, BuiltinFn)> = Vec::new();
        for (name, value) in [
            ("digits", digits),
            ("letters", letters),
            ("punctuation", punctuation),
        ] {
            e.push((name, BuiltinFn {
                name: name.to_string(),
                arity: Some(0),
                members: Rc::new(HashMap::new()),
                func: Rc::new(move |_vm, _args, _kw| Ok(Value::str(value.clone()))),
            }));
        }
        super::install_module(vm, "strings", e);
    }
}

mod files {
    use super::*;

    fn io_err(e: std::io::Error) -> crate::error::ZError {
        crate::error::ZError::raised("OSError", format!("{}", e))
    }

    pub fn install(vm: &mut Vm) {
        let mut e: Vec<(&str, BuiltinFn)> = Vec::new();
        e.push(("read", BuiltinFn {
            name: "read".to_string(),
            arity: Some(1),
            members: Rc::new(HashMap::new()),
            func: Rc::new(|vm, args, _kw| {
                let path = resolve(vm, &args[0])?;
                match std::fs::read_to_string(&path) {
                    Ok(text) => Ok(Value::str(text)),
                    Err(err) => Err(io_err(err)),
                }
            }),
        }));
        e.push(("write", BuiltinFn {
            name: "write".to_string(),
            arity: Some(2),
            members: Rc::new(HashMap::new()),
            func: Rc::new(|vm, args, _kw| {
                let path = resolve(vm, &args[0])?;
                let text = vm.to_string_value(&args[1]);
                match std::fs::write(&path, text) {
                    Ok(()) => Ok(Value::None),
                    Err(err) => Err(io_err(err)),
                }
            }),
        }));
        e.push(("append", BuiltinFn {
            name: "append".to_string(),
            arity: Some(2),
            members: Rc::new(HashMap::new()),
            func: Rc::new(|vm, args, _kw| {
                use std::io::Write;
                let path = resolve(vm, &args[0])?;
                let text = vm.to_string_value(&args[1]);
                let mut f = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .map_err(io_err)?;
                f.write_all(text.as_bytes()).map_err(io_err)?;
                Ok(Value::None)
            }),
        }));
        e.push(("exists", BuiltinFn {
            name: "exists".to_string(),
            arity: Some(1),
            members: Rc::new(HashMap::new()),
            func: Rc::new(|vm, args, _kw| {
                let path = resolve(vm, &args[0])?;
                Ok(Value::Bool(path.exists()))
            }),
        }));
        e.push(("delete", BuiltinFn {
            name: "delete".to_string(),
            arity: Some(1),
            members: Rc::new(HashMap::new()),
            func: Rc::new(|vm, args, _kw| {
                let path = resolve(vm, &args[0])?;
                if path.is_file() {
                    std::fs::remove_file(&path).map_err(io_err)?;
                }
                Ok(Value::None)
            }),
        }));
        e.push(("lines", BuiltinFn {
            name: "lines".to_string(),
            arity: Some(1),
            members: Rc::new(HashMap::new()),
            func: Rc::new(|vm, args, _kw| {
                let path = resolve(vm, &args[0])?;
                let text = std::fs::read_to_string(&path).map_err(io_err)?;
                let items: Vec<Value> = text
                    .lines()
                    .map(|l| Value::str(l.to_string()))
                    .collect();
                Ok(Value::list(items))
            }),
        }));
        e.push(("listdir", BuiltinFn {
            name: "listdir".to_string(),
            arity: Some(1),
            members: Rc::new(HashMap::new()),
            func: Rc::new(|vm, args, _kw| {
                let path = resolve(vm, &args[0])?;
                let entries = std::fs::read_dir(&path).map_err(io_err)?;
                let mut out = Vec::new();
                for e in entries {
                    if let Ok(e) = e {
                        out.push(Value::str(e.file_name().to_string_lossy().to_string()));
                    }
                }
                Ok(Value::list(out))
            }),
        }));
        e.push(("open", BuiltinFn {
            name: "open".to_string(),
            arity: Some(2),
            members: Rc::new(HashMap::new()),
            func: Rc::new(|vm, args, _kw| {
                // Returns a table with read/close methods.
                let path = resolve(vm, &args[0])?;
                Ok(build_file_table(vm, path))
            }),
        }));
        super::install_module(vm, "files", e);
    }

    fn resolve(vm: &Vm, v: &Value) -> crate::error::ZResult<std::path::PathBuf> {
        match v {
            Value::Str(s) => {
                let p = std::path::PathBuf::from(&**s);
                if p.is_absolute() {
                    Ok(p)
                } else {
                    Ok(vm.entry_dir.join(p))
                }
            }
            other => Err(crate::error::ZError::raised(
                "TypeError",
                format!("expected str path, not '{}'", other.type_name()),
            )),
        }
    }

    fn build_file_table(_vm: &Vm, path: std::path::PathBuf) -> Value {
        let mut fields = Map::new();
        let p1 = path.clone();
        fields.insert(
            ValueKey::Str("read".to_string()),
            Value::Builtin(Rc::new(BuiltinFn {
                name: "read".to_string(),
                arity: Some(0),
                members: Rc::new(HashMap::new()),
                func: Rc::new(move |_vm, _args, _kw| match std::fs::read_to_string(&p1) {
                    Ok(text) => Ok(Value::str(text)),
                    Err(err) => Err(io_err(err)),
                }),
            })),
        );
        let p2 = path.clone();
        fields.insert(
            ValueKey::Str("write".to_string()),
            Value::Builtin(Rc::new(BuiltinFn {
                name: "write".to_string(),
                arity: Some(1),
                members: Rc::new(HashMap::new()),
                func: Rc::new(move |vm, args, _kw| {
                    let text = vm.to_string_value(&args[0]);
                    std::fs::write(&p2, text).map_err(io_err)?;
                    Ok(Value::None)
                }),
            })),
        );
        fields.insert(
            ValueKey::Str("close".to_string()),
            Value::Builtin(Rc::new(BuiltinFn {
                name: "close".to_string(),
                arity: Some(0),
                members: Rc::new(HashMap::new()),
                func: Rc::new(|_vm, _args, _kw| Ok(Value::None)),
            })),
        );
        Value::dict(fields)
    }
}

mod json {
    use super::*;

    pub fn install(vm: &mut Vm) {
        let mut e: Vec<(&str, BuiltinFn)> = Vec::new();
        e.push(("parse", BuiltinFn {
            name: "parse".to_string(),
            arity: Some(1),
            members: Rc::new(HashMap::new()),
            func: Rc::new(|vm, args, _kw| {
                let text = vm.to_string_value(&args[0]);
                parse_json(&text)
            }),
        }));
        e.push(("stringify", BuiltinFn {
            name: "stringify".to_string(),
            arity: Some(2),
            members: Rc::new(HashMap::new()),
            func: Rc::new(|_vm, args, _kw| {
                let indent = args.get(1).map(|v| v.as_float() as usize).unwrap_or(0);
                Ok(Value::str(stringify(&args[0], indent, 0)))
            }),
        }));
        super::install_module(vm, "json", e);
    }

    pub fn parse_json(text: &str) -> ZResult<Value> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(crate::error::ZError::raised("ValueError", "empty JSON input"));
        }
        let mut i = 0usize;
        let chars: Vec<char> = trimmed.chars().collect();
        let v = parse_value(&chars, &mut i)?;
        Ok(v)
    }

    fn skip_ws(chars: &[char], i: &mut usize) {
        while *i < chars.len() && chars[*i].is_whitespace() {
            *i += 1;
        }
    }

    fn parse_value(chars: &[char], i: &mut usize) -> ZResult<Value> {
        skip_ws(chars, i);
        match chars.get(*i) {
            None => Err(crate::error::ZError::raised("ValueError", "unexpected end of JSON")),
            Some('{') => parse_object(chars, i),
            Some('[') => parse_array(chars, i),
            Some('"') => parse_string_lit(chars, i).map(Value::str),
            Some('t') if chars[*i..].starts_with(&['t', 'r', 'u', 'e']) => {
                *i += 4;
                Ok(Value::Bool(true))
            }
            Some('f') if chars[*i..].starts_with(&['f', 'a', 'l', 's', 'e']) => {
                *i += 5;
                Ok(Value::Bool(false))
            }
            Some('n') if chars[*i..].starts_with(&['n', 'u', 'l', 'l']) => {
                *i += 4;
                Ok(Value::None)
            }
            Some(_) => parse_number(chars, i),
        }
    }

    fn parse_object(chars: &[char], i: &mut usize) -> ZResult<Value> {
        *i += 1; // '{'
        let mut map = Map::new();
        skip_ws(chars, i);
        if chars.get(*i) == Some(&'}') {
            *i += 1;
            return Ok(Value::dict(map));
        }
        loop {
            skip_ws(chars, i);
            let key = parse_string_lit(chars, i)?;
            skip_ws(chars, i);
            if chars.get(*i) != Some(&':') {
                return Err(crate::error::ZError::raised("ValueError", "expected ':' in object"));
            }
            *i += 1;
            let value = parse_value(chars, i)?;
            map.insert(ValueKey::Str(key), value);
            skip_ws(chars, i);
            match chars.get(*i) {
                Some(',') => {
                    *i += 1;
                }
                Some('}') => {
                    *i += 1;
                    break;
                }
                _ => {
                    return Err(crate::error::ZError::raised(
                        "ValueError",
                        "expected ',' or '}' in object",
                    ))
                }
            }
        }
        Ok(Value::dict(map))
    }

    fn parse_array(chars: &[char], i: &mut usize) -> ZResult<Value> {
        *i += 1; // '['
        let mut items = Vec::new();
        skip_ws(chars, i);
        if chars.get(*i) == Some(&']') {
            *i += 1;
            return Ok(Value::list(items));
        }
        loop {
            let v = parse_value(chars, i)?;
            items.push(v);
            skip_ws(chars, i);
            match chars.get(*i) {
                Some(',') => {
                    *i += 1;
                }
                Some(']') => {
                    *i += 1;
                    break;
                }
                _ => {
                    return Err(crate::error::ZError::raised(
                        "ValueError",
                        "expected ',' or ']' in array",
                    ))
                }
            }
        }
        Ok(Value::list(items))
    }

    fn parse_string_lit(chars: &[char], i: &mut usize) -> ZResult<String> {
        if chars.get(*i) != Some(&'"') {
            return Err(crate::error::ZError::raised("ValueError", "expected string"));
        }
        *i += 1;
        let mut out = String::new();
        while *i < chars.len() {
            match chars[*i] {
                '"' => {
                    *i += 1;
                    return Ok(out);
                }
                '\\' => {
                    *i += 1;
                    match chars.get(*i) {
                        Some('n') => out.push('\n'),
                        Some('t') => out.push('\t'),
                        Some('r') => out.push('\r'),
                        Some('"') => out.push('"'),
                        Some('\\') => out.push('\\'),
                        Some('/') => out.push('/'),
                        Some('u') => {
                            let hex: String = chars[*i + 1..(*i + 5).min(chars.len())]
                                .iter()
                                .collect();
                            *i += 4;
                            if let Ok(n) = u32::from_str_radix(&hex, 16) {
                                if let Some(c) = char::from_u32(n) {
                                    out.push(c);
                                }
                            }
                        }
                        _ => {}
                    }
                    *i += 1;
                }
                c => {
                    out.push(c);
                    *i += 1;
                }
            }
        }
        Err(crate::error::ZError::raised("ValueError", "unterminated string"))
    }

    fn parse_number(chars: &[char], i: &mut usize) -> ZResult<Value> {
        let start = *i;
        while *i < chars.len()
            && (chars[*i].is_ascii_digit() || matches!(chars[*i], '-' | '+' | '.' | 'e' | 'E'))
        {
            *i += 1;
        }
        let raw: String = chars[start..*i].iter().collect();
        if raw.contains('.') || raw.contains('e') || raw.contains('E') {
            raw.parse::<f64>()
                .map(crate::vm::from_float)
                .map_err(|_| crate::error::ZError::raised("ValueError", format!("invalid number '{}'", raw)))
        } else {
            raw.parse::<i64>()
                .map(Value::Int)
                .map_err(|_| crate::error::ZError::raised("ValueError", format!("invalid number '{}'", raw)))
        }
    }

    fn stringify(v: &Value, indent: usize, level: usize) -> String {
        let pad = if indent > 0 {
            "\n".to_string() + &" ".repeat(indent * level)
        } else {
            String::new()
        };
        let close_pad = if indent > 0 {
            "\n".to_string() + &" ".repeat(indent * (level.saturating_sub(1)))
        } else {
            String::new()
        };
        let sep = if indent > 0 { ": " } else { ":" };
        match v {
            Value::None => "null".to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => crate::vm::format_float(*f),
            Value::Str(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
            Value::List(l) | Value::Tuple(l) => {
                if l.is_empty() {
                    return "[]".to_string();
                }
                let items: Vec<String> = l
                    .iter()
                    .map(|e| {
                        format!(
                            "{}{}",
                            if indent > 0 {
                                "\n".to_string() + &" ".repeat(indent * (level + 1))
                            } else {
                                String::new()
                            },
                            stringify(e, indent, level + 1)
                        )
                    })
                    .collect();
                format!("[{}{}{}]", pad, items.join(","), close_pad)
            }
            Value::Set(s) => {
                let items: Vec<String> = s
                    .iter()
                    .map(|e| stringify(e, indent, level))
                    .collect();
                format!("[{}]", items.join(","))
            }
            Value::Dict(d) => {
                if d.is_empty() {
                    return "{}".to_string();
                }
                let items: Vec<String> = d
                    .iter()
                    .map(|(k, val)| {
                        format!(
                            "{}\"{}\"{}",
                            if indent > 0 {
                                "\n".to_string() + &" ".repeat(indent * (level + 1))
                            } else {
                                String::new()
                            },
                            str_of_key(&k),
                            if indent > 0 {
                                sep.to_string()
                            } else {
                                String::new()
                            },
                        ) + &stringify(val, indent, level + 1)
                    })
                    .collect();
                format!("{{{}{}{}}}", pad, items.join(","), close_pad)
            }
            other => format!("\"{}\"", other),
        }
    }
}

mod os {
    use super::*;

    pub fn install(vm: &mut Vm) {
        let mut e: Vec<(&str, BuiltinFn)> = Vec::new();
        e.push(("cwd", BuiltinFn {
            name: "cwd".to_string(),
            arity: Some(0),
            members: Rc::new(HashMap::new()),
            func: Rc::new(|_vm, _args, _kw| {
                let cwd = std::env::current_dir().unwrap_or_default();
                Ok(Value::str(cwd.to_string_lossy().to_string()))
            }),
        }));
        e.push(("env", BuiltinFn {
            name: "env".to_string(),
            arity: Some(1),
            members: Rc::new(HashMap::new()),
            func: Rc::new(|vm, args, _kw| {
                let name = vm.to_string_value(&args[0]);
                match std::env::var(&name) {
                    Ok(v) => Ok(Value::str(v)),
                    Err(_) => Ok(Value::None),
                }
            }),
        }));
        e.push(("listdir", BuiltinFn {
            name: "listdir".to_string(),
            arity: Some(1),
            members: Rc::new(HashMap::new()),
            func: Rc::new(|vm, args, _kw| {
                let path = vm.to_string_value(&args[0]);
                let entries = std::fs::read_dir(&path)
                    .map_err(|e| crate::error::ZError::raised("OSError", format!("{}", e)))?;
                let mut out = Vec::new();
                for e in entries {
                    if let Ok(e) = e {
                        out.push(Value::str(e.file_name().to_string_lossy().to_string()));
                    }
                }
                Ok(Value::list(out))
            }),
        }));
        e.push(("platform", BuiltinFn {
            name: "platform".to_string(),
            arity: Some(0),
            members: Rc::new(HashMap::new()),
            func: Rc::new(|_vm, _args, _kw| {
                let p = if cfg!(windows) {
                    "windows"
                } else if cfg!(target_os = "macos") {
                    "darwin"
                } else {
                    "linux"
                };
                Ok(Value::str(p))
            }),
        }));
        super::install_module(vm, "os", e);
    }
}