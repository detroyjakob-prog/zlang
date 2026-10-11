//! Methods available on `str` values, called with `callclass`.

use crate::error::{ZError, ZResult};
use crate::value::Value;
use crate::vm::Vm;

fn attr_err(name: &str) -> ZError {
    ZError::raised(
        "AttributeError",
        format!("'str' object has no method '{}'", name),
    )
}

/// Returns `Ok(None)` when `obj` is not a string or `name` is not a string
/// method, so `builtin_method` can try other types.
pub fn string_method(
    vm: &mut Vm,
    obj: &Value,
    name: &str,
    args: Vec<Value>,
) -> ZResult<Option<Value>> {
    let s = match obj {
        Value::Str(s) => s.to_string(),
        _ => return Ok(None),
    };

    let result = match name {
        "upper" => Value::str(s.to_uppercase()),
        "lower" => Value::str(s.to_lowercase()),
        "strip" => match args.first() {
            Some(cut) => {
                let cut = vm.to_string_value(cut);
                Value::str(s.trim_matches(|c| cut.contains(c)).to_string())
            }
            None => Value::str(s.trim().to_string()),
        },
        "split" => match args.first() {
            Some(Value::None) | None => Value::list(
                s.split_whitespace()
                    .map(|p| Value::str(p.to_string()))
                    .collect(),
            ),
            Some(sep) => {
                let sep = vm.to_string_value(sep);
                if sep.is_empty() {
                    return Err(ZError::raised("ValueError", "empty separator"));
                }
                Value::list(s.split(&sep).map(|p| Value::str(p.to_string())).collect())
            }
        },
        "join" => {
            let items = vm.iterate(&args[0])?;
            let mut parts: Vec<String> = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    Value::Str(x) => parts.push((*x).clone()),
                    other => {
                        return Err(ZError::raised(
                            "TypeError",
                            format!(
                                "sequence item: expected str instance, {} found",
                                other.type_name()
                            ),
                        ))
                    }
                }
            }
            Value::str(parts.join(&s))
        }
        "replace" => {
            if args.len() < 2 {
                return Err(ZError::raised(
                    "TypeError",
                    "replace() expected at least 2 arguments",
                ));
            }
            let from = vm.to_string_value(&args[0]);
            let to = vm.to_string_value(&args[1]);
            let count = args.get(2).map(|v| v.as_float() as usize).unwrap_or(usize::MAX);
            let out = if count == usize::MAX {
                s.replace(&from, &to)
            } else {
                s.replacen(&from, &to, count)
            };
            Value::str(out)
        }
        "find" => {
            let needle = vm.to_string_value(&args[0]);
            match s.find(&needle) {
                Some(i) => Value::Int(i as i64),
                None => Value::Int(-1),
            }
        }
        "startswith" => {
            let needle = vm.to_string_value(&args[0]);
            Value::Bool(s.starts_with(&needle))
        }
        "endswith" => {
            let needle = vm.to_string_value(&args[0]);
            Value::Bool(s.ends_with(&needle))
        }
        "count" => {
            let needle = vm.to_string_value(&args[0]);
            Value::Int(s.matches(&needle).count() as i64)
        }
        "format" => format_string(vm, &s, &args)?,
        "title" => Value::str(title_case(&s)),
        "isdigit" => Value::Bool(!s.is_empty() && s.chars().all(|c| c.is_ascii_digit())),
        "isalpha" => Value::Bool(!s.is_empty() && s.chars().all(|c| c.is_alphabetic())),
        "isspace" => Value::Bool(!s.is_empty() && s.chars().all(|c| c.is_whitespace())),
        "isupper" => Value::Bool(s.chars().any(|c| c.is_uppercase()) && !s.chars().any(|c| c.is_lowercase())),
        "islower" => Value::Bool(s.chars().any(|c| c.is_lowercase()) && !s.chars().any(|c| c.is_uppercase())),
        other => return Err(attr_err(other)),
    };
    Ok(Some(result))
}

fn title_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut boundary = true;
    for c in s.chars() {
        if boundary && c.is_alphabetic() {
            out.extend(c.to_uppercase());
            boundary = false;
        } else {
            out.extend(c.to_lowercase());
            if !c.is_alphabetic() {
                boundary = true;
            }
        }
    }
    out
}

/// `str.format`, supporting `{}`, `{0}`, `{name}` placeholders.
fn format_string(vm: &mut Vm, template: &str, args: &[Value]) -> ZResult<Value> {
    let mut out = String::new();
    let mut auto = 0usize;
    let mut named: std::collections::HashMap<String, Value> = std::collections::HashMap::new();
    // Split trailing `k=v` keyword arguments out of the positional ones.
    let mut positional: Vec<Value> = Vec::new();
    for a in args {
        positional.push(a.clone());
    }

    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '{' if i + 1 < chars.len() && chars[i + 1] == '{' => {
                out.push('{');
                i += 2;
            }
            '}' if i + 1 < chars.len() && chars[i + 1] == '}' => {
                out.push('}');
                i += 2;
            }
            '{' => {
                let mut field = String::new();
                i += 1;
                while i < chars.len() && chars[i] != '}' {
                    field.push(chars[i]);
                    i += 1;
                }
                i += 1;
                let spec: Vec<&str> = field.splitn(2, ':').collect();
                let key = spec[0].trim().to_string();
                let value = if key.is_empty() {
                    let v = positional.get(auto).cloned().unwrap_or(Value::None);
                    auto += 1;
                    v
                } else if key.chars().all(|c| c.is_ascii_digit()) {
                    match key.parse::<usize>() {
                        Ok(n) => positional.get(n).cloned().unwrap_or(Value::None),
                        Err(_) => Value::None,
                    }
                } else {
                    // Last occurrence wins for duplicates; `None` if unknown.
                    match named.remove(&key) {
                        Some(v) => v,
                        None => Value::str(format!("{{{}}}", key)),
                    }
                };
                out.push_str(&vm.to_string_value(&value));
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    Ok(Value::str(out))
}