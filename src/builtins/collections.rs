//! Methods on collections: list, dict, set and tuple.
//!
//! Each dispatcher returns `Ok(None)` when the value is not of the type it
//! handles, so `builtin_method` can try list, dict and set in turn.

use crate::error::{ZError, ZResult};
use crate::value::{Map, Value, ValueKey};
use crate::vm::{values_equal, Vm};

fn type_err(msg: &str) -> ZError {
    ZError::raised("TypeError", msg)
}

pub fn list_method(
    vm: &mut Vm,
    obj: &Value,
    name: &str,
    args: Vec<Value>,
) -> ZResult<Option<Value>> {
    let items: Vec<Value> = match obj {
        Value::List(_) => match obj {
            Value::List(l) => (**l).clone(),
            _ => unreachable!(),
        },
        _ => return Ok(None),
    };

    match name {
        "append" => {
            if args.len() != 1 {
                return Err(type_err("append() takes exactly 1 argument"));
            }
            let mut out = items;
            out.push(args[0].clone());
            Ok(Some(vm.replace_container(obj, Value::list(out))?))
        }
        "extend" => {
            let more = vm.iterate(&args[0])?;
            let mut out = items;
            out.extend(more);
            Ok(Some(vm.replace_container(obj, Value::list(out))?))
        }
        "insert" => {
            if args.len() < 2 {
                return Err(type_err("insert() takes exactly 2 arguments"));
            }
            let mut out = items;
            let len = out.len() as i64;
            let mut idx = match &args[0] {
                Value::Int(i) => *i,
                Value::Float(f) => *f as i64,
                _ => 0,
            };
            if idx < 0 {
                idx += len;
            }
            let idx = idx.clamp(0, len) as usize;
            out.insert(idx, args[1].clone());
            Ok(Some(vm.replace_container(obj, Value::list(out))?))
        }
        "remove" => {
            let target = &args[0];
            match items.iter().position(|v| values_equal(v, target)) {
                Some(i) => {
                    let mut out = items;
                    out.remove(i);
                    Ok(Some(vm.replace_container(obj, Value::list(out))?))
                }
                None => Err(ZError::raised(
                    "ValueError",
                    "list.remove(x): x not in list",
                )),
            }
        }
        "pop" => {
            if items.is_empty() {
                return Err(ZError::raised("IndexError", "pop from empty list"));
            }
            let mut out = items;
            let idx = match args.first() {
                Some(v) => {
                    let mut i = match v {
                        Value::Int(i) => *i,
                        Value::Float(f) => *f as i64,
                        _ => 0,
                    };
                    let len = out.len() as i64;
                    if i < 0 {
                        i += len;
                    }
                    if i < 0 || i >= len {
                        return Err(ZError::raised("IndexError", "pop index out of range"));
                    }
                    i as usize
                }
                None => out.len() - 1,
            };
            let removed = out.remove(idx);
            vm.replace_container(obj, Value::list(out))?;
            Ok(Some(removed))
        }
        "clear" => {
            let r = vm.replace_container(obj, Value::list(Vec::new()))?;
            Ok(Some(r))
        }
        "copy" => Ok(Some(Value::list(items))),
        "index" => Ok(Some(Value::Int(
            items
                .iter()
                .position(|v| values_equal(v, &args[0]))
                .map(|i| i as i64)
                .unwrap_or(-1),
        ))),
        "count" => Ok(Some(Value::Int(
            items
                .iter()
                .filter(|v| values_equal(v, &args[0]))
                .count() as i64,
        ))),
        "sort" => {
            let mut out = vm.sort_values(items)?;
            if args.len() >= 2 && args[1].is_truthy() {
                out.reverse();
            }
            let r = vm.replace_container(obj, Value::list(out))?;
            Ok(Some(r))
        }
        "reverse" => {
            let mut out = items;
            out.reverse();
            Ok(Some(vm.replace_container(obj, Value::list(out))?))
        }
        _ => Ok(None),
    }
}

pub fn dict_method(
    vm: &mut Vm,
    obj: &Value,
    name: &str,
    args: Vec<Value>,
) -> ZResult<Option<Value>> {
    let map: Map = match obj {
        Value::Dict(d) => (**d).clone(),
        _ => return Ok(None),
    };

    let key_of = |v: &Value| -> ZResult<ValueKey> {
        v.as_key().ok_or_else(|| {
            ZError::raised("TypeError", "unhashable type used as a dict key")
        })
    };

    match name {
        "get" => {
            let first = args.first().cloned().unwrap_or(Value::None);
            let k = key_of(&first)?;
            Ok(Some(match map.get(&k) {
                Some(v) => v.clone(),
                None => args.get(1).cloned().unwrap_or(Value::None),
            }))
        }
        "keys" => Ok(Some(Value::list(
            map.keys().map(|k| k.to_value()).collect(),
        ))),
        "values" => Ok(Some(Value::list(map.values().cloned().collect()))),
        "items" => Ok(Some(Value::list(
            map.iter()
                .map(|(k, v)| Value::tuple(vec![k.to_value(), v.clone()]))
                .collect(),
        ))),
        "pop" => {
            let first = args.first().cloned().unwrap_or(Value::None);
            let k = key_of(&first)?;
            match map.get(&k) {
                Some(v) => {
                    let removed = v.clone();
                    let mut m = map.clone();
                    m.remove(&k);
                    vm.replace_container(obj, Value::dict(m))?;
                    Ok(Some(removed))
                }
                None => Ok(Some(args.get(1).cloned().unwrap_or(Value::None))),
            }
        }
        "update" => {
            let mut m = map.clone();
            merge_map(vm, &mut m, &args[0])?;
            let r = vm.replace_container(obj, Value::dict(m))?;
            Ok(Some(r))
        }
        "clear" => {
            let r = vm.replace_container(obj, Value::dict(Map::new()))?;
            Ok(Some(r))
        }
        "copy" => Ok(Some(Value::dict(map))),
        "setdefault" => {
            let first = args.first().cloned().unwrap_or(Value::None);
            let k = key_of(&first)?;
            if let Some(v) = map.get(&k) {
                Ok(Some(v.clone()))
            } else {
                let fallback = args.get(1).cloned().unwrap_or(Value::None);
                let mut m = map.clone();
                m.insert(k, fallback.clone());
                vm.replace_container(obj, Value::dict(m))?;
                Ok(Some(fallback))
            }
        }
        _ => Ok(None),
    }
}

fn merge_map(vm: &mut Vm, target: &mut Map, source: &Value) -> ZResult<()> {
    match source {
        Value::Dict(d) => {
            for (k, v) in d.iter() {
                target.insert(k.clone(), v.clone());
            }
            Ok(())
        }
        Value::List(l) | Value::Tuple(l) => {
            for item in l.iter() {
                let pair = vm.iterate(item)?;
                if pair.len() != 2 {
                    return Err(ZError::raised(
                        "ValueError",
                        "dictionary update sequence element has wrong length",
                    ));
                }
                if let Some(k) = pair[0].as_key() {
                    target.insert(k, pair[1].clone());
                }
            }
            Ok(())
        }
        other => Err(ZError::raised(
            "TypeError",
            format!("'{}' object is not iterable", other.type_name()),
        )),
    }
}

pub fn set_method(
    vm: &mut Vm,
    obj: &Value,
    name: &str,
    args: Vec<Value>,
) -> ZResult<Option<Value>> {
    let items: Vec<Value> = match obj {
        Value::Set(s) => (**s).clone(),
        _ => return Ok(None),
    };

    match name {
        "add" => {
            let mut out = items;
            if !out.iter().any(|o| values_equal(o, &args[0])) {
                out.push(args[0].clone());
            }
            let r = vm.replace_container(obj, Value::set(out))?;
            Ok(Some(r))
        }
        "remove" => match items.iter().position(|v| values_equal(v, &args[0])) {
            Some(i) => {
                let mut out = items;
                out.remove(i);
                let r = vm.replace_container(obj, Value::set(out))?;
                Ok(Some(r))
            }
            None => Err(ZError::raised("KeyError", vm.to_string_value(&args[0]))),
        },
        "union" => {
            let other = vm.iterate(&args[0])?;
            let mut out = items;
            for v in other {
                if !out.iter().any(|o| values_equal(o, &v)) {
                    out.push(v);
                }
            }
            Ok(Some(Value::set(out)))
        }
        "intersection" => {
            let other = vm.iterate(&args[0])?;
            let out: Vec<Value> = items
                .into_iter()
                .filter(|v| other.iter().any(|o| values_equal(o, v)))
                .collect();
            Ok(Some(Value::set(out)))
        }
        "difference" => {
            let other = vm.iterate(&args[0])?;
            let out: Vec<Value> = items
                .into_iter()
                .filter(|v| !other.iter().any(|o| values_equal(o, v)))
                .collect();
            Ok(Some(Value::set(out)))
        }
        "clear" => {
            let r = vm.replace_container(obj, Value::set(Vec::new()))?;
            Ok(Some(r))
        }
        "copy" => Ok(Some(Value::set(items))),
        _ => Ok(None),
    }
}
