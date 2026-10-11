//! Built-in functions for the Z language.

pub mod collections;
pub mod string;

use crate::value::{BuiltinFn, Map, Value, ValueKey};
use std::collections::HashMap;
use std::rc::Rc;

use crate::error::{ZError, ZResult};
use crate::vm::Vm;

/// Register a function in `globals` as a callable value.
pub fn define(vm: &mut Vm, name: &str, func: BuiltinFn) {
    vm.globals
        .borrow_mut()
        .insert(name.to_string(), Value::Builtin(Rc::new(func)));
}

pub fn install(vm: &mut Vm) {
    install_printers(vm);
    install_conversions(vm);
    install_numeric(vm);
    install_iteration(vm);
    install_types(vm);
    install_misc(vm);
    crate::stdlib::install(vm);
}

fn simple(
    name: &str,
    arity: Option<usize>,
    f: impl Fn(&mut Vm, &[Value], &Map) -> ZResult<Value> + 'static,
) -> BuiltinFn {
    BuiltinFn {
        name: name.to_string(),
        arity,
        members: Rc::new(HashMap::new()),
        func: Rc::new(f),
    }
}

fn install_printers(vm: &mut Vm) {
    define(vm, "printer", simple("printer", None, |vm, args, kw| {
        let sep = kw
            .get(&ValueKey::Str("sep".to_string()))
            .map(|v| vm.to_string_value(v))
            .unwrap_or_else(|| " ".to_string());
        let end = match kw.get(&ValueKey::Str("end".to_string())) {
            Some(v) => vm.to_string_value(v),
            None => "\n".to_string(),
        };
        let parts: Vec<String> = args.iter().map(|a| vm.to_string_value(a)).collect();
        let line = format!("{}{}", parts.join(&sep), end);
        print!("{}", line);
        use std::io::Write;
        let _ = std::io::stdout().flush();
        Ok(Value::None)
    }));

    // `printer.error(...)`
    let mut error_members = HashMap::new();
    error_members.insert(
        "error".to_string(),
        Value::Builtin(Rc::new(simple("printer.error", None, |vm, args, _kw| {
            let parts: Vec<String> = args.iter().map(|a| vm.to_string_value(a)).collect();
            eprintln!("{}", parts.join(" "));
            Ok(Value::None)
        }))),
    );
    define(vm, "__printer_error", simple("printer.error", None, |_vm, args, _kw| {
        eprintln!("{}", args.len());
        Ok(Value::None)
    }));

    let mut members = HashMap::new();
    members.insert(
        "error".to_string(),
        Value::Builtin(Rc::new(simple("printer.error", None, |vm, args, _kw| {
            let parts: Vec<String> = args.iter().map(|a| vm.to_string_value(a)).collect();
            eprintln!("{}", parts.join(" "));
            Ok(Value::None)
        }))),
    );
    let _ = error_members;
    define(vm, "printer", BuiltinFn {
        name: "printer".to_string(),
        arity: None,
        members: Rc::new(members),
        func: Rc::new(|vm: &mut Vm, args: &[Value], kw: &Map| {
            let sep = kw
                .get(&ValueKey::Str("sep".to_string()))
                .map(|v| vm.to_string_value(v))
                .unwrap_or_else(|| " ".to_string());
            let end = match kw.get(&ValueKey::Str("end".to_string())) {
                Some(v) => vm.to_string_value(v),
                None => "\n".to_string(),
            };
            let parts: Vec<String> = args.iter().map(|a| vm.to_string_value(a)).collect();
            let line = format!("{}{}", parts.join(&sep), end);
            if vm.capture_output {
                vm.output.push_str(&line);
            } else {
                print!("{}", line);
                use std::io::Write;
                let _ = std::io::stdout().flush();
            }
            Ok(Value::None)
        }),
    });
}

fn install_conversions(vm: &mut Vm) {
    define(vm, "int", simple("int", Some(1), |vm, args, _kw| {
        let v = &args[0];
        let n = match v {
            Value::Int(i) => *i,
            Value::Float(f) => {
                if f.is_nan() || f.is_infinite() {
                    return Err(ZError::raised(
                        "ValueError",
                        "cannot convert float NaN or infinity to integer",
                    ));
                }
                *f as i64
            }
            Value::Bool(b) => {
                if *b {
                    1
                } else {
                    0
                }
            }
            Value::Str(s) => s.trim().parse::<i64>().map_err(|_| {
                ZError::raised(
                    "ValueError",
                    format!("invalid literal for int(): '{}'", s),
                )
            })?,
            other => {
                return Err(ZError::raised(
                    "TypeError",
                    format!("int() argument must be a number or string, not '{}'", other.type_name()),
                ))
            }
        };
        let _ = vm;
        Ok(Value::Int(n))
    }));

    define(vm, "float", simple("float", Some(1), |vm, args, _kw| {
        let f = match &args[0] {
            Value::Int(i) => *i as f64,
            Value::Float(x) => *x,
            Value::Bool(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            Value::Str(s) => s.trim().parse::<f64>().map_err(|_| {
                ZError::raised(
                    "ValueError",
                    format!("could not convert string to float: '{}'", s),
                )
            })?,
            other => {
                return Err(ZError::raised(
                    "TypeError",
                    format!("float() argument must be a number or string, not '{}'", other.type_name()),
                ))
            }
        };
        let _ = vm;
        Ok(crate::vm::from_float(f))
    }));

    define(vm, "str", simple("str", Some(1), |vm, args, _kw| {
        Ok(Value::str(vm.to_string_value(&args[0])))
    }));

    define(vm, "bool", simple("bool", Some(1), |_vm, args, _kw| {
        Ok(Value::Bool(args[0].is_truthy()))
    }));

    define(vm, "list", simple("list", None, |vm, args, _kw| {
        let src = if args.is_empty() {
            Value::list(Vec::new())
        } else {
            args[0].clone()
        };
        let items = vm.iterate(&src)?;
        Ok(Value::list(items))
    }));

    define(vm, "tuple", simple("tuple", None, |vm, args, _kw| {
        let src = if args.is_empty() {
            Value::list(Vec::new())
        } else {
            args[0].clone()
        };
        let items = vm.iterate(&src)?;
        Ok(Value::tuple(items))
    }));

    define(vm, "dict", simple("dict", None, |vm, args, _kw| {
        if args.is_empty() {
            return Ok(Value::dict(Map::new()));
        }
        // Copy from another dict.
        let mut out = Map::new();
        if let Value::Dict(d) = &args[0] {
            for (k, v) in d.iter() {
                out.insert(k.clone(), v.clone());
            }
        }
        let _ = vm;
        Ok(Value::dict(out))
    }));

    define(vm, "set", simple("set", None, |vm, args, _kw| {
        if args.is_empty() {
            return Ok(Value::set(Vec::new()));
        }
        let items = vm.iterate(&args[0])?;
        let mut out: Vec<Value> = Vec::new();
        for v in items {
            if !out.iter().any(|o| crate::vm::values_equal(o, &v)) {
                out.push(v);
            }
        }
        Ok(Value::set(out))
    }));

    define(vm, "frozenset", simple("frozenset", None, |vm, args, _kw| {
        if args.is_empty() {
            return Ok(Value::set(Vec::new()));
        }
        let items = vm.iterate(&args[0])?;
        let mut out: Vec<Value> = Vec::new();
        for v in items {
            if !out.iter().any(|o| crate::vm::values_equal(o, &v)) {
                out.push(v);
            }
        }
        Ok(Value::set(out))
    }));
}

fn install_numeric(vm: &mut Vm) {
    define(vm, "len", simple("len", Some(1), |vm, args, _kw| {
        let n = vm.len(&args[0])?;
        Ok(Value::Int(n as i64))
    }));

    define(vm, "abs", simple("abs", Some(1), |_vm, args, _kw| {
        match &args[0] {
            Value::Int(i) => Ok(Value::Int(i.abs())),
            Value::Float(f) => Ok(crate::vm::from_float(f.abs())),
            Value::Bool(b) => Ok(Value::Int(if *b { 1 } else { 0 })),
            other => Err(ZError::raised(
                "TypeError",
                format!("bad operand type for abs(): '{}'", other.type_name()),
            )),
        }
    }));

    define(vm, "min", simple("min", None, |vm, args, _kw| {
        numeric_extreme(vm, args, false)
    }));

    define(vm, "max", simple("max", None, |vm, args, _kw| {
        numeric_extreme(vm, args, true)
    }));

    define(vm, "sum", simple("sum", None, |vm, args, _kw| {
        let src = if args.is_empty() {
            return Ok(Value::Int(0));
        } else {
            args[0].clone()
        };
        let items = vm.iterate(&src)?;
        let mut total_i: i64 = 0;
        let mut total_f: f64 = 0.0;
        let mut is_float = false;
        for v in items {
            match v {
                Value::Int(i) => {
                    if is_float {
                        total_f += i as f64;
                    } else {
                        total_i += i;
                    }
                }
                Value::Float(f) => {
                    if !is_float {
                        total_f += total_i as f64;
                        is_float = true;
                    }
                    total_f += f;
                }
                Value::Bool(b) => {
                    if is_float {
                        total_f += if b { 1.0 } else { 0.0 };
                    } else if b {
                        total_i += 1;
                    }
                }
                other => {
                    return Err(ZError::raised(
                        "TypeError",
                        format!("unsupported operand type(s) for +: 'int' and '{}'", other.type_name()),
                    ))
                }
            }
        }
        Ok(if is_float {
            crate::vm::from_float(total_f)
        } else {
            Value::Int(total_i)
        })
    }));

    define(vm, "round", simple("round", None, |_vm, args, _kw| {
        if args.is_empty() {
            return Ok(Value::Int(0));
        }
        let f = args[0].as_float();
        let ndigits = args.get(1).map(|v| v.as_float()).unwrap_or(0.0) as i32;
        let factor = 10f64.powi(ndigits);
        Ok(crate::vm::from_float((f * factor).round() / factor))
    }));

    define(vm, "pow", simple("pow", Some(3), |_vm, args, _kw| {
        let base = args[0].as_float();
        let exp = args[1].as_float();
        let modulus = args.get(2).map(|v| v.as_float());
        let res = match modulus {
            Some(m) if m != 0.0 => base.powf(exp) % m,
            _ => base.powf(exp),
        };
        Ok(crate::vm::from_float(res))
    }));

    define(vm, "divmod", simple("divmod", Some(2), |_vm, args, _kw| {
        let a = args[0].as_float();
        let b = args[1].as_float();
        if b == 0.0 {
            return Err(ZError::raised(
                "ZeroDivisionError",
                "integer division or modulo by zero",
            ));
        }
        Ok(Value::tuple(vec![
            crate::vm::from_float((a / b).floor()),
            crate::vm::from_float(a % b),
        ]))
    }));

    define(vm, "range", simple("range", None, |_vm, args, _kw| {
        let (start, stop, step) = match args.len() {
            0 => return Ok(Value::list(Vec::new())),
            1 => (0i64, args[0].as_float() as i64, 1i64),
            2 => (args[0].as_float() as i64, args[1].as_float() as i64, 1i64),
            _ => (args[0].as_float() as i64, args[1].as_float() as i64, args[2].as_float() as i64),
        };
        if step == 0 {
            return Err(ZError::raised(
                "ValueError",
                "range() arg 3 must not be zero",
            ));
        }
        let mut out = Vec::new();
        let mut i = start;
        if step > 0 {
            while i < stop {
                out.push(Value::Int(i));
                i += step;
            }
        } else {
            while i > stop {
                out.push(Value::Int(i));
                i += step;
            }
        }
        Ok(Value::list(out))
    }));
}

fn numeric_extreme(vm: &mut Vm, args: &[Value], want_max: bool) -> ZResult<Value> {
    let items = if args.len() == 1 {
        vm.iterate(&args[0])?
    } else {
        args.to_vec()
    };
    if items.is_empty() {
        return Err(ZError::raised(
            "ValueError",
            if want_max {
                "max() arg is an empty sequence"
            } else {
                "min() arg is an empty sequence"
            },
        ));
    }
    let mut best = items[0].clone();
    for v in items.into_iter().skip(1) {
        let cmp = vm.compare_values_direct(&best, &v)?;
        let take = if want_max {
            cmp == std::cmp::Ordering::Less
        } else {
            cmp == std::cmp::Ordering::Greater
        };
        if take {
            best = v;
        }
    }
    Ok(best)
}

fn install_iteration(vm: &mut Vm) {
    define(vm, "sorted", simple("sorted", None, |vm, args, _kw| {
        let items = if args.is_empty() {
            Vec::new()
        } else {
            vm.iterate(&args[0])?
        };
        Ok(Value::list(vm.sort_values(items)?))
    }));

    define(vm, "reversed", simple("reversed", Some(1), |vm, args, _kw| {
        let mut items = vm.iterate(&args[0])?;
        items.reverse();
        Ok(Value::list(items))
    }));

    define(vm, "enumerate", simple("enumerate", None, |vm, args, _kw| {
        let items = if args.is_empty() {
            Vec::new()
        } else {
            vm.iterate(&args[0])?
        };
        let start = args.get(1).map(|v| v.as_float() as i64).unwrap_or(0);
        let out: Vec<Value> = items
            .into_iter()
            .enumerate()
            .map(|(i, v)| Value::tuple(vec![Value::Int(start + i as i64), v]))
            .collect();
        Ok(Value::list(out))
    }));

    define(vm, "zip", simple("zip", None, |vm, args, _kw| {
        let mut sequences: Vec<Vec<Value>> = Vec::new();
        for a in args {
            sequences.push(vm.iterate(a)?);
        }
        let min = sequences.iter().map(|s| s.len()).min().unwrap_or(0);
        let mut out = Vec::new();
        for i in 0..min {
            out.push(Value::tuple(sequences.iter().map(|s| s[i].clone()).collect()));
        }
        Ok(Value::list(out))
    }));

    define(vm, "map", simple("map", None, |vm, args, _kw| {
        if args.len() < 2 {
            return Err(ZError::raised(
                "TypeError",
                "map() must have at least two arguments",
            ));
        }
        let f = args[0].clone();
        let src = vm.iterate(&args[1])?;
        let mut out = Vec::new();
        for v in src {
            out.push(vm.call_value(f.clone(), vec![v])?);
        }
        Ok(Value::list(out))
    }));

    define(vm, "filter", simple("filter", None, |vm, args, _kw| {
        if args.len() < 2 {
            return Err(ZError::raised(
                "TypeError",
                "filter() must have at least two arguments",
            ));
        }
        let f = args[0].clone();
        let src = vm.iterate(&args[1])?;
        let mut out = Vec::new();
        for v in src {
            let keep = vm.call_value(f.clone(), vec![v.clone()])?;
            if keep.is_truthy() {
                out.push(v);
            }
        }
        Ok(Value::list(out))
    }));

    define(vm, "any", simple("any", Some(1), |vm, args, _kw| {
        let items = vm.iterate(&args[0])?;
        Ok(Value::Bool(items.iter().any(|v| v.is_truthy())))
    }));

    define(vm, "all", simple("all", Some(1), |vm, args, _kw| {
        let items = vm.iterate(&args[0])?;
        Ok(Value::Bool(items.iter().all(|v| v.is_truthy())))
    }));
}

fn install_types(vm: &mut Vm) {
    define(vm, "type", simple("type", Some(1), |_vm, args, _kw| {
        Ok(Value::str(args[0].type_name()))
    }));

    define(vm, "isinstance", simple("isinstance", Some(2), |vm, args, _kw| {
        vm.isinstance(&args[0], &args[1])
    }));

    define(vm, "issubclass", simple("issubclass", Some(2), |vm, args, _kw| {
        vm.is_subclass(&args[0], &args[1])
    }));

    define(vm, "hasattr", simple("hasattr", Some(2), |vm, args, _kw| {
        let name = match &args[1] {
            Value::Str(s) => s.to_string(),
            other => {
                return Err(ZError::raised(
                    "TypeError",
                    format!("hasattr() attribute name must be a string, not '{}'", other.type_name()),
                ))
            }
        };
        Ok(Value::Bool(vm.has_attr(&args[0], &name)))
    }));

    define(vm, "getattr", simple("getattr", Some(2), |vm, args, _kw| {
        let name = match &args[1] {
            Value::Str(s) => s.to_string(),
            other => {
                return Err(ZError::raised(
                    "TypeError",
                    format!("getattr() attribute name must be a string, not '{}'", other.type_name()),
                ))
            }
        };
        match vm.attr_value(&args[0], &name) {
            Ok(v) => Ok(v),
            Err(_) => match args.get(2) {
                Some(d) => Ok(d.clone()),
                None => Err(ZError::raised(
                    "AttributeError",
                    format!("'{}' object has no attribute '{}'", args[0].type_name(), name),
                )),
            },
        }
    }));

    define(vm, "setattr", simple("setattr", Some(3), |vm, args, _kw| {
        let name = match &args[1] {
            Value::Str(s) => s.to_string(),
            _ => return Err(ZError::raised("TypeError", "attribute name must be a string")),
        };
        vm.set_attr(&args[0], &name, args[2].clone())?;
        Ok(Value::None)
    }));

    define(vm, "repr", simple("repr", Some(1), |vm, args, _kw| {
        Ok(Value::str(vm.repr_value(&args[0])))
    }));
}

fn install_misc(vm: &mut Vm) {
    define(vm, "input", simple("input", Some(1), |_vm, args: &[Value], _kw| {
        use std::io::Write;
        let prompt = match &args[0] {
            Value::Str(s) => (**s).clone(),
            _ => String::new(),
        };
        if !prompt.is_empty() {
            print!("{}", prompt);
            let _ = std::io::stdout().flush();
        }
        let mut line = String::new();
        match std::io::stdin().read_line(&mut line) {
            Ok(0) => Ok(Value::None),
            Ok(_) => {
                let trimmed = line.trim_end_matches(['\r', '\n']);
                Ok(Value::str(trimmed.to_string()))
            }
            Err(e) => Err(ZError::raised(
                "RuntimeError",
                format!("cannot read input: {}", e),
            )),
        }
    }));

    define(vm, "exit", simple("exit", Some(1), |_vm, args, _kw| {
        let code = args[0].as_float() as i32;
        std::process::exit(code);
    }));

    define(vm, "id", simple("id", Some(1), |_vm, args, _kw| {
        let bits = match &args[0] {
            Value::Int(i) => (*i as u64) << 1,
            Value::Float(f) => f.to_bits(),
            Value::Bool(b) => {
                if *b {
                    0x11
                } else {
                    0x10
                }
            }
            Value::None => 0,
            _ => args[0].type_name().len() as u64,
        };
        Ok(Value::Int(bits as i64))
    }));

    define(vm, "hash", simple("hash", Some(1), |_vm, args, _kw| {
        match args[0].as_key() {
            Some(k) => {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                k.hash(&mut h);
                Ok(Value::Int(h.finish() as i64))
            }
            None => Err(ZError::raised(
                "TypeError",
                format!("unhashable type: '{}'", args[0].type_name()),
            )),
        }
    }));

    define(vm, "assert", simple("assert", Some(1), |_vm, args, _kw| {
        if !args[0].is_truthy() {
            let msg = args
                .get(1)
                .map(|v| v.to_string())
                .unwrap_or_else(|| "assertion failed".to_string());
            return Err(ZError::raised("AssertionError", msg));
        }
        Ok(Value::None)
    }));

    define(vm, "callable", simple("callable", Some(1), |_vm, args, _kw| {
        let c = matches!(
            args[0],
            Value::Function(_) | Value::Builtin(_) | Value::Class(_) | Value::BoundMethod(_)
        );
        Ok(Value::Bool(c))
    }));

    define(vm, "ord", simple("ord", Some(1), |_vm, args, _kw| match &args[0] {
        Value::Str(s) => match s.chars().next() {
            Some(c) => Ok(Value::Int(c as i64)),
            None => Err(ZError::raised(
                "TypeError",
                "ord() expected a character",
            )),
        },
        other => Err(ZError::raised(
            "TypeError",
            format!("ord() expected a string of length 1, not '{}'", other.type_name()),
        )),
    }));

    define(vm, "chr", simple("chr", Some(1), |_vm, args: &[Value], _kw| {
        let n = args[0].as_float() as u32;
        match char::from_u32(n) {
            Some(c) => Ok(Value::str(c.to_string())),
            None => Err(ZError::raised(
                "ValueError",
                format!("chr() arg not in range(0x110000): {}", n),
            )),
        }
    }));
}

// ================= methods on built-in values =================

/// Dispatch `callclass recv.name(args)` for str / list / dict / set / tuple.
pub fn builtin_method(
    vm: &mut Vm,
    obj: &Value,
    name: &str,
    args: Vec<Value>,
) -> ZResult<Value> {
    if let Some(v) = string::string_method(vm, obj, name, args.clone())? {
        return Ok(v);
    }
    if let Some(v) = collections::list_method(vm, obj, name, args.clone())? {
        return Ok(v);
    }
    if let Some(v) = collections::dict_method(vm, obj, name, args.clone())? {
        return Ok(v);
    }
    if let Some(v) = collections::set_method(vm, obj, name, args)? {
        return Ok(v);
    }
    Err(ZError::raised(
        "AttributeError",
        format!("'{}' object has no method '{}'", obj.type_name(), name),
    ))
}

/// Attributes exposed on built-in values, e.g. `<builtin>.name`.
pub fn builtin_attr(obj: &Value, attr: &str) -> Option<Value> {
    match obj {
        Value::Builtin(b) => b.members.get(attr).cloned(),
        _ => None,
    }
}